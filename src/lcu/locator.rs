use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const COMMON_SUBPATHS: &[&str] = &[
    "Riot Games/League of Legends",
    "Games/League of Legends",
    "Games/Riot Games/League of Legends",
    "Program Files/Riot Games/League of Legends",
    "Program Files (x86)/Riot Games/League of Legends",
    "League of Legends",
];

const SKIP_DIRS: &[&str] = &[
    "windows",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "perflogs",
    "config.msi",
    "node_modules",
    ".git",
    ".venv",
    "__pycache__",
    "appdata",
    "onedrive",
    "programdata",
];

#[derive(Clone, Debug)]
pub struct Lockfile {
    pub path: PathBuf,
    pub pid: i32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

#[derive(Clone, Debug)]
pub struct Location {
    pub install_dir: Option<PathBuf>,
    pub lockfile: Option<PathBuf>,
    pub source: String,
}

impl Location {
    pub fn drive(&self) -> Option<String> {
        let target = self.install_dir.as_ref().or(self.lockfile.as_ref())?;
        target.components().next().and_then(|component| {
            let text = component.as_os_str().to_string_lossy();
            if text.len() == 2 && text.ends_with(':') {
                Some(text.to_string())
            } else {
                None
            }
        })
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SavedLocation {
    install_dir: Option<String>,
    lockfile: Option<String>,
    source: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
}

pub struct Locator {
    saved_path: PathBuf,
    location: Mutex<Option<Location>>,
    last_error: Mutex<Option<String>>,
}

impl Locator {
    pub fn new(root: &Path) -> Self {
        let saved_path = root.join("data").join("client_location.json");
        let locator = Self {
            saved_path,
            location: Mutex::new(None),
            last_error: Mutex::new(None),
        };
        locator.refresh();
        locator
    }

    pub fn refresh(&self) -> serde_json::Value {
        let found = self.find_fast();
        if let Some(location) = &found {
            *self.location.lock().unwrap() = Some(location.clone());
            *self.last_error.lock().unwrap() = None;
        }
        self.status_json(found.is_some())
    }

    pub fn scan(&self) -> serde_json::Value {
        if let Some(location) = self.find_fast() {
            self.store(&location, true);
            return self.status_json(true);
        }
        match self.scan_drives() {
            Some(location) => {
                self.store(&location, true);
                self.status_json(true)
            }
            None => {
                *self.last_error.lock().unwrap() =
                    Some("No se encontró League of Legends en este PC.".to_string());
                self.status_json(false)
            }
        }
    }

    pub fn set_custom(&self, raw: &str) -> Result<serde_json::Value> {
        let location = valid_install(Path::new(raw), "manual")
            .or_else(|| valid_install(&Path::new(raw).join("League of Legends"), "manual"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No se encontró LeagueClient.exe ni lockfile en esa ruta. Selecciona la carpeta que contiene LeagueClient.exe."
                )
            })?;
        self.store(&location, true);
        Ok(self.status_json(true))
    }

    pub fn lockfile(&self) -> Option<Lockfile> {
        self.ensure_location();
        if let Some(location) = self.location.lock().unwrap().clone() {
            for candidate in [
                location.lockfile,
                location.install_dir.map(|dir| dir.join("lockfile")),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(info) = read_lockfile(&candidate) {
                    return Some(info);
                }
            }
        }
        if let Some(location) = self.find_fast() {
            self.store(&location, false);
            if let Some(dir) = &location.install_dir {
                if let Some(info) = read_lockfile(&dir.join("lockfile")) {
                    return Some(info);
                }
            }
        }
        None
    }

    pub fn status_json(&self, found_override: bool) -> serde_json::Value {
        let location = self.location.lock().unwrap().clone();
        let install = location.as_ref().and_then(|loc| loc.install_dir.clone());
        let lockfile = location
            .as_ref()
            .and_then(|loc| loc.lockfile.clone())
            .filter(|path| read_lockfile(path).is_some())
            .or_else(|| {
                install
                    .as_ref()
                    .map(|dir| dir.join("lockfile"))
                    .filter(|path| read_lockfile(path).is_some())
            });
        let found = found_override || location.is_some();
        let saved = if self.saved_path.is_file() {
            Some(self.saved_path.to_string_lossy().to_string())
        } else {
            None
        };
        json!({
            "state": if found { "found" } else { "not_found" },
            "searching": false,
            "installPath": install.map(|path| path.to_string_lossy().to_string()),
            "lockfilePath": lockfile.map(|path| path.to_string_lossy().to_string()),
            "drive": location.as_ref().and_then(Location::drive),
            "source": location.as_ref().map(|loc| loc.source.clone()),
            "drives": drive_roots().iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
            "scannedDirs": 0,
            "currentDrive": null,
            "startedAt": null,
            "finishedAt": null,
            "error": self.last_error.lock().unwrap().clone(),
            "incomplete": false,
            "saved": saved,
        })
    }

    fn ensure_location(&self) {
        if self.location.lock().unwrap().is_none() {
            if let Some(location) = self.find_fast() {
                self.store(&location, false);
            }
        }
    }

    fn store(&self, location: &Location, save: bool) {
        *self.location.lock().unwrap() = Some(location.clone());
        *self.last_error.lock().unwrap() = None;
        if save {
            self.save(location);
        }
    }

    fn save(&self, location: &Location) {
        let saved = SavedLocation {
            install_dir: location
                .install_dir
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            lockfile: location
                .lockfile
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            source: Some(location.source.clone()),
            updated_at: Some(Utc::now().to_rfc3339()),
        };
        if let Some(parent) = self.saved_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string(&saved) {
            let _ = std::fs::write(&self.saved_path, text);
        }
    }

    fn find_fast(&self) -> Option<Location> {
        from_env()
            .or_else(|| from_saved(&self.saved_path))
            .or_else(from_riot_metadata)
            .or_else(from_riot_installs)
            .or_else(from_registry)
            .or_else(from_common)
    }

    fn scan_drives(&self) -> Option<Location> {
        let deadline = Instant::now() + Duration::from_secs(20);
        for root in drive_roots() {
            let mut queue = VecDeque::from([(root, 0usize)]);
            while let Some((path, depth)) = queue.pop_front() {
                if Instant::now() > deadline {
                    return None;
                }
                if depth > 4 {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&path) else {
                    continue;
                };
                let mut dirs = Vec::new();
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if entry_path.is_file() && name == "leagueclient.exe" {
                        if let Some(location) = valid_install(&path, "scan") {
                            return Some(location);
                        }
                    }
                    if entry_path.is_dir()
                        && !SKIP_DIRS.contains(&name.as_str())
                        && !name.contains("pbe")
                    {
                        dirs.push(entry_path);
                    }
                }
                for dir in dirs {
                    queue.push_back((dir, depth + 1));
                }
            }
        }
        None
    }
}

pub fn read_lockfile(path: &Path) -> Option<Lockfile> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() < 10 {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let parts: Vec<&str> = raw.trim().splitn(5, ':').collect();
    if parts.len() < 5 || !parts[0].to_lowercase().contains("leagueclient") {
        return None;
    }
    let pid = parts[1].parse::<i32>().ok()?;
    let port = parts[2].parse::<u16>().ok()?;
    let password = parts[3].to_string();
    let protocol = if parts[4].trim().is_empty() {
        "https".to_string()
    } else {
        parts[4].trim().to_string()
    };
    if password.is_empty() || port == 0 {
        return None;
    }
    Some(Lockfile {
        path: path.to_path_buf(),
        pid,
        port,
        password,
        protocol,
    })
}

fn valid_install(path: &Path, source: &str) -> Option<Location> {
    let path = dunce::canonicalize(path).ok()?;
    let mut path = path;
    if path.is_file() {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name == "lockfile" {
            if read_lockfile(&path).is_none() {
                return None;
            }
            let install = path.parent()?.to_path_buf();
            return Some(Location {
                install_dir: Some(install),
                lockfile: Some(path),
                source: source.to_string(),
            });
        }
        if name == "leagueclient.exe" || name == "leagueclientux.exe" {
            path = path.parent()?.to_path_buf();
        } else {
            return None;
        }
    }
    if !path.is_dir() {
        return None;
    }

    let mut candidates = vec![path.clone()];
    if path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        == Some("riot games".to_string())
    {
        candidates.push(path.join("League of Legends"));
    }
    if path.join("League of Legends").is_dir() {
        candidates.push(path.join("League of Legends"));
    }

    for candidate in candidates {
        if !candidate.is_dir() {
            continue;
        }
        let lock = candidate.join("lockfile");
        if read_lockfile(&lock).is_some() {
            return Some(Location {
                install_dir: Some(candidate),
                lockfile: Some(lock),
                source: source.to_string(),
            });
        }
        if candidate.join("LeagueClient.exe").is_file()
            || candidate.join("LeagueClientUx.exe").is_file()
        {
            return Some(Location {
                install_dir: Some(candidate),
                lockfile: if lock.is_file() { Some(lock) } else { None },
                source: source.to_string(),
            });
        }
    }
    None
}

fn from_env() -> Option<Location> {
    for name in ["LOL_INSTALL_DIR", "LOL_LOCKFILE"] {
        let Ok(value) = std::env::var(name) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        if let Some(location) = valid_install(Path::new(value.trim()), "env") {
            return Some(location);
        }
    }
    None
}

fn from_saved(path: &Path) -> Option<Location> {
    let text = std::fs::read_to_string(path).ok()?;
    let saved: SavedLocation = serde_json::from_str(&text).ok()?;
    for value in [saved.install_dir, saved.lockfile].into_iter().flatten() {
        if let Some(location) = valid_install(Path::new(&value), "saved") {
            return Some(location);
        }
    }
    None
}

fn from_riot_metadata() -> Option<Location> {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
    let base = Path::new(&program_data).join("Riot Games").join("Metadata");
    let folder = base.join("league_of_legends.live");
    let yaml = folder.join("league_of_legends.live.product_settings.yaml");
    let text = std::fs::read_to_string(yaml).ok()?;
    let mut full = None;
    let mut root = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("product_install_full_path:") {
            full = Some(value.trim().trim_matches('"').to_string());
        } else if let Some(value) = line.strip_prefix("product_install_root:") {
            root = Some(value.trim().trim_matches('"').to_string());
        }
    }
    if let Some(full) = full {
        return valid_install(Path::new(&full), "riot-metadata");
    }
    if let Some(root) = root {
        return valid_install(&Path::new(&root).join("League of Legends"), "riot-metadata");
    }
    None
}

fn from_riot_installs() -> Option<Location> {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
    let path = Path::new(&program_data)
        .join("Riot Games")
        .join("RiotClientInstalls.json");
    let text = std::fs::read_to_string(path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&text).ok()?;
    let associated = data.get("associated_client")?.as_object()?;
    for install in associated.keys() {
        if install.to_lowercase().contains("pbe") {
            continue;
        }
        if let Some(location) = valid_install(Path::new(install), "riot-installs") {
            return Some(location);
        }
    }
    None
}

fn from_registry() -> Option<Location> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let roots = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];
    let mut candidates = Vec::new();
    for (hive, subkey) in roots {
        let Ok(key) = RegKey::predef(hive).open_subkey(subkey) else {
            continue;
        };
        for name in key.enum_keys().flatten() {
            let Ok(child) = key.open_subkey(&name) else {
                continue;
            };
            let display: String = child.get_value("DisplayName").unwrap_or_default();
            let display_lower = display.trim().to_lowercase();
            if display_lower.contains("pbe") {
                continue;
            }
            if display_lower != "league of legends" && display_lower != "league of legends live" {
                continue;
            }
            let location: String = child.get_value("InstallLocation").unwrap_or_default();
            if !location.trim().is_empty() {
                candidates.push(location);
            }
        }
    }
    for candidate in candidates {
        if let Some(location) = valid_install(Path::new(&candidate), "registry") {
            return Some(location);
        }
    }
    None
}

fn from_common() -> Option<Location> {
    for root in drive_roots() {
        for relative in COMMON_SUBPATHS {
            if let Some(location) = valid_install(&root.join(relative), "common") {
                return Some(location);
            }
        }
    }
    None
}

pub fn drive_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for letter in 'C'..='Z' {
        let root = PathBuf::from(format!("{letter}:\\"));
        if root.is_dir() {
            roots.push(root);
        }
    }
    roots
}

pub fn lockfile_paths(location: &Location) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(lockfile) = &location.lockfile {
        paths.push(lockfile.clone());
    }
    if let Some(dir) = &location.install_dir {
        paths.push(dir.join("lockfile"));
    }
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}
