//! Preferencias locales de la aplicación (interfaz, arranque y bandeja).

use crate::config::Paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub type SettingsHandle = Arc<RwLock<Settings>>;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "RiftSense";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Muestra las pestañas técnicas Modelo y Datos.
    pub show_advanced: bool,
    /// Registra RiftSense en el arranque de Windows (minimizado en la bandeja).
    pub start_with_windows: bool,
    /// Al cerrar o minimizar, mantiene la app en la bandeja del sistema.
    pub minimize_to_tray: bool,
}

impl Settings {
    pub fn load(paths: &Paths) -> Self {
        match load_from(&settings_file(paths)) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!("No se pudieron leer los ajustes: {error}");
                Self::default()
            }
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let path = settings_file(paths);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("No se pudo crear {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text)
            .with_context(|| format!("No se pudieron guardar los ajustes en {}", path.display()))
    }
}

fn settings_file(paths: &Paths) -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("RiftSense")
            .join("settings.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("riftsense")
            .join("settings.json");
    }
    paths.root.join("data").join("settings.json")
}

fn load_from(path: &Path) -> Result<Settings> {
    if !path.is_file() {
        return Ok(Settings::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("No se pudo leer {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("JSON invalido en {}", path.display()))
}

/// Devuelve el ejecutable de escritorio que debe arrancar con Windows.
fn desktop_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_string_lossy().to_ascii_lowercase();
    if stem == "riftsense-desktop" {
        return Some(exe);
    }
    let sibling = exe.with_file_name("riftsense-desktop.exe");
    sibling.is_file().then_some(sibling)
}

/// Indica si esta ejecución puede registrar el arranque automático.
pub fn startup_supported() -> bool {
    desktop_exe().is_some()
}

/// Crea o elimina la entrada de arranque automático.
///
/// Devuelve `false` cuando no hay un `riftsense-desktop.exe` disponible
/// (por ejemplo, al ejecutar solo la CLI desde una carpeta aislada).
pub fn apply_startup(enabled: bool) -> Result<bool> {
    let Some(exe) = desktop_exe() else {
        return Ok(false);
    };
    #[cfg(windows)]
    {
        use std::io::ErrorKind;
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

        let run = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
            .with_context(|| format!("No se pudo abrir {RUN_KEY}"))?;
        if enabled {
            let command = format!("\"{}\" --minimized", exe.display());
            run.set_value(RUN_VALUE, &command)
                .with_context(|| "No se pudo registrar el arranque con Windows")?;
        } else {
            match run.delete_value(RUN_VALUE) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).context("No se pudo quitar el arranque con Windows");
                }
            }
        }
        let _ = exe;
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, exe);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let settings = Settings::default();
        assert!(!settings.show_advanced);
        assert!(!settings.start_with_windows);
        assert!(!settings.minimize_to_tray);
    }

    #[test]
    fn roundtrips_with_camel_case() {
        let settings = Settings {
            show_advanced: true,
            start_with_windows: false,
            minimize_to_tray: true,
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert!(text.contains("\"showAdvanced\":true"));
        assert!(text.contains("\"minimizeToTray\":true"));
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, settings);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = std::env::temp_dir().join("riftsense-settings-missing.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_from(&path).unwrap(), Settings::default());
    }
}
