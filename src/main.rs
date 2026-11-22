use anyhow::{Context, Result};
use clap::Parser;
use riftsense::logging;
use riftsense::settings::Settings;
use riftsense::{api, config::Paths, engine::Engine};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

#[derive(Parser, Debug)]
#[command(name = "riftsense", about = "RiftSense reescrito en Rust")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8000)]
    port: u16,
    #[arg(long, help = "Abre la interfaz en el navegador")]
    open: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let log = logging::init("riftsense")?;
    logging::install_panic_hook();
    tracing::info!("logs: {}", log.directory.display());

    let args = Args::parse();
    let paths = Paths::discover()?;
    tracing::info!("raiz: {}", paths.root.display());
    tracing::info!("modelos: {}", paths.models.display());
    let engine = tokio::task::spawn_blocking({
        let paths = paths.clone();
        move || Engine::load(paths)
    })
    .await
    .context("Fallo cargando el motor")??;
    tracing::info!(
        "motor listo ({} campeones, {} objetos)",
        engine.champions.len(),
        engine.items.len()
    );
    if !engine.model_notes.is_empty() {
        tracing::warn!("{}", engine.model_notes.join(" | "));
    }

    let lcu = riftsense::lcu::LcuService::new(&paths.root)?;
    let settings = StdRwLock::new(Settings::load(&paths));
    let state = Arc::new(api::AppState {
        engine: RwLock::new(Arc::new(engine)),
        paths,
        lcu,
        settings: Arc::new(settings),
    });

    let mut bound = None;
    let ports = std::iter::once(args.port).chain(8765..=8785);
    for port in ports {
        match TcpListener::bind((args.host.as_str(), port)).await {
            Ok(listener) => {
                bound = Some(listener);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::warn!("puerto {port} ocupado, probando el siguiente");
            }
            Err(error) => return Err(error).context("No se pudo abrir el puerto"),
        }
    }
    let listener = bound.context("No hay puertos libres")?;
    let address = listener.local_addr()?;
    let host = if args.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        args.host.as_str()
    };
    let url = format!("http://{host}:{}", address.port());
    println!("RiftSense (Rust) -> {url}");
    if args.open {
        let url = url.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(350)).await;
            if let Err(error) = open::that(&url) {
                tracing::warn!("No se pudo abrir el navegador: {error}");
            }
        });
    }

    axum::serve(listener, api::router(state))
        .await
        .context("Servidor HTTP detenido")?;
    Ok(())
}
