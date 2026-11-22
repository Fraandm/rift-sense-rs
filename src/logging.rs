//! Logs persistentes y utilidades de diagnóstico.

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub struct LogGuard {
    pub directory: PathBuf,
}

pub fn log_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("RIFTSENSE_LOG_DIR") {
        return PathBuf::from(value);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("RiftSense").join("logs");
    }
    PathBuf::from("logs")
}

#[derive(Clone)]
struct SharedFile(Arc<Mutex<std::fs::File>>);

struct SharedFileWriter(Arc<Mutex<std::fs::File>>);

impl Write for SharedFileWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("log lock poisoned"))?;
        file.write_all(buffer)?;
        file.flush()?;
        file.sync_data()?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("log lock poisoned"))?
            .flush()
    }
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for SharedFile {
    type Writer = SharedFileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedFileWriter(self.0.clone())
    }
}

pub fn init(component: &str) -> Result<LogGuard> {
    let directory = log_dir();
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("No se pudo crear {}", directory.display()))?;
    cleanup_old_logs(&directory);
    let path = directory.join(format!("{component}.log"));
    rotate_if_large(&path);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("No se pudo abrir {}", path.display()))?;
    let writer = SharedFile(Arc::new(Mutex::new(file)));
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false);
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true);
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .try_init();
    Ok(LogGuard { directory })
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        previous(info);
    }));
}

fn rotate_if_large(path: &std::path::Path) {
    let large = std::fs::metadata(path)
        .map(|metadata| metadata.len() > 5 * 1024 * 1024)
        .unwrap_or(false);
    if !large {
        return;
    }
    let backup = path.with_extension("log.1");
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, backup);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_log_file() {
        let directory = std::env::temp_dir().join(format!(
            "riftsense-log-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        unsafe {
            std::env::set_var("RIFTSENSE_LOG_DIR", &directory);
        }
        let guard = init("test").expect("log init");
        tracing::info!("diagnostic marker");
        let path = guard.directory.join("test.log");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        unsafe {
            std::env::remove_var("RIFTSENSE_LOG_DIR");
        }
        let _ = std::fs::remove_dir_all(&directory);
        assert!(text.contains("diagnostic marker"), "log text: {text:?}");
    }
}

fn cleanup_old_logs(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(14 * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_log = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".log"));
        if !is_log {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| modified < cutoff)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(path);
        }
    }
}
