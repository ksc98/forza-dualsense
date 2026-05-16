use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;

use crate::settings::Settings;
use crate::state::SharedState;

#[derive(RustEmbed)]
#[folder = "assets/web/"]
struct WebAssets;

#[derive(Clone)]
struct AppCtx {
    state: SharedState,
    pps: Arc<parking_lot::Mutex<PpsTracker>>,
}

struct PpsTracker {
    last_count: u64,
    last_at: Instant,
    pps: f32,
}

impl PpsTracker {
    fn new() -> Self {
        Self { last_count: 0, last_at: Instant::now(), pps: 0.0 }
    }
    fn update(&mut self, count: u64) -> f32 {
        let dt = self.last_at.elapsed().as_secs_f32();
        if dt >= 0.5 {
            let delta = count.saturating_sub(self.last_count) as f32;
            self.pps = delta / dt;
            self.last_count = count;
            self.last_at = Instant::now();
        }
        self.pps
    }
}

pub async fn serve(state: SharedState, addr: SocketAddr) -> Result<()> {
    let ctx = AppCtx {
        state: state.clone(),
        pps: Arc::new(parking_lot::Mutex::new(PpsTracker::new())),
    };

    let app = Router::new()
        .route("/api/state", get(get_state))
        .route("/api/settings", get(get_settings).post(post_settings))
        .route("/api/ws", get(ws_handler))
        .fallback(static_handler)
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    {
        let mut s = state.lock();
        s.web_url = format!("http://{bound}");
    }
    tracing::info!("Web UI on http://{bound}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_state(State(ctx): State<AppCtx>) -> Json<serde_json::Value> {
    let pps = {
        let s = ctx.state.lock();
        let mut p = ctx.pps.lock();
        p.update(s.packets_received)
    };
    let s = ctx.state.lock();
    let snap = s.snapshot(pps);
    Json(serde_json::to_value(&snap).unwrap())
}

async fn get_settings(State(ctx): State<AppCtx>) -> Json<Settings> {
    Json(ctx.state.lock().settings.clone())
}

async fn post_settings(
    State(ctx): State<AppCtx>,
    Json(new_settings): Json<Settings>,
) -> impl IntoResponse {
    {
        let mut s = ctx.state.lock();
        s.settings = new_settings.clone();
    }
    match new_settings.save() {
        Ok(_) => (StatusCode::OK, "ok"),
        Err(e) => {
            ctx.state.lock().last_settings_save_error = e.to_string();
            (StatusCode::INTERNAL_SERVER_ERROR, "save failed")
        }
    }
}

async fn ws_handler(State(ctx): State<AppCtx>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_ws(socket, ctx))
}

async fn handle_ws(mut socket: WebSocket, ctx: AppCtx) {
    let mut interval = tokio::time::interval(Duration::from_millis(33));
    loop {
        interval.tick().await;
        let pps = {
            let s = ctx.state.lock();
            let mut p = ctx.pps.lock();
            p.update(s.packets_received)
        };
        let payload = {
            let s = ctx.state.lock();
            serde_json::to_string(&s.snapshot(pps)).unwrap()
        };
        if socket.send(Message::Text(payload)).await.is_err() {
            break;
        }
    }
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
