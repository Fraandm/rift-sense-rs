#![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use riftsense::{api, config::Paths, engine::Engine, lcu::LcuService};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, WindowBuilder},
};
use tokio::{net::TcpListener, sync::RwLock};
use wry::{PageLoadEvent, WebViewBuilder};

fn start_server(preferred_port: u16) -> Result<u16> {
    let (sender, receiver) = mpsc::channel::<Result<u16, String>>();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = sender.send(Err(error.to_string()));
                return;
            }
        };
        runtime.block_on(async move {
            let result = async {
                let paths = Paths::discover()?;
                let engine = tokio::task::spawn_blocking({
                    let paths = paths.clone();
                    move || Engine::load(paths)
                })
                .await??;
                let lcu = LcuService::new(&paths.root)?;
                let state = Arc::new(api::AppState {
                    engine: RwLock::new(Arc::new(engine)),
                    paths,
                    lcu,
                });
                let listener = TcpListener::bind(("127.0.0.1", preferred_port)).await?;
                let port = listener.local_addr()?.port();
                if sender.send(Ok(port)).is_err() {
                    return Ok(());
                }
                axum::serve(listener, api::router(state)).await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if let Err(error) = result {
                let _ = sender.send(Err(error.to_string()));
            }
        });
    });
    receiver
        .recv_timeout(Duration::from_secs(90))
        .map_err(|error| anyhow::anyhow!("El servidor no arranco: {error}"))?
        .map_err(anyhow::Error::msg)
}

fn main() -> Result<()> {
    let startup = Instant::now();
    let mut preferred_port = 0u16;
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--port") {
        preferred_port = args
            .get(index + 1)
            .and_then(|value| value.parse().ok())
            .context("--port necesita un numero")?;
    }

    let port = start_server(preferred_port)?;
    let url = format!("http://127.0.0.1:{port}");
    println!(
        "RiftSense Desktop (Rust + WebView2) -> {url} ({:.2}s)",
        startup.elapsed().as_secs_f64()
    );

    let event_loop = EventLoop::new();
    const ICON_RGBA: &[u8] = include_bytes!("../../assets/RiftSense-64.rgba");
    let window_icon = Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok();
    let window = WindowBuilder::new()
        .with_title("RiftSense · Analizador de draft y build")
        .with_window_icon(window_icon)
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(1000.0, 680.0))
        .build(&event_loop)
        .context("No se pudo crear la ventana")?;
    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_devtools(true)
        .with_on_page_load_handler(move |event, loaded_url| {
            if matches!(event, PageLoadEvent::Finished) && loaded_url.starts_with("http://127.0.0.1:")
            {
                println!(
                    "UI cargada en {:.2}s: {}",
                    startup.elapsed().as_secs_f64(),
                    loaded_url
                );
            }
        })
        .build(&window)
        .context("No se pudo crear WebView2")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}
