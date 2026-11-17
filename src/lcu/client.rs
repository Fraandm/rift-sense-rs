use chrono::Utc;
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
            .map_err(|error| format!("Cliente no accesible: {error}"))?;
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
            .filter(|value| value != "None")
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
