use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
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

const SCAN_TIMEOUT: Duration = Duration::from_secs(45);
const PROGRESS_EVERY: usize = 25;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Searching,
    Found,
    NotFound,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Searching => "searching",
            Phase::Found => "found",
            Phase::NotFound => "not_found",
        }
    }
}

struct SearchState {
    generation: u64,
    phase: Phase,
    searching: bool,
    drives: Vec<String>,
    scanned: usize,
    current_drive: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    error: Option<String>,
    incomplete: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: Phase::Idle,
            searching: false,
            drives: Vec::new(),
            scanned: 0,
            current_drive: None,
            started_at: None,
            finished_at: None,
            error: None,
            incomplete: false,
        }
    }
}

struct Shared {
    location: Mutex<Option<Location>>,
    search: Mutex<SearchState>,
}

/// Localiza League of Legends y mantiene el estado de la búsqueda en segundo
/// plano, igual que el localizador del runtime Python.
#[derive(Clone)]
pub struct Locator {
    saved_path: PathBuf,
    shared: Arc<Shared>,
}

impl Locator {
    pub fn new(root: &Path) -> Self {
        let saved_path = root.join("data").join("client_location.json");
        let locator = Self {
            saved_path,
            shared: Arc::new(Shared {
                location: Mutex::new(None),
                search: Mutex::new(SearchState::default()),
            }),
        };
        locator.refresh();
        locator
    }

    /// Comprueba solo las fuentes rápidas (entorno, ruta guardada, Riot,
    /// registro y rutas habituales). No inicia un escaneo de discos.
    pub fn refresh(&self) -> serde_json::Value {
        let found = self.find_fast();
        if let Some(location) = &found {
            self.store(location, false);
        }
        self.status_json(found.is_some())
    }

    /// Inicia (o reutiliza) la búsqueda en segundo plano y devuelve el estado
    /// inmediatamente, sin bloquear la petición HTTP.
    pub fn start(&self, force: bool) -> serde_json::Value {
        {
            let mut search = self.shared.search.lock().unwrap();
            if search.searching {
                drop(search);
                return self.status_json(false);
            }
            if !force {
                let found = self.shared.location.lock().unwrap().is_some();
                if search.phase == Phase::Found && found {
                    drop(search);
                    return self.status_json(false);
                }
            }

            let roots = drive_roots();
            search.generation = search.generation.wrapping_add(1);
            let generation = search.generation;
            search.phase = Phase::Searching;
            search.searching = true;
            search.drives = roots.iter().map(|root| drive_label(root)).collect();
            search.scanned = 0;
            search.current_drive = search.drives.first().cloned();
            search.started_at = Some(Utc::now().to_rfc3339());
            search.finished_at = None;
            search.error = None;
            search.incomplete = false;

            tracing::info!(
                "buscando League of Legends en {}",
                if search.drives.is_empty() {
                    "el sistema".to_string()
                } else {
                    search.drives.join(", ")
                }
            );

            let worker = self.clone();
            let spawned = thread::Builder::new()
                .name("lol-client-locator".to_string())
                .spawn(move || worker.run(generation, roots));
            if let Err(error) = spawned {
                search.searching = false;
                search.phase = Phase::NotFound;
                search.finished_at = Some(Utc::now().to_rfc3339());
                search.error = Some(format!("No se pudo iniciar la búsqueda: {error}"));
            }
        }
        self.status_json(false)
    }

    /// Fija manualmente la carpeta de League of Legends y cancela cualquier
    /// búsqueda en curso.
    pub fn set_custom(&self, raw: &str) -> Result<serde_json::Value> {
        let raw = raw.trim().trim_matches('"').trim_matches('\'');
        let location = valid_install(Path::new(raw), "manual")
            .or_else(|| valid_install(&Path::new(raw).join("League of Legends"), "manual"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No se encontró LeagueClient.exe ni lockfile en esa ruta. Selecciona la carpeta que contiene LeagueClient.exe."
                )
            })?;

        {
            let mut search = self.shared.search.lock().unwrap();
            search.generation = search.generation.wrapping_add(1);
            search.searching = false;
            search.phase = Phase::Found;
            search.error = None;
            search.finished_at = Some(Utc::now().to_rfc3339());
            search.incomplete = false;
            *self.shared.location.lock().unwrap() = Some(location.clone());
            self.save(&location);
        }
        tracing::info!(
            "ruta manual de League of Legends: {}",
            location
                .install_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        );
        Ok(self.status_json(true))
    }

    /// Devuelve el lockfile válido, lanzando una búsqueda en segundo plano la
    /// primera vez que hace falta y aún no hay una ubicación conocida.
    pub fn lockfile(&self) -> Option<Lockfile> {
        self.ensure_started();
        self.ensure_location();
        if let Some(location) = self.shared.location.lock().unwrap().clone() {
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
        let (
            phase,
            searching,
            drives,
            scanned,
            current_drive,
            started_at,
            finished_at,
            error,
            incomplete,
        ) = {
            let search = self.shared.search.lock().unwrap();
            (
                search.phase,
                search.searching,
                search.drives.clone(),
                search.scanned,
                search.current_drive.clone(),
                search.started_at.clone(),
                search.finished_at.clone(),
                search.error.clone(),
                search.incomplete,
            )
        };
        let location = self.shared.location.lock().unwrap().clone();
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
        let saved = if self.saved_path.is_file() {
            Some(self.saved_path.to_string_lossy().to_string())
        } else {
            None
        };
        let mut state = phase.as_str();
        if found_override && phase != Phase::Searching {
            state = "found";
        }
        json!({
            "state": state,
            "searching": searching,
            "installPath": install.map(|path| path.to_string_lossy().to_string()),
            "lockfilePath": lockfile.map(|path| path.to_string_lossy().to_string()),
            "drive": location.as_ref().and_then(Location::drive),
            "source": location.as_ref().map(|loc| loc.source.clone()),
            "drives": drives,
            "scannedDirs": scanned,
            "currentDrive": current_drive,
            "startedAt": started_at,
            "finishedAt": finished_at,
            "error": error,
            "incomplete": incomplete,
            "saved": saved,
        })
    }

    fn run(&self, generation: u64, roots: Vec<PathBuf>) {
        let found = self
            .find_fast()
            .or_else(|| self.scan_roots(generation, &roots));

        let mut search = self.shared.search.lock().unwrap();
        if search.generation != generation {
            return;
        }
        search.searching = false;
        search.finished_at = Some(Utc::now().to_rfc3339());
        if let Some(location) = &found {
            search.phase = Phase::Found;
            search.error = None;
            search.incomplete = false;
            *self.shared.location.lock().unwrap() = Some(location.clone());
            tracing::info!(
                "League of Legends encontrado en {} -> {} ({})",
                location.drive().unwrap_or_else(|| "?".to_string()),
                location
                    .install_dir
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                location.source
            );
            self.save(location);
        } else {
            search.phase = Phase::NotFound;
            search.error = Some(if search.incomplete {
                "Búsqueda incompleta: se agotó el tiempo. Pulsa Buscar de nuevo.".to_string()
            } else {
                "No se encontró League of Legends en este PC.".to_string()
            });
        }
    }

    fn scan_roots(&self, generation: u64, roots: &[PathBuf]) -> Option<Location> {
        let deadline = Instant::now() + SCAN_TIMEOUT;
        let mut scanned = 0usize;
        for root in roots {
            {
                let mut search = self.shared.search.lock().unwrap();
                if search.generation != generation {
                    return None;
                }
                search.current_drive = Some(drive_label(root));
            }
            let mut queue = VecDeque::from([(root.clone(), 0usize)]);
            while let Some((path, depth)) = queue.pop_front() {
                if Instant::now() > deadline {
                    let mut search = self.shared.search.lock().unwrap();
                    if search.generation == generation {
                        search.incomplete = true;
                    }
                    return None;
                }
                if depth > 4 {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&path) else {
                    continue;
                };
                scanned += 1;
                if scanned % PROGRESS_EVERY == 0 {
                    let mut search = self.shared.search.lock().unwrap();
                    if search.generation != generation {
                        return None;
                    }
                    search.scanned = scanned;
                }

                let mut dirs = Vec::new();
                for entry in entries.flatten() {
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if file_type.is_file() && name == "leagueclient.exe" {
                        if let Some(location) = valid_install(&path, "scan") {
                            let mut search = self.shared.search.lock().unwrap();
                            if search.generation == generation {
                                search.scanned = scanned;
                            }
                            return Some(location);
                        }
                    }
                    if file_type.is_dir()
                        && !SKIP_DIRS.contains(&name.as_str())
                        && !name.contains("pbe")
                    {
                        dirs.push(entry.path());
                    }
                }
                for dir in dirs {
                    queue.push_back((dir, depth + 1));
                }
            }
        }

        let mut search = self.shared.search.lock().unwrap();
        if search.generation == generation {
            search.scanned = scanned;
        }
        None
    }

    fn ensure_started(&self) {
        let should_start = {
            let search = self.shared.search.lock().unwrap();
            search.phase == Phase::Idle && !search.searching
        };
        if should_start && self.shared.location.lock().unwrap().is_none() {
            self.start(true);
        }
    }

    fn ensure_location(&self) {
        let missing = self.shared.location.lock().unwrap().is_none();
        if missing {
            if let Some(location) = self.find_fast() {
                self.store(&location, false);
            }
        }
    }

    fn store(&self, location: &Location, save: bool) {
        let mut search = self.shared.search.lock().unwrap();
        search.phase = Phase::Found;
        search.error = None;
        search.incomplete = false;
        *self.shared.location.lock().unwrap() = Some(location.clone());
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
            read_lockfile(&path)?;
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

fn strip_quotes(value: &str) -> &str {
    value.trim().trim_matches('"').trim_matches('\'')
}

fn from_env() -> Option<Location> {
    for name in ["LOL_INSTALL_DIR", "LOL_LOCKFILE"] {
        let Ok(value) = std::env::var(name) else {
            continue;
        };
        let value = strip_quotes(&value);
        if value.is_empty() {
            continue;
        }
        if let Some(location) = valid_install(Path::new(value), "env") {
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

fn drive_label(path: &Path) -> String {
    path.components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .filter(|text| text.len() == 2 && text.ends_with(':'))
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

#[cfg(windows)]
pub fn drive_roots() -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStrExt;

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    let mut roots = Vec::new();
    unsafe {
        let mask = windows_sys::Win32::Storage::FileSystem::GetLogicalDrives();
        for index in 0u32..26 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let letter = (b'A' + index as u8) as char;
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = std::ffi::OsStr::new(&root)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let drive_type = windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr());
            if drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_FIXED {
                roots.push(PathBuf::from(root));
            }
        }
    }
    roots
}

#[cfg(not(windows))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("riftsense-{name}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn reads_and_rejects_lockfiles() {
        let dir = temp_dir("lockfile");
        let path = dir.join("lockfile");
        fs::write(&path, "LeagueClient:4242:43123:secret:https").unwrap();
        let info = read_lockfile(&path).unwrap();
        assert_eq!(info.pid, 4242);
        assert_eq!(info.port, 43123);
        assert_eq!(info.password, "secret");
        assert_eq!(info.protocol, "https");
        fs::write(&path, "not a lockfile").unwrap();
        assert!(read_lockfile(&path).is_none());
        fs::write(&path, "LeagueClient:1:0:x:https").unwrap();
        assert!(read_lockfile(&path).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manual_path_sets_found_state() {
        let root = temp_dir("root");
        let install = root.join("League of Legends");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("LeagueClient.exe"), b"").unwrap();
        let locator = Locator::new(&root);
        let status = locator.set_custom(install.to_str().unwrap()).unwrap();
        assert_eq!(status["state"], "found");
        assert_eq!(status["searching"], false);
        assert!(
            status["installPath"]
                .as_str()
                .unwrap()
                .ends_with("League of Legends")
        );
        assert!(root.join("data").join("client_location.json").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_reports_progress_and_finds_install() {
        let root = temp_dir("scan");
        fs::write(root.join("LeagueClient.exe"), b"").unwrap();
        let locator = Locator::new(&root);
        {
            let mut search = locator.shared.search.lock().unwrap();
            search.generation = 1;
        }
        let found = locator.scan_roots(1, std::slice::from_ref(&root));
        assert!(found.is_some());
        let status = locator.status_json(false);
        assert!(status["scannedDirs"].as_u64().unwrap() >= 1);
        let _ = fs::remove_dir_all(&root);
    }
}
