use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

use crate::config::Paths;
use crate::engine::{BansRequest, BuildRequest, ChampionRef, DraftRequest, Engine, LiveRequest};
use crate::lcu::LcuService;
use crate::lcu::mapper::{map_champ_select, map_live_gamedata};
use crate::settings::{self, Settings, SettingsHandle};

#[derive(RustEmbed)]
#[folder = "web"]
struct WebAssets;

pub struct AppState {
    pub engine: RwLock<Arc<Engine>>,
    pub paths: Paths,
    pub lcu: LcuService,
    pub settings: SettingsHandle,
    pub overlay_snapshot: RwLock<Option<(std::time::Instant, Value)>>,
}

type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/meta", get(meta))
        .route("/api/meta/champions", get(meta_champions))
        .route("/api/draft", post(draft))
        .route("/api/draft/bans", post(draft_bans))
        .route("/api/build", post(build))
        .route("/api/live", post(live))
        .route(
            "/api/overlay/snapshot",
            get(overlay_snapshot).post(publish_overlay_snapshot),
        )
        .route("/api/dataset", get(dataset))
        .route("/api/model", get(model))
        .route("/api/reload", post(reload))
        .route("/api/update/check", get(update_check))
        .route("/api/update/install", post(update_install))
        .route("/api/settings", get(settings_get).post(settings_update))
        .route("/api/diagnostics", get(diagnostics))
        .route("/api/diagnostics/open-logs", post(open_logs))
        .route("/api/client/status", get(client_status))
        .route("/api/client/draft", get(client_draft))
        .route("/api/client/live", get(client_live))
        .route("/api/client/postgame", get(client_postgame))
        .route(
            "/api/client/search",
            get(client_search_status).post(client_search),
        )
        .route("/api/client/path", post(client_path))
        .fallback(static_asset)
        .with_state(state)
        .layer(middleware::from_fn(same_origin))
        .layer(TraceLayer::new_for_http())
}

async fn publish_overlay_snapshot(
    State(state): State<SharedState>,
    Json(snapshot): Json<Value>,
) -> StatusCode {
    *state.overlay_snapshot.write().await = Some((std::time::Instant::now(), snapshot));
    StatusCode::NO_CONTENT
}

async fn overlay_snapshot(State(state): State<SharedState>) -> Json<Value> {
    let snapshot = state.overlay_snapshot.read().await;
    match snapshot.as_ref() {
        Some((at, value)) if at.elapsed().as_secs() < 10 => Json(value.clone()),
        _ => Json(Value::Null),
    }
}

fn split_host_port(value: &str) -> (&str, Option<&str>) {
    if let Some(rest) = value.strip_prefix('[') {
        if let Some((host, port)) = rest.split_once(']') {
            return (host, port.strip_prefix(':').filter(|port| !port.is_empty()));
        }
    }
    match value.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        _ => (value, None),
    }
}

fn is_local_host(host: &str) -> bool {
    let (name, _) = split_host_port(host);
    name.eq_ignore_ascii_case("localhost") || name == "127.0.0.1" || name == "::1"
}

fn origin_matches_host(origin: &str, host: &str) -> bool {
    let value = origin.trim();
    let Some(rest) = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
    else {
        return false;
    };
    let origin_host = rest.split('/').next().unwrap_or("");
    if origin_host.eq_ignore_ascii_case(host) {
        return true;
    }
    let (origin_name, origin_port) = split_host_port(origin_host);
    let (host_name, host_port) = split_host_port(host);
    origin_port == host_port
        && is_local_host(origin_name)
        && is_local_host(host_name)
        && ((origin_name == "127.0.0.1" && host_name == "localhost")
            || (origin_name == "localhost" && host_name == "127.0.0.1"))
}

async fn same_origin(request: Request, next: Next) -> Response {
    if request.uri().path().starts_with("/api/") {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !host.is_empty() && !is_local_host(host) {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "detail": "Host no permitido." })),
            )
                .into_response();
        }
        if let Some(origin) = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
        {
            if !origin_matches_host(origin, host) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "detail": "Origen no permitido." })),
                )
                    .into_response();
            }
        }
    }
    next.run(request).await
}

async fn engine(state: &SharedState) -> Arc<Engine> {
    state.engine.read().await.clone()
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "detail": message.into() }))).into_response()
}

async fn health(State(state): State<SharedState>) -> Json<Value> {
    let engine = engine(&state).await;
    Json(json!({
        "status": "ok",
        "patch": engine.patch,
        "models": {
            "draft": engine.draft_model.is_some(),
            "build": engine.build_model.is_some(),
            "device": "cpu",
        },
        "dataset": {
            "games": engine.summary.get("games").cloned().unwrap_or(json!(0)),
            "players": engine.summary.get("players").cloned().unwrap_or(json!(0)),
            "dateRange": engine.summary.get("dateRange").cloned().unwrap_or(json!([])),
        },
    }))
}

async fn meta(State(state): State<SharedState>) -> Response {
    let engine = engine(&state).await;
    match engine.meta_payload() {
        Ok(value) => Json(value).into_response(),
        Err(error_value) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
    }
}

#[derive(Deserialize)]
struct RoleQuery {
    role: Option<String>,
}

async fn meta_champions(
    State(state): State<SharedState>,
    Query(query): Query<RoleQuery>,
) -> Response {
    let engine = engine(&state).await;
    match engine.champion_meta(query.role.as_deref()) {
        Ok(champions) => {
            Json(json!({ "role": query.role, "champions": champions })).into_response()
        }
        Err(error_value) => error(StatusCode::BAD_REQUEST, error_value.to_string()),
    }
}

async fn draft(State(state): State<SharedState>, Json(request): Json<DraftRequest>) -> Response {
    let engine = engine(&state).await;
    match engine.draft_recommend(&request) {
        Ok(value) => Json(value).into_response(),
        Err(error_value) => error(StatusCode::BAD_REQUEST, error_value.to_string()),
    }
}

fn names(engine: &Engine, values: &[ChampionRef]) -> Vec<String> {
    values
        .iter()
        .map(|value| {
            engine
                .resolve_champion(value)
                .map(|champ| champ.name.clone())
                .unwrap_or_else(|| match value {
                    ChampionRef::Int(id) => id.to_string(),
                    ChampionRef::Str(text) => text.clone(),
                })
        })
        .collect()
}

async fn draft_bans(
    State(state): State<SharedState>,
    Json(request): Json<BansRequest>,
) -> Response {
    let engine = engine(&state).await;
    let exclude = names(&engine, &request.exclude);
    Json(json!({ "bans": engine.ban_priority(&exclude, request.limit) })).into_response()
}

#[derive(Deserialize)]
struct BuildQuery {
    simulate: Option<bool>,
}

async fn build(
    State(state): State<SharedState>,
    Query(query): Query<BuildQuery>,
    Json(mut request): Json<BuildRequest>,
) -> Response {
    if let Some(simulate) = query.simulate {
        request.simulate = simulate;
    }
    let engine = engine(&state).await;
    match engine.build_recommend(&request) {
        Ok(value) => Json(value).into_response(),
        Err(error_value) => error(StatusCode::BAD_REQUEST, error_value.to_string()),
    }
}

async fn live(State(state): State<SharedState>, Json(request): Json<LiveRequest>) -> Response {
    let engine = engine(&state).await;
    let draft_request = DraftRequest {
        role: request.build.role.clone(),
        side: request.build.side.clone(),
        bans: request.bans.clone(),
        allies: request.build.allies.clone(),
        enemies: request.build.enemies.clone(),
        my_champion: None,
        limit: request.build.limit,
    };
    let draft_state = match engine.draft_evaluate(&draft_request) {
        Ok(value) => value,
        Err(error_value) => return error(StatusCode::BAD_REQUEST, error_value.to_string()),
    };
    let build_state = match engine.build_recommend(&request.build) {
        Ok(value) => value,
        Err(error_value) => return error(StatusCode::BAD_REQUEST, error_value.to_string()),
    };
    Json(json!({ "draft": draft_state, "build": build_state })).into_response()
}

async fn dataset(State(state): State<SharedState>) -> Response {
    let engine = engine(&state).await;
    match engine.dataset_payload() {
        Ok(value) => Json(value).into_response(),
        Err(error_value) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
    }
}

async fn model(State(state): State<SharedState>) -> Json<Value> {
    let engine = engine(&state).await;
    Json(engine.model_payload())
}

async fn reload(State(state): State<SharedState>) -> Response {
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || Engine::load(paths)).await {
        Ok(Ok(engine)) => {
            let lab_enabled = state
                .settings
                .read()
                .map(|settings| settings.lab_enabled)
                .unwrap_or(false);
            engine.lab_enabled.store(
                lab_enabled && engine.lab_build_model.is_some(),
                std::sync::atomic::Ordering::Relaxed,
            );
            let draft = engine.draft_model.is_some();
            let build = engine.build_model.is_some();
            let patch = engine.patch.clone();
            *state.engine.write().await = Arc::new(engine);
            Json(json!({
                "reloaded": true,
                "draft": draft,
                "build": build,
                "patch": patch,
            }))
            .into_response()
        }
        Ok(Err(error_value)) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
        Err(error_value) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
    }
}

async fn update_check() -> Json<Value> {
    match crate::update::check().await {
        Ok(status) => {
            Json(serde_json::to_value(status).unwrap_or_else(|_| json!({ "available": false })))
        }
        Err(error) => Json(json!({
            "current": crate::update::current_version(),
            "available": false,
            "error": error.to_string(),
        })),
    }
}

async fn update_install() -> Response {
    match crate::update::install().await {
        Ok(path) => Json(json!({
            "started": true,
            "installer": path.display().to_string(),
        }))
        .into_response(),
        Err(err) => error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

fn settings_payload(settings: &Settings) -> Value {
    json!({
        "showAdvanced": settings.show_advanced,
        "startWithWindows": settings.start_with_windows,
        "minimizeToTray": settings.minimize_to_tray,
        "overlayEnabled": settings.overlay_enabled,
        "overlayPosition": settings.overlay_position,
        "labEnabled": settings.lab_enabled,
        "startupSupported": crate::settings::startup_supported(),
        "version": crate::update::current_version(),
    })
}

async fn settings_get(State(state): State<SharedState>) -> Json<Value> {
    let settings = state
        .settings
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let engine = engine(&state).await;
    let mut payload = settings_payload(&settings);
    payload["labAvailable"] = json!(engine.lab_build_model.is_some());
    payload["labEnabled"] = json!(
        engine
            .lab_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    Json(payload)
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct SettingsPatch {
    show_advanced: Option<bool>,
    start_with_windows: Option<bool>,
    minimize_to_tray: Option<bool>,
    overlay_enabled: Option<bool>,
    overlay_position: Option<settings::OverlayPosition>,
    lab_enabled: Option<bool>,
}

async fn settings_update(
    State(state): State<SharedState>,
    Json(patch): Json<SettingsPatch>,
) -> Response {
    let mut next = state
        .settings
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();

    if let Some(enabled) = patch.start_with_windows {
        match settings::apply_startup(enabled) {
            Ok(true) => next.start_with_windows = enabled,
            Ok(false) if enabled => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "El arranque con Windows solo esta disponible desde la app de escritorio.",
                );
            }
            Ok(false) => next.start_with_windows = false,
            Err(error_value) => {
                return error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string());
            }
        }
    }
    if let Some(value) = patch.show_advanced {
        next.show_advanced = value;
    }
    if let Some(value) = patch.minimize_to_tray {
        next.minimize_to_tray = value;
    }
    if let Some(value) = patch.overlay_enabled {
        next.overlay_enabled = value;
    }
    if let Some(value) = patch.overlay_position {
        next.overlay_position = value;
    }
    let engine = engine(&state).await;
    if let Some(value) = patch.lab_enabled {
        if value && engine.lab_build_model.is_none() {
            return error(StatusCode::BAD_REQUEST, "El modelo Lab no está disponible.");
        }
        next.lab_enabled = value;
    }

    if let Err(error_value) = next.save(&state.paths) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string());
    }
    if let Ok(mut current) = state.settings.write() {
        *current = next.clone();
    }
    engine.lab_enabled.store(
        next.lab_enabled && engine.lab_build_model.is_some(),
        std::sync::atomic::Ordering::Relaxed,
    );
    *state.overlay_snapshot.write().await = None;
    let mut payload = settings_payload(&next);
    payload["labAvailable"] = json!(engine.lab_build_model.is_some());
    payload["labEnabled"] = json!(
        engine
            .lab_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    Json(payload).into_response()
}

async fn diagnostics(State(state): State<SharedState>) -> Json<Value> {
    let engine = engine(&state).await;
    let settings = state
        .settings
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let client = state.lcu.client.status().await;
    Json(json!({
        "version": crate::update::current_version(),
        "patch": engine.patch,
        "dataset": {
            "games": engine.summary.get("games").cloned().unwrap_or(json!(0)),
            "dateRange": engine.summary.get("dateRange").cloned().unwrap_or(json!([])),
        },
        "models": {
            "draft": engine.draft_model.is_some(),
            "build": engine.build_model.is_some(),
            "notes": engine.model_notes,
        },
        "data": engine.data_status(),
        "paths": {
            "root": state.paths.root.display().to_string(),
            "models": state.paths.models.display().to_string(),
            "logs": crate::logging::log_dir().display().to_string(),
        },
        "settings": settings_payload(&settings),
        "processId": std::process::id(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "client": client,
    }))
}

async fn open_logs() -> Response {
    let directory = crate::logging::log_dir();
    if let Err(error_value) = std::fs::create_dir_all(&directory) {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("No se pudo crear {}: {error_value}", directory.display()),
        );
    }
    match open::that(&directory) {
        Ok(()) => Json(json!({
            "opened": true,
            "directory": directory.display().to_string(),
        }))
        .into_response(),
        Err(error_value) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
    }
}

async fn client_status(State(state): State<SharedState>) -> Json<Value> {
    Json(state.lcu.client.status().await)
}

async fn client_draft(State(state): State<SharedState>) -> Json<Value> {
    let status = state.lcu.client.status().await;
    let connected = status
        .get("connected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let phase = status.get("phase").and_then(Value::as_str);
    if !connected || phase != Some("ChampSelect") {
        return Json(json!({
            "available": false,
            "status": status,
            "message": "Abre un champ select en League of Legends.",
        }));
    }
    let Some(session) = state.lcu.client.champ_select_session().await else {
        return Json(json!({
            "available": false,
            "status": status,
            "message": "LCU está en champ select pero no devolvió la sesión.",
        }));
    };
    let engine = engine(&state).await;
    let gameflow = state
        .lcu
        .client
        .gameflow_session()
        .await
        .unwrap_or_else(|| json!({}));
    let mapped = map_champ_select(&engine, &session, Some(&gameflow), &status);
    let available = mapped
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Json(json!({ "available": available, "status": status, "draft": mapped }))
}

async fn client_live(State(state): State<SharedState>) -> Json<Value> {
    let Some(mut data) = state
        .lcu
        .client
        .live_gamedata(std::time::Duration::from_millis(1500))
        .await
    else {
        return Json(json!({
            "available": false,
            "message": "No hay una partida en curso o el Live Client API no responde en el puerto 2999.",
        }));
    };
    if let Some(session) = state.lcu.client.gameflow_session().await {
        if let (Some(game), Some(session_data)) = (
            data.get_mut("gameData").and_then(Value::as_object_mut),
            session.get("gameData"),
        ) {
            if let Some(queue_id) = session_data
                .get("queue")
                .and_then(|value| value.get("queueId"))
                .or_else(|| session_data.get("queueId"))
            {
                game.insert("queueId".to_string(), queue_id.clone());
            }
            if let Some(game_id) = session_data.get("gameId") {
                game.insert("gameId".to_string(), game_id.clone());
            }
        }
    }
    let engine = engine(&state).await;
    let mapped = map_live_gamedata(&engine, &data);
    let available = mapped
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Json(json!({ "available": available, "state": mapped }))
}

async fn client_postgame(State(state): State<SharedState>) -> Json<Value> {
    match state.lcu.client.postgame_summary().await {
        Some(value) => Json(value),
        None => Json(json!({
            "available": false,
            "message": "Todavia no hay una partida terminada en el historial local del cliente.",
        })),
    }
}

async fn client_search_status(State(state): State<SharedState>) -> Json<Value> {
    Json(json!({ "search": state.lcu.locator.status_json(false) }))
}

async fn client_search(State(state): State<SharedState>) -> Json<Value> {
    let search = state.lcu.locator.start(true);
    Json(json!({ "started": true, "search": search }))
}

#[derive(Deserialize)]
struct ClientPathRequest {
    path: String,
}

async fn client_path(
    State(state): State<SharedState>,
    Json(request): Json<ClientPathRequest>,
) -> Response {
    let locator = state.lcu.locator.clone();
    match tokio::task::spawn_blocking(move || locator.set_custom(&request.path)).await {
        Ok(Ok(search)) => {
            let status = state.lcu.client.status().await;
            Json(json!({ "ok": true, "search": search, "status": status })).into_response()
        }
        Ok(Err(error_value)) => error(StatusCode::BAD_REQUEST, error_value.to_string()),
        Err(error_value) => error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string()),
    }
}

async fn static_asset(uri: Uri) -> Response {
    let request_path = uri.path().trim_start_matches('/');
    let path = if request_path.is_empty() {
        "index.html"
    } else if request_path == "favicon.svg" {
        "assets/favicon.svg"
    } else {
        request_path
    };
    let Some(content) = WebAssets::get(path) else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )],
        content.data.into_owned(),
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' https://ddragon.leagueoflegends.com data:; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; script-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    );
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    if path.starts_with("assets/") || path == "index.html" || path == "overlay.html" {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, must-revalidate"),
        );
        headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hosts_are_detected() {
        assert!(is_local_host("127.0.0.1:8000"));
        assert!(is_local_host("localhost:8000"));
        assert!(is_local_host("[::1]:8000"));
        assert!(!is_local_host("evil.example:8000"));
        assert!(!is_local_host("192.168.1.10:8000"));
    }

    #[test]
    fn same_origin_is_enforced() {
        assert!(origin_matches_host(
            "http://127.0.0.1:8000",
            "127.0.0.1:8000"
        ));
        assert!(origin_matches_host(
            "http://localhost:8000",
            "localhost:8000"
        ));
        assert!(origin_matches_host(
            "http://localhost:8000",
            "127.0.0.1:8000"
        ));
        assert!(!origin_matches_host(
            "http://evil.example:8000",
            "127.0.0.1:8000"
        ));
        assert!(!origin_matches_host("null", "127.0.0.1:8000"));
    }
}
