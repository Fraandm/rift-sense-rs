use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

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
        .route("/api/dataset", get(dataset))
        .route("/api/model", get(model))
        .route("/api/reload", post(reload))
        .route("/api/update/check", get(update_check))
        .route("/api/update/install", post(update_install))
        .route("/api/settings", get(settings_get).post(settings_update))
        .route("/api/client/status", get(client_status))
        .route("/api/client/draft", get(client_draft))
        .route("/api/client/live", get(client_live))
        .route(
            "/api/client/search",
            get(client_search_status).post(client_search),
        )
        .route("/api/client/path", post(client_path))
        .fallback(static_asset)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
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
    Json(settings_payload(&settings))
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct SettingsPatch {
    show_advanced: Option<bool>,
    start_with_windows: Option<bool>,
    minimize_to_tray: Option<bool>,
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

    if let Err(error_value) = next.save(&state.paths) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string());
    }
    if let Ok(mut current) = state.settings.write() {
        *current = next.clone();
    }
    Json(settings_payload(&next)).into_response()
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
        if let (Some(game), Some(queue_id)) = (
            data.get_mut("gameData").and_then(Value::as_object_mut),
            session
                .get("gameData")
                .and_then(|value| value.get("queue"))
                .and_then(|value| value.get("queueId")),
        ) {
            game.insert("queueId".to_string(), queue_id.clone());
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
    if path.starts_with("assets/") || path == "index.html" {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, must-revalidate"),
        );
        response
            .headers_mut()
            .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}
