use chrono::Utc;
use serde_json::{Value, json};

use crate::data::norm_name;
use crate::engine::{Engine, ItemRef};
use crate::mode::GameMode;

const ROLES: [&str; 5] = ["TOP", "JUNGLE", "MID", "BOT", "SUPPORT"];

fn role_map(value: &str) -> Option<&'static str> {
    match value.trim().to_lowercase().replace('_', "").as_str() {
        "top" => Some("TOP"),
        "jungle" | "jgl" | "jungler" => Some("JUNGLE"),
        "middle" | "mid" => Some("MID"),
        "bottom" | "bot" | "adc" => Some("BOT"),
        "utility" | "support" | "sup" => Some("SUPPORT"),
        _ => None,
    }
}

fn role_index(value: Option<&str>) -> Option<usize> {
    let role = role_map(value?)?;
    ROLES.iter().position(|candidate| *candidate == role)
}

fn champion_from_id(engine: &Engine, value: Option<&Value>) -> Option<String> {
    let value = value?;
    let id = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))?;
    if id <= 0 {
        return None;
    }
    engine
        .champ_by_key
        .get(&id)
        .map(|index| engine.champions[*index].name.clone())
}

fn canonical_champion(engine: &Engine, value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.trim().is_empty() {
        return None;
    }
    engine
        .champ_by_norm
        .get(&norm_name(value))
        .map(|index| engine.champions[*index].name.clone())
}

fn infer_role(engine: &Engine, champion: Option<&str>) -> String {
    let Some(champion) = champion else {
        return "MID".to_string();
    };
    let Some(data) = engine.priors.champions.get(champion) else {
        return "MID".to_string();
    };
    let best = data
        .roles
        .iter()
        .max_by_key(|(_, value)| value.n)
        .map(|(role, _)| role.clone());
    match best {
        Some(role) if ROLES.contains(&role.as_str()) => role,
        _ => "MID".to_string(),
    }
}

fn first_empty(team: &[Option<String>]) -> Option<usize> {
    team.iter().position(Option::is_none)
}

fn fill_team_by_id(engine: &Engine, players: &[Value], position_key: &str) -> Vec<Option<String>> {
    let mut team: Vec<Option<String>> = vec![None; 5];
    for player in players {
        let Some(player) = player.as_object() else {
            continue;
        };
        let champion = champion_from_id(engine, player.get("championId"))
            .or_else(|| champion_from_id(engine, player.get("championPickIntent")));
        let Some(champion) = champion else {
            continue;
        };
        if team
            .iter()
            .any(|value| value.as_deref() == Some(champion.as_str()))
        {
            continue;
        }
        let mut index = player
            .get(position_key)
            .and_then(Value::as_str)
            .and_then(|value| role_index(Some(value)));
        if index.is_none() {
            index = player
                .get("cellId")
                .and_then(Value::as_i64)
                .map(|cell| (cell % 5) as usize);
        }
        let index = match index {
            Some(index) if index < 5 && team[index].is_none() => Some(index),
            _ => first_empty(&team),
        };
        if let Some(index) = index {
            team[index] = Some(champion);
        }
    }
    team
}

fn fill_team_by_name(engine: &Engine, players: &[Value]) -> Vec<Option<String>> {
    fill_team_by_name_limited(engine, players, 5)
}

fn fill_team_by_name_limited(
    engine: &Engine,
    players: &[Value],
    limit: usize,
) -> Vec<Option<String>> {
    let mut team: Vec<Option<String>> = vec![None; limit];
    for player in players {
        let Some(player) = player.as_object() else {
            continue;
        };
        let champion =
            canonical_champion(engine, player.get("championName").and_then(Value::as_str));
        let Some(champion) = champion else {
            continue;
        };
        if team
            .iter()
            .any(|value| value.as_deref() == Some(champion.as_str()))
        {
            continue;
        }
        let mut index = player
            .get("position")
            .and_then(Value::as_str)
            .and_then(|value| role_index(Some(value)));
        if index.is_none_or(|index| index >= team.len()) {
            index = first_empty(&team);
        }
        if let Some(mut index) = index {
            if team.get(index).is_some_and(Option::is_some) {
                index = first_empty(&team).unwrap_or(index);
            }
            if index < team.len() {
                team[index] = Some(champion);
            }
        }
    }
    team
}

fn value_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

fn team_side(my_team: &[Value], gameflow: Option<&Value>) -> String {
    for player in my_team {
        let team = player.get("team").and_then(Value::as_i64);
        match team {
            Some(1 | 100) => return "blue".to_string(),
            Some(2 | 200) => return "red".to_string(),
            _ => {}
        }
    }
    let mut my_ids = std::collections::HashSet::new();
    for player in my_team {
        for key in ["summonerId", "puuid"] {
            if let Some(value) = player.get(key) {
                my_ids.insert(value_key(value));
            }
        }
    }
    if my_ids.is_empty() {
        return "blue".to_string();
    }
    let game_data = gameflow
        .and_then(|value| value.get("gameData"))
        .cloned()
        .unwrap_or(Value::Null);
    let ids_from = |key: &str| {
        let mut result = std::collections::HashSet::new();
        if let Some(values) = game_data.get(key).and_then(Value::as_array) {
            for value in values {
                if let Some(object) = value.as_object() {
                    for field in ["summonerId", "puuid"] {
                        if let Some(value) = object.get(field) {
                            result.insert(value_key(value));
                        }
                    }
                } else if !value.is_null() {
                    result.insert(value_key(value));
                }
            }
        }
        result
    };
    let team_one = ids_from("teamOne");
    let team_two = ids_from("teamTwo");
    let in_one = my_ids.intersection(&team_one).next().is_some();
    let in_two = my_ids.intersection(&team_two).next().is_some();
    if in_two && !in_one {
        "red".to_string()
    } else {
        "blue".to_string()
    }
}

fn map_bans(engine: &Engine, session: &Value) -> Vec<String> {
    let mut raw = Vec::new();
    if let Some(bans) = session.get("bans").and_then(Value::as_object) {
        for key in ["myTeamBans", "theirTeamBans"] {
            if let Some(values) = bans.get(key).and_then(Value::as_array) {
                raw.extend(values.iter().cloned());
            }
        }
    }
    if let Some(actions) = session.get("actions").and_then(Value::as_array) {
        for group in actions {
            let Some(group) = group.as_array() else {
                continue;
            };
            for action in group {
                if action.get("type").and_then(Value::as_str) == Some("ban")
                    && action.get("completed").and_then(Value::as_bool) == Some(true)
                {
                    if let Some(id) = action.get("championId") {
                        raw.push(id.clone());
                    }
                }
            }
        }
    }
    let mut names = Vec::new();
    for id in raw {
        if let Some(name) = champion_from_id(engine, Some(&id)) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.truncate(10);
    names
}

fn timer_seconds(session: &Value) -> Option<f64> {
    let timer = session.get("timer")?;
    let value = timer
        .get("adjustedTimeLeftInPhase")
        .or_else(|| timer.get("timeLeftInPhase"))?;
    let number = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))?;
    let seconds = if number > 1000.0 {
        number / 1000.0
    } else {
        number
    };
    Some((seconds * 10.0).round() / 10.0)
}

fn place_champion(
    engine: &Engine,
    team: &mut [Option<String>],
    champion: Option<String>,
    role: &str,
) {
    let Some(champion) = champion else {
        return;
    };
    let role = if ROLES.contains(&role) {
        role.to_string()
    } else {
        infer_role(engine, Some(&champion))
    };
    let Some(index) = role_index(Some(&role)) else {
        return;
    };
    for (i, value) in team.iter_mut().enumerate() {
        if value.as_deref() == Some(champion.as_str()) && i != index {
            *value = None;
        }
    }
    team[index] = Some(champion);
}

fn names(team: &[Option<String>]) -> Vec<Value> {
    team.iter()
        .map(|value| value.clone().map(Value::String).unwrap_or(Value::Null))
        .collect()
}

pub fn map_champ_select(
    engine: &Engine,
    session: &Value,
    gameflow: Option<&Value>,
    status: &Value,
) -> Value {
    if !session.is_object() {
        return json!({"available": false, "error": "Sesion de champ select no disponible."});
    }
    let my_team = session
        .get("myTeam")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let their_team = session
        .get("theirTeam")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if my_team.is_empty() {
        return json!({"available": false, "error": "El champ select no tiene equipo propio."});
    }
    let local_cell = session
        .get("localPlayerCellId")
        .cloned()
        .unwrap_or(Value::Null);
    let me = my_team
        .iter()
        .find(|player| {
            local_cell
                .as_i64()
                .zip(player.get("cellId").and_then(Value::as_i64))
                .map(|(a, b)| a == b)
                .unwrap_or(false)
        })
        .or_else(|| my_team.first())
        .cloned()
        .unwrap_or(Value::Null);
    let my_champion = champion_from_id(engine, me.get("championId"))
        .or_else(|| champion_from_id(engine, me.get("championPickIntent")));
    let mut role = me
        .get("assignedPosition")
        .and_then(Value::as_str)
        .and_then(role_map)
        .map(str::to_string);
    if role.is_none() {
        role = Some(infer_role(engine, my_champion.as_deref()));
    }
    let role = role.unwrap_or_else(|| "MID".to_string());
    let mut allies = fill_team_by_id(engine, &my_team, "assignedPosition");
    let enemies = fill_team_by_id(engine, &their_team, "assignedPosition");
    place_champion(engine, &mut allies, my_champion.clone(), &role);

    let queue_id = gameflow
        .and_then(|value| value.get("gameData"))
        .and_then(|value| value.get("queue"))
        .and_then(|value| value.get("queueId"))
        .and_then(Value::as_i64)
        .or_else(|| {
            session
                .get("gameData")
                .and_then(|value| value.get("queue"))
                .and_then(|value| value.get("queueId"))
                .and_then(Value::as_i64)
        });
    let mode = GameMode::resolve(
        None,
        gameflow
            .and_then(|value| value.get("gameData"))
            .and_then(|value| value.get("gameMode"))
            .and_then(Value::as_str),
        gameflow
            .and_then(|value| value.get("gameData"))
            .and_then(|value| value.get("map"))
            .and_then(Value::as_str),
        queue_id,
    );

    json!({
        "available": true,
        "source": "lcu",
        "phase": session.get("timer").and_then(|timer| timer.get("phase")).cloned().or_else(|| status.get("phase").cloned()),
        "side": team_side(&my_team, gameflow),
        "role": role,
        "allies": names(&allies),
        "enemies": names(&enemies),
        "bans": map_bans(engine, session),
        "myChampion": my_champion,
        "localPlayerCellId": local_cell,
        "timerSeconds": timer_seconds(session),
        "queueId": queue_id,
        "gameMode": mode.key(),
        "modeLabel": mode.label(),
        "summoner": status.get("summoner"),
        "updatedAt": Utc::now().to_rfc3339(),
    })
}

fn same_player(active_name: &str, player: &Value) -> bool {
    let candidates = [
        player.get("summonerName").and_then(Value::as_str),
        player.get("riotId").and_then(Value::as_str),
        player.get("riotIdGameName").and_then(Value::as_str),
    ];
    let active_base = active_name
        .split('#')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    candidates.into_iter().flatten().any(|candidate| {
        candidate.trim() == active_name
            || candidate
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase()
                == active_base
    })
}

pub fn map_live_gamedata(engine: &Engine, data: &Value) -> Value {
    if !data.is_object() {
        return json!({"available": false, "error": "Live Client Data no disponible."});
    }
    let active = data.get("activePlayer").cloned().unwrap_or(Value::Null);
    let players = data
        .get("allPlayers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let active_name = active
        .get("summonerName")
        .or_else(|| active.get("riotIdGameName"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if active_name.is_empty() {
        return json!({"available": false, "error": "No se pudo identificar al jugador activo."});
    }
    let Some(me) = players
        .iter()
        .find(|player| same_player(&active_name, player))
    else {
        return json!({"available": false, "error": "El jugador activo no aparece en la lista de jugadores."});
    };
    let raw_champion = me.get("championName").and_then(Value::as_str);
    let Some(champion) = canonical_champion(engine, raw_champion) else {
        return json!({
            "available": false,
            "error": format!("Campeón no reconocido: {}", raw_champion.unwrap_or("")),
        });
    };
    let game_data = data.get("gameData").cloned().unwrap_or(Value::Null);
    let queue_id = game_data.get("queueId").and_then(Value::as_i64);
    let mode = GameMode::resolve(
        None,
        game_data.get("gameMode").and_then(Value::as_str),
        game_data.get("mapName").and_then(Value::as_str),
        queue_id,
    );
    let role = role_index(me.get("position").and_then(Value::as_str))
        .map(|index| ROLES[index].to_string())
        .unwrap_or_else(|| infer_role(engine, Some(&champion)));
    let team = me
        .get("team")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_uppercase();
    let side = if team == "CHAOS" { "red" } else { "blue" };

    let subteam_id = ["playerSubteamId", "subteamId", "teamId"]
        .iter()
        .find_map(|key| me.get(*key).and_then(Value::as_i64));
    let mut team_players = Vec::new();
    let mut enemy_players = Vec::new();
    let mut arena_team_known = false;
    if mode == GameMode::Arena {
        for player in &players {
            if same_player(&active_name, player) {
                continue;
            }
            if let Some(active_subteam) = subteam_id {
                let player_subteam = ["playerSubteamId", "subteamId", "teamId"]
                    .iter()
                    .find_map(|key| player.get(*key).and_then(Value::as_i64));
                if player_subteam == Some(active_subteam) {
                    arena_team_known = true;
                    team_players.push(player.clone());
                    continue;
                }
            }
            enemy_players.push(player.clone());
        }
    } else {
        for player in &players {
            let player_team = player
                .get("team")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_uppercase();
            if player_team == team {
                team_players.push(player.clone());
            } else {
                enemy_players.push(player.clone());
            }
        }
    }

    let mut allies = fill_team_by_name(engine, &team_players);
    if mode.is_classic() {
        place_champion(engine, &mut allies, Some(champion.clone()), &role);
    } else {
        allies.retain(|value| value.as_deref() != Some(champion.as_str()));
        allies.insert(0, Some(champion.clone()));
        allies.truncate(5);
    }
    let mut enemies = fill_team_by_name_limited(engine, &enemy_players, mode.max_enemies());
    while enemies.last().is_some_and(Option::is_none) {
        enemies.pop();
    }

    let mut raw_items: Vec<Value> = me
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            let id = item.get("itemID").and_then(Value::as_i64).unwrap_or(0);
            id > 0
                && engine
                    .resolve_item(&ItemRef::Int(id))
                    .is_some_and(|item| engine.is_tracked_item(item))
        })
        .collect();
    raw_items.sort_by_key(|item| item.get("slot").and_then(Value::as_i64).unwrap_or(0));
    let items: Vec<Value> = raw_items
        .iter()
        .map(|item| {
            json!({
                "id": item.get("itemID").and_then(Value::as_i64).unwrap_or(0),
                "name": item.get("displayName").and_then(Value::as_str).unwrap_or(""),
                "price": item.get("price").and_then(Value::as_i64).unwrap_or(0),
                "slot": item.get("slot").and_then(Value::as_i64).unwrap_or(0),
            })
        })
        .collect();
    let owned: Vec<i64> = items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_i64))
        .filter(|id| engine.resolve_item(&ItemRef::Int(*id)).is_some())
        .collect();

    let game_time = game_data
        .get("gameTime")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let scores = me.get("scores").cloned().unwrap_or(Value::Null);
    let score = |key: &str| scores.get(key).and_then(Value::as_i64).unwrap_or(0);
    let player_score = |player: &Value, key: &str| {
        player
            .get("scores")
            .and_then(|scores| scores.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let team_stats = |players: &[Value]| {
        let kills = players
            .iter()
            .map(|player| player_score(player, "kills"))
            .sum::<i64>();
        let cs = players
            .iter()
            .map(|player| player_score(player, "creepScore"))
            .sum::<i64>();
        let level_sum = players
            .iter()
            .map(|player| player.get("level").and_then(Value::as_i64).unwrap_or(1))
            .sum::<i64>();
        let level = if players.is_empty() {
            0.0
        } else {
            level_sum as f64 / players.len() as f64
        };
        (kills, cs, level)
    };
    let (team_kills, team_cs, team_level, enemy_kills, enemy_cs, enemy_level) =
        if mode == GameMode::Arena {
            (0, 0, 0.0, 0, 0, 0.0)
        } else {
            let (team_kills, team_cs, team_level) = team_stats(&team_players);
            let (enemy_kills, enemy_cs, enemy_level) = team_stats(&enemy_players);
            (
                team_kills,
                team_cs,
                team_level,
                enemy_kills,
                enemy_cs,
                enemy_level,
            )
        };
    let mut fed: Vec<(f64, String)> = enemy_players
        .iter()
        .filter_map(|player| {
            let name =
                canonical_champion(engine, player.get("championName").and_then(Value::as_str))?;
            let kills = player_score(player, "kills");
            let deaths = player_score(player, "deaths");
            let cs = player_score(player, "creepScore");
            let level = player.get("level").and_then(Value::as_i64).unwrap_or(1);
            let score =
                kills as f64 * 2.0 + cs as f64 / 100.0 + level as f64 * 0.5 - deaths as f64 * 0.2;
            if kills >= 3 || score >= 7.0 {
                Some((score, name))
            } else {
                None
            }
        })
        .collect();
    fed.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let enemy_fed: Vec<String> = fed.into_iter().take(2).map(|(_, name)| name).collect();
    let minute = game_time / 60.0;
    let cs = score("creepScore");
    let kill_participation = if team_kills > 0 {
        ((score("kills") + score("assists")) as f64 / team_kills as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    json!({
        "available": true,
        "source": "liveclientdata",
        "champion": champion,
        "role": role,
        "side": side,
        "allies": names(&allies),
        "enemies": names(&enemies),
        "bans": [],
        "owned": owned,
        "items": items,
        "gold": active.get("currentGold").and_then(Value::as_f64).unwrap_or(0.0) as i64,
        "minute": (game_time / 60.0 * 10.0).round() / 10.0,
        "gameTime": (game_time * 10.0).round() / 10.0,
        "level": me.get("level").and_then(Value::as_i64).unwrap_or(0),
        "scores": {
            "kills": score("kills"),
            "deaths": score("deaths"),
            "assists": score("assists"),
            "cs": cs,
            "wards": score("wardScore"),
        },
        "kills": score("kills"),
        "deaths": score("deaths"),
        "assists": score("assists"),
        "cs": cs,
        "csPerMin": if minute > 0.0 { (cs as f64 / minute * 10.0).round() / 10.0 } else { 0.0 },
        "killParticipation": (kill_participation * 1000.0).round() / 1000.0,
        "teamKills": team_kills,
        "enemyKills": enemy_kills,
        "teamCs": team_cs,
        "enemyCs": enemy_cs,
        "teamLevel": (team_level * 10.0).round() / 10.0,
        "enemyLevel": (enemy_level * 10.0).round() / 10.0,
        "enemyFed": enemy_fed,
        "team": team,
        "gameMode": game_data.get("gameMode"),
        "mapName": game_data.get("mapName"),
        "queueId": queue_id,
        "mode": mode.key(),
        "modeLabel": mode.label(),
        "allPlayersCount": players.len(),
        "arenaTeamKnown": arena_team_known,
        "summoner": active_name,
        "updatedAt": Utc::now().to_rfc3339(),
    })
}
