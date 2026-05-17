mod controller;
mod gui;
mod hid;
mod hid_task;
mod settings;
mod state;
mod telemetry;
mod tray;
mod triggers;
mod udp;
mod update;
mod web;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use parking_lot::Mutex;

use crate::settings::Settings;
use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(
    name = "forza-dualsense",
    version,
    about = "Adaptive trigger feedback for Forza Horizon on the DualSense controller."
)]
struct Args {
    /// UDP bind address.
    #[arg(long)]
    host: Option<String>,

    /// UDP port. Must match Forza Horizon's "Data Out IP Port".
    #[arg(long)]
    port: Option<u16>,

    /// Skip the native window — only run the headless engine + web UI.
    #[arg(long)]
    no_gui: bool,

    /// Disable the embedded web UI.
    #[arg(long)]
    no_web: bool,

    /// Port for the embedded web UI.
    #[arg(long, default_value_t = 5301)]
    web_port: u16,

    /// Skip the launch-time update check.
    #[arg(long)]
    no_update: bool,

    /// Verbose logging.
    #[arg(long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let env_filter = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| env_filter.into()),
        )
        .with_target(false)
        .init();

    let mut settings = Settings::load_or_default();
    if let Some(h) = args.host {
        settings.udp_host = h;
    }
    if let Some(p) = args.port {
        settings.udp_port = p;
    }

    let state = Arc::new(Mutex::new(AppState::new(settings)));

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    // Spawn background tasks. udp::run flips `udp_bound` true once the
    // bind succeeds — don't set it optimistically here.
    {
        let st = state.clone();
        let host = st.lock().settings.udp_host.clone();
        let port = st.lock().settings.udp_port;
        rt.spawn(async move {
            if let Err(e) = udp::run(st.clone(), host, port).await {
                tracing::error!("UDP listener fatal: {e}");
                st.lock().udp_bound = false;
            }
        });
    }
    {
        let st = state.clone();
        std::thread::Builder::new()
            .name("dualsense-hid".into())
            .spawn(move || hid_task::run(st))?;
    }
    if !args.no_web {
        let st = state.clone();
        let addr: SocketAddr = format!("127.0.0.1:{}", args.web_port).parse()?;
        rt.spawn(async move {
            if let Err(e) = web::serve(st, addr).await {
                tracing::error!("Web server failed: {e}");
            }
        });
    }

    {
        let auto_update = state.lock().settings.enable_auto_update;
        if args.no_update || !auto_update {
            state.lock().update_status = update::Status::Disabled;
        } else {
            let st = state.clone();
            st.lock().update_status = update::Status::Checking;
            rt.spawn(async move {
                let status = tokio::task::spawn_blocking(update::check_and_apply)
                    .await
                    .unwrap_or_else(|e| update::Status::Failed { error: e.to_string() });
                match &status {
                    update::Status::Applied { version } => {
                        tracing::info!("Update {version} downloaded — restart to apply.");
                    }
                    update::Status::Failed { error } => {
                        tracing::warn!("Update check failed: {error}");
                    }
                    _ => {}
                }
                st.lock().update_status = status;
            });
        }
    }

    if args.no_gui {
        // Headless: block on Ctrl-C.
        rt.block_on(async {
            tokio::signal::ctrl_c().await.ok();
        });
        return Ok(());
    }

    // Hold the runtime alive for the lifetime of the GUI.
    let _guard = rt.enter();

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([720.0, 500.0])
            .with_title("Forza DualSense"),
        ..Default::default()
    };

    eframe::run_native(
        "Forza DualSense",
        native_options,
        Box::new(move |cc| Box::new(gui::GuiApp::new(state.clone(), cc))),
    )
    .map_err(|e| anyhow::anyhow!("egui: {e}"))?;

    Ok(())
}
