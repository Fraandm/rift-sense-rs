#![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use riftsense::settings::{Settings, SettingsHandle};
use riftsense::{api, config::Paths, engine::Engine, lcu::LcuService};
use std::sync::mpsc;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Icon, Window, WindowBuilder},
};
use tokio::{net::TcpListener, sync::RwLock};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use wry::{PageLoadEvent, WebViewBuilder};

enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

fn start_server(preferred_port: u16, paths: Paths, settings: SettingsHandle) -> Result<u16> {
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
                    settings,
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

fn show_window(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
}

fn minimize_to_tray(settings: &SettingsHandle) -> bool {
    settings
        .read()
        .map(|settings| settings.minimize_to_tray)
        .unwrap_or(false)
}

fn main() -> Result<()> {
    let startup = Instant::now();
    let mut preferred_port = 0u16;
    let mut start_minimized = false;
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--port") {
        preferred_port = args
            .get(index + 1)
            .and_then(|value| value.parse().ok())
            .context("--port necesita un numero")?;
    }
    if args
        .iter()
        .any(|arg| arg == "--minimized" || arg == "--tray")
    {
        start_minimized = true;
    }

    let paths = Paths::discover()?;
    let settings: SettingsHandle = Arc::new(StdRwLock::new(Settings::load(&paths)));
    let port = start_server(preferred_port, paths, settings.clone())?;
    let url = format!("http://127.0.0.1:{port}");
    println!(
        "RiftSense Desktop (Rust + WebView2) -> {url} ({:.2}s)",
        startup.elapsed().as_secs_f64()
    );

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Tray(event));
    }));
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    const ICON_RGBA: &[u8] = include_bytes!("../../assets/RiftSense-64.rgba");
    let window_icon = Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok();
    let window = WindowBuilder::new()
        .with_title("RiftSense — Analizador de draft y build")
        .with_window_icon(window_icon)
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(1000.0, 680.0))
        .with_visible(false)
        .build(&event_loop)
        .context("No se pudo crear la ventana")?;
    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_devtools(true)
        .with_on_page_load_handler(move |event, loaded_url| {
            if matches!(event, PageLoadEvent::Finished)
                && loaded_url.starts_with("http://127.0.0.1:")
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

    let show_item = MenuItem::with_id("show", "Abrir RiftSense", true, None);
    let quit_item = MenuItem::with_id("quit", "Salir", true, None);
    let separator = PredefinedMenuItem::separator();
    let menu = Menu::new();
    menu.append_items(&[&show_item, &separator, &quit_item])
        .context("No se pudo montar el menu de la bandeja")?;
    let tray = TrayIconBuilder::new()
        .with_icon(
            tray_icon::Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64)
                .context("No se pudo crear el icono de la bandeja")?,
        )
        .with_tooltip("RiftSense · Coach de compras")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build();

    let tray = match tray {
        Ok(tray) => Some(tray),
        Err(error) => {
            eprintln!("No se pudo crear el icono de la bandeja: {error}");
            None
        }
    };

    if !start_minimized || tray.is_none() {
        show_window(&window);
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let hide = tray.is_some() && minimize_to_tray(&settings);
                if hide {
                    window.set_visible(false);
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                if size.width == 0 || size.height == 0 {
                    if tray.is_some() && minimize_to_tray(&settings) {
                        window.set_visible(false);
                    }
                }
            }
            Event::UserEvent(UserEvent::Menu(event)) => match event.id().as_ref() {
                "show" => show_window(&window),
                "quit" => *control_flow = ControlFlow::Exit,
                _ => {}
            },
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show_window(&window),
            _ => {}
        }
    })
}
