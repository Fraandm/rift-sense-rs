#![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use riftsense::logging;
use riftsense::settings::{Settings, SettingsHandle};
use riftsense::{api, config::Paths, engine::Engine, lcu::LcuService};
use std::sync::mpsc;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
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
    ToggleOverlay,
}

const WINDOW_TITLE: &str = "RiftSense — Analizador de draft y build";
const OVERLAY_TITLE: &str = "RiftSense Overlay";
const OVERLAY_WIDTH: f64 = 392.0;
const OVERLAY_HEIGHT: f64 = 132.0;
const OVERLAY_MARGIN: f64 = 18.0;
const OVERLAY_TOP: f64 = 64.0;

#[cfg(windows)]
struct InstanceGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn single_instance() -> Result<Option<InstanceGuard>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    #[cfg(debug_assertions)]
    let instance_name = std::env::var("RIFTSENSE_DEV_MUTEX")
        .unwrap_or_else(|_| "Local\\RiftSenseDesktop".to_string());
    #[cfg(not(debug_assertions))]
    let instance_name = "Local\\RiftSenseDesktop".to_string();
    let name: Vec<u16> = std::ffi::OsStr::new(&instance_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        anyhow::bail!("No se pudo crear el mutex de instancia única");
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        return Ok(None);
    }
    Ok(Some(InstanceGuard(handle)))
}

#[cfg(windows)]
fn focus_existing_window() -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let title: Vec<u16> = std::ffi::OsStr::new(WINDOW_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[cfg(not(windows))]
fn single_instance() -> Result<Option<()>> {
    Ok(Some(()))
}

#[cfg(not(windows))]
fn focus_existing_window() -> bool {
    false
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

fn overlay_enabled(settings: &SettingsHandle) -> bool {
    settings
        .read()
        .map(|settings| settings.overlay_enabled)
        .unwrap_or(false)
}

fn toggle_overlay_setting(settings: &SettingsHandle, paths: &Paths) {
    let next = match settings.write() {
        Ok(mut settings) => {
            settings.overlay_enabled = !settings.overlay_enabled;
            settings.clone()
        }
        Err(_) => return,
    };
    if let Err(error) = next.save(paths) {
        eprintln!("No se pudo guardar el estado del overlay: {error}");
    }
}

fn show_overlay(window: &Window) {
    window.set_visible(true);
    window.set_always_on_top(true);
    apply_overlay_styles(window);
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
        };
        let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST as windows_sys::Win32::Foundation::HWND,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

#[cfg(windows)]
fn apply_overlay_styles(window: &Window) {
    use tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_CAPTION, WS_EX_ACCEPTFILES,
        WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };

    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let chrome =
            (WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU) as isize;
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !chrome) | WS_POPUP as isize);

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex_chrome =
            (WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_APPWINDOW | WS_EX_ACCEPTFILES) as isize;
        let overlay_style = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex_style & !ex_chrome) | overlay_style);

        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    let _ = window.set_ignore_cursor_events(true);
}

#[cfg(not(windows))]
fn apply_overlay_styles(_window: &Window) {}

#[cfg(windows)]
fn league_in_foreground() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(foreground, &mut process_id);
    }
    if process_id == 0 {
        return false;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return false;
    }
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(process);
    }
    if ok == 0 {
        return false;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]).to_ascii_lowercase();
    path.ends_with("league of legends.exe")
}

#[cfg(not(windows))]
fn league_in_foreground() -> bool {
    false
}

#[cfg(windows)]
fn spawn_overlay_hotkey(proxy: EventLoopProxy<UserEvent>) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_O, VK_SHIFT,
    };

    std::thread::spawn(move || {
        let mut pressed = false;
        loop {
            let down = |key: i32| unsafe { (GetAsyncKeyState(key) as u16 & 0x8000) != 0 };
            let active = down(VK_CONTROL as i32) && down(VK_SHIFT as i32) && down(VK_O as i32);
            if active && !pressed {
                let _ = proxy.send_event(UserEvent::ToggleOverlay);
            }
            pressed = active;
            std::thread::sleep(Duration::from_millis(80));
        }
    });
}

#[cfg(not(windows))]
fn spawn_overlay_hotkey(_proxy: EventLoopProxy<UserEvent>) {}

fn main() -> Result<()> {
    let _instance = match single_instance()? {
        Some(guard) => guard,
        None => {
            focus_existing_window();
            return Ok(());
        }
    };

    let log = logging::init("riftsense-desktop")?;
    logging::install_panic_hook();
    tracing::info!("logs: {}", log.directory.display());

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
    let desktop_paths = paths.clone();
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
    spawn_overlay_hotkey(event_loop.create_proxy());

    const ICON_RGBA: &[u8] = include_bytes!("../../assets/RiftSense-64.rgba");
    let window_icon = Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok();
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
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

    let overlay_position = event_loop.primary_monitor().map(|monitor| {
        let scale = monitor.scale_factor();
        let origin = monitor.position();
        let size = monitor.size();
        PhysicalPosition::new(
            origin.x + size.width as i32
                - (OVERLAY_WIDTH * scale) as i32
                - (OVERLAY_MARGIN * scale) as i32,
            origin.y + (OVERLAY_TOP * scale) as i32,
        )
    });
    let mut overlay_builder = WindowBuilder::new()
        .with_title(OVERLAY_TITLE)
        .with_window_icon(Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok())
        .with_inner_size(LogicalSize::new(OVERLAY_WIDTH, OVERLAY_HEIGHT))
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_resizable(false)
        .with_visible(false)
        .with_focused(false);
    if let Some(position) = overlay_position {
        overlay_builder = overlay_builder.with_position(position);
    }
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        overlay_builder = overlay_builder.with_undecorated_shadow(false);
    }
    let overlay_window = overlay_builder
        .build(&event_loop)
        .context("No se pudo crear la ventana del overlay")?;
    apply_overlay_styles(&overlay_window);
    let _overlay_webview = WebViewBuilder::new()
        .with_url(format!("{url}/overlay.html"))
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled)
        .build(&overlay_window)
        .context("No se pudo crear el WebView del overlay")?;

    if let Err(error) = _overlay_webview.set_background_color((0, 0, 0, 0)) {
        eprintln!("No se pudo hacer transparente el overlay: {error}");
    }

    let show_item = MenuItem::with_id("show", "Abrir RiftSense", true, None);
    let overlay_item = MenuItem::with_id(
        "overlay",
        if overlay_enabled(&settings) {
            "Overlay en partida: activado"
        } else {
            "Overlay en partida: desactivado"
        },
        true,
        None,
    );
    let quit_item = MenuItem::with_id("quit", "Salir", true, None);
    let separator = PredefinedMenuItem::separator();
    let menu = Menu::new();
    menu.append_items(&[&show_item, &overlay_item, &separator, &quit_item])
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

    let main_window_id = window.id();
    let overlay_window_id = overlay_window.id();
    let mut overlay_visible = false;
    let mut last_overlay_setting = overlay_enabled(&settings);
    let mut next_overlay_check = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        let mut check_overlay_now = false;
        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if window_id == overlay_window_id {
                    overlay_window.set_visible(false);
                    overlay_visible = false;
                } else {
                    let hide = tray.is_some() && minimize_to_tray(&settings);
                    if hide {
                        window.set_visible(false);
                    } else {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Resized(size),
                ..
            } => {
                if window_id == main_window_id
                    && (size.width == 0 || size.height == 0)
                    && tray.is_some()
                    && minimize_to_tray(&settings)
                {
                    window.set_visible(false);
                }
            }
            Event::UserEvent(UserEvent::Menu(event)) => match event.id().as_ref() {
                "show" => show_window(&window),
                "overlay" => {
                    toggle_overlay_setting(&settings, &desktop_paths);
                    check_overlay_now = true;
                }
                "quit" => *control_flow = ControlFlow::Exit,
                _ => {}
            },
            Event::UserEvent(UserEvent::ToggleOverlay) => {
                toggle_overlay_setting(&settings, &desktop_paths);
                check_overlay_now = true;
            }
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show_window(&window),
            _ => {}
        }

        if check_overlay_now || Instant::now() >= next_overlay_check {
            next_overlay_check = Instant::now() + Duration::from_millis(400);
            let enabled = overlay_enabled(&settings);
            let show = enabled && league_in_foreground();
            if show != overlay_visible {
                if show {
                    show_overlay(&overlay_window);
                } else {
                    overlay_window.set_visible(false);
                }
                overlay_visible = show;
            }
            if enabled != last_overlay_setting {
                overlay_item.set_text(if enabled {
                    "Overlay en partida: activado"
                } else {
                    "Overlay en partida: desactivado"
                });
                last_overlay_setting = enabled;
            }
        }
    })
}
