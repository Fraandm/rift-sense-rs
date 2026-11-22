use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

use super::locator::Locator;

const LIVE_URLS: [&str; 2] = [
    "https://127.0.0.1:2999/liveclientdata/allgamedata",
    "http://127.0.0.1:2999/liveclientdata/allgamedata",
];

pub struct LcuClient {
    client: Client,
    live_client: Client,
    locator: Arc<Locator>,
}

impl LcuClient {
    pub fn new(locator: Arc<Locator>) -> anyhow::Result<Self> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()?;
        let live_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        Ok(Self {
            client,
            live_client,
            locator,
        })
    }

    async fn get_json(&self, path: &str, timeout: Duration) -> Result<Option<Value>, String> {
        let Some(info) = self.locator.lockfile() else {
            return Ok(None);
        };
        let url = format!("{}://127.0.0.1:{}{}", info.protocol, info.port, path);
        let response = self
            .client
            .get(&url)
            .basic_auth("riot", Some(&info.password))
            .timeout(timeout)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!("LCU {path} fallo: {error:#}");
                format!("Cliente no accesible: {error}")
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("El cliente rechazó las credenciales del lockfile.".to_string());
        }
        if status.is_server_error() {
            return Err(format!("LCU devolvio HTTP {status}."));
        }
        if !status.is_success() {
            return Err(format!("LCU devolvio HTTP {status}."));
        }
        let text = response
            .text()
            .await
            .map_err(|error| format!("Respuesta LCU invalida: {error}"))?;
        if text.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| format!("Respuesta LCU invalida: {error}"))
    }

    pub async fn phase(&self) -> Option<String> {
        self.get_json("/lol-gameflow/v1/gameflow-phase", Duration::from_secs(2))
            .await
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
        // "None" es una fase válida: el cliente está abierto pero en el lobby.
    }

    pub async fn current_summoner(&self) -> Option<Value> {
        self.get_json("/lol-summoner/v1/current-summoner", Duration::from_secs(2))
            .await
            .ok()
            .flatten()
            .filter(Value::is_object)
    }

    pub async fn champ_select_session(&self) -> Option<Value> {
        self.get_json("/lol-champ-select/v1/session", Duration::from_secs(2))
            .await
            .ok()
            .flatten()
            .filter(Value::is_object)
    }

    pub async fn gameflow_session(&self) -> Option<Value> {
        self.get_json("/lol-gameflow/v1/session", Duration::from_secs(2))
            .await
            .ok()
            .flatten()
            .filter(Value::is_object)
    }

    pub async fn live_gamedata(&self, timeout: Duration) -> Option<Value> {
        for url in LIVE_URLS {
            let Ok(response) = self.live_client.get(url).timeout(timeout).send().await else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let Ok(data) = response.json::<Value>().await else {
                continue;
            };
            if data.is_object() {
                return Some(data);
            }
        }
        None
    }

    /// Ultima partida terminada, con resultado, inventario final y KDA.
    ///
    /// El Live Client desaparece al acabar la partida; el historial local del
    /// LCU sigue disponible y no necesita API key.
    pub async fn postgame_summary(&self) -> Option<Value> {
        let summoner = self.current_summoner().await.unwrap_or(Value::Null);
        let puuid = summoner
            .get("puuid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let summoner_id = summoner
            .get("summonerId")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let mut game_id = self.gameflow_session().await.and_then(|session| {
            session
                .get("gameData")
                .and_then(|data| data.get("gameId"))
                .and_then(|value| {
                    value
                        .as_i64()
                        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
                })
        });
        if game_id.is_none() {
            let recent = self
                .get_json(
                    "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=3",
                    Duration::from_secs(4),
                )
                .await
                .ok()
                .flatten();
            game_id = recent.as_ref().and_then(recent_game_id);
        }
        let game_id = game_id?;
        let game = self
            .get_json(
                &format!("/lol-match-history/v1/games/{game_id}"),
                Duration::from_secs(4),
            )
            .await
            .ok()
            .flatten()?;
        Some(parse_postgame(&game, &puuid, summoner_id, game_id))
    }

    pub async fn status(&self) -> Value {
        let phase = self.phase().await;
        let live = if matches!(
            phase.as_deref(),
            Some("InProgress" | "Reconnect" | "WaitingForStats")
        ) {
            self.live_gamedata(Duration::from_millis(1200)).await
        } else if phase.is_none() {
            self.live_gamedata(Duration::from_millis(350)).await
        } else {
            None
        };
        let summoner = if phase.is_some() {
            self.current_summoner().await
        } else {
            None
        };
        let connected = phase.is_some();
        let search = self.locator.status_json(false);
        let lockfile_path = search.get("lockfilePath").cloned().unwrap_or(Value::Null);
        let install_path = search.get("installPath").cloned().unwrap_or(Value::Null);
        let drive = search.get("drive").cloned().unwrap_or(Value::Null);

        let display_name = summoner.as_ref().and_then(|summoner| {
            let game_name = summoner
                .get("gameName")
                .or_else(|| summoner.get("displayName"))
                .and_then(Value::as_str);
            let tag = summoner.get("tagLine").and_then(Value::as_str);
            match (game_name, tag) {
                (Some(name), Some(tag)) => Some(format!("{name}#{tag}")),
                (Some(name), None) => Some(name.to_string()),
                _ => None,
            }
        });
        let summoner_level = summoner
            .as_ref()
            .and_then(|summoner| summoner.get("summonerLevel"))
            .cloned()
            .unwrap_or(Value::Null);

        let error = if connected || live.is_some() {
            Value::Null
        } else if let Some(drive) = drive.as_str() {
            json!(format!(
                "Cliente encontrado en el disco {drive} pero está cerrado. Abre League of Legends."
            ))
        } else if search.get("searching").and_then(Value::as_bool) == Some(true) {
            json!("Buscando League of Legends en los discos...")
        } else {
            json!("No se encontró el cliente de LoL. Revisa la instalación o fija la ruta.")
        };

        let effective_phase = if live.is_some() && phase.is_none() {
            Some("InProgress".to_string())
        } else {
            phase.clone()
        };
        json!({
            "connected": connected,
            "phase": effective_phase,
            "summoner": display_name,
            "summonerLevel": summoner_level,
            "lockfilePath": lockfile_path,
            "installPath": install_path,
            "drive": drive,
            "search": search,
            "liveClient": live.is_some(),
            "inGame": phase.as_deref() == Some("InProgress") || live.is_some(),
            "champSelect": phase.as_deref() == Some("ChampSelect"),
            "error": error,
            "checkedAt": Utc::now().to_rfc3339(),
        })
    }
}

fn recent_game_id(value: &Value) -> Option<i64> {
    let games = value
        .get("games")
        .and_then(|games| games.get("games").or(Some(games)))
        .and_then(Value::as_array)
        .or_else(|| value.get("games").and_then(Value::as_array))
        .or_else(|| value.as_array());
    games?.iter().find_map(|game| {
        game.get("gameId")
            .and_then(Value::as_i64)
            .or_else(|| game.get("gameId").and_then(Value::as_str)?.parse().ok())
    })
}

fn number(value: Option<&Value>) -> f64 {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(0.0)
}

fn integer(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(0)
}

fn parse_postgame(game: &Value, puuid: &str, summoner_id: i64, game_id: i64) -> Value {
    let participants = game
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let identities = game
        .get("participantIdentities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let participant_id = identities.iter().find_map(|identity| {
        let player = identity.get("player")?;
        let same_puuid = player
            .get("puuid")
            .and_then(Value::as_str)
            .is_some_and(|value| !puuid.is_empty() && value == puuid);
        let same_id = player
            .get("summonerId")
            .and_then(Value::as_i64)
            .is_some_and(|value| summoner_id != 0 && value == summoner_id);
        (same_puuid || same_id)
            .then(|| identity.get("participantId").and_then(Value::as_i64))
            .flatten()
    });
    let participant = participant_id
        .and_then(|id| {
            participants.iter().find(|participant| {
                participant.get("participantId").and_then(Value::as_i64) == Some(id)
            })
        })
        .or_else(|| participants.first());
    let Some(participant) = participant else {
        return json!({
            "available": false,
            "gameId": game_id,
            "message": "La partida no incluye participantes.",
        });
    };
    let stats = participant.get("stats").unwrap_or(&Value::Null);
    let items: Vec<i64> = (0..7)
        .filter_map(|slot| {
            stats
                .get(format!("item{slot}"))
                .and_then(Value::as_i64)
                .filter(|id| *id > 0)
        })
        .collect();
    let augments: Vec<i64> = (1..=4)
        .filter_map(|slot| {
            stats
                .get(format!("playerAugment{slot}"))
                .and_then(Value::as_i64)
                .filter(|id| *id > 0)
        })
        .collect();
    let win = stats.get("win").and_then(Value::as_bool);
    let finished_at = game
        .get("gameCreation")
        .and_then(Value::as_i64)
        .and_then(|created| {
            let created_ms = if created > 1_000_000_000_000 {
                created
            } else if created > 1_000_000_000 {
                created * 1000
            } else {
                return None;
            };
            Some(created_ms)
        });
    let duration = {
        let seconds = number(game.get("gameDuration"));
        let legacy = number(game.get("gameLength"));
        if seconds > 0.0 {
            if seconds > 100_000.0 {
                seconds / 1000.0
            } else {
                seconds
            }
        } else if legacy > 100_000.0 {
            legacy / 1000.0
        } else {
            legacy
        }
    };
    let finished_at = finished_at
        .and_then(|created_ms| {
            DateTime::<Utc>::from_timestamp_millis(created_ms + (duration * 1000.0) as i64)
        })
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    json!({
        "available": true,
        "source": "match-history",
        "gameId": game_id,
        "gameMode": game.get("gameMode").cloned().unwrap_or(Value::Null),
        "queueId": game.get("queueId").cloned().unwrap_or(Value::Null),
        "durationSeconds": (duration * 10.0).round() / 10.0,
        "result": win.map(|win| if win { "win" } else { "loss" }),
        "win": win,
        "championId": participant.get("championId").cloned().unwrap_or(Value::Null),
        "teamId": participant.get("teamId").cloned().unwrap_or(Value::Null),
        "items": items,
        "augmentIds": augments,
        "kills": integer(stats.get("kills")),
        "deaths": integer(stats.get("deaths")),
        "assists": integer(stats.get("assists")),
        "gold": integer(stats.get("goldEarned")),
        "cs": integer(stats.get("totalMinionsKilled")),
        "level": integer(stats.get("champLevel")),
        "finishedAt": finished_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_match_history_for_local_player() {
        let game = json!({
            "gameId": 123456,
            "gameCreation": 1764511757000_i64,
            "gameMode": "CLASSIC",
            "gameDuration": 1800,
            "queueId": 420,
            "participants": [
                {
                    "participantId": 1,
                    "teamId": 100,
                    "championId": 103,
                    "stats": {
                        "win": true,
                        "item0": 6655,
                        "item1": 3020,
                        "item2": 0,
                        "kills": 4,
                        "deaths": 2,
                        "assists": 5,
                        "goldEarned": 12000,
                        "totalMinionsKilled": 200,
                        "champLevel": 16,
                        "playerAugment1": 0
                    }
                }
            ],
            "participantIdentities": [
                {
                    "participantId": 1,
                    "player": {"puuid": "local-puuid", "summonerId": 42}
                }
            ]
        });
        let parsed = parse_postgame(&game, "local-puuid", 42, 123456);
        assert_eq!(parsed["available"], true);
        assert_eq!(parsed["result"], "win");
        assert_eq!(parsed["championId"], 103);
        assert_eq!(parsed["items"], json!([6655, 3020]));
        assert_eq!(parsed["kills"], 4);
        assert_eq!(parsed["durationSeconds"], 1800.0);
        assert_eq!(parsed["finishedAt"], "2025-11-30T14:39:17+00:00");
    }

    #[test]
    fn recent_match_ids_accept_both_wrappers() {
        assert_eq!(
            recent_game_id(&json!({"games": {"games": [{"gameId": 7}]}})),
            Some(7)
        );
        assert_eq!(
            recent_game_id(&json!({"games": [{"gameId": "8"}]})),
            Some(8)
        );
    }
}
