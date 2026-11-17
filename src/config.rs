use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};

/// Rutas de datos y artefactos que comparte el runtime Rust con el proyecto.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub processed: PathBuf,
    pub artifacts: PathBuf,
    pub models: PathBuf,
    pub web: PathBuf,
}

#[derive(Deserialize)]
struct RootConfig {
    root: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let root = find_root()?;
        let processed = root.join("data").join("processed");
        let artifacts = root.join("artifacts");
        let models = find_models_dir(&root);
        let web = root.join("web");
        Ok(Self {
            root,
            processed,
            artifacts,
            models,
            web,
        })
    }

    pub fn draft_model(&self) -> PathBuf {
        self.models.join("draft_net.onnx")
    }

    pub fn build_model(&self) -> PathBuf {
        self.models.join("build_net.onnx")
    }
}

fn is_project_root(path: &Path) -> bool {
    path.join("data/processed/vocab.json").is_file()
        && path.join("artifacts/meta.json").is_file()
        && path.join("web/index.html").is_file()
}

fn find_root() -> Result<PathBuf> {
    if let Some(value) = env::var_os("RIFTSENSE_ROOT") {
        let candidate = PathBuf::from(value);
        if is_project_root(&candidate) {
            return Ok(candidate);
        }
        bail!(
            "RIFTSENSE_ROOT apunta a una carpeta sin data/processed/vocab.json, artifacts/meta.json y web/index.html: {}",
            candidate.display()
        );
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        candidates.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.extend(dir.ancestors().take(8).map(Path::to_path_buf));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));

    for candidate in &candidates {
        if is_project_root(candidate) {
            return Ok(normalize(candidate));
        }
    }

    for base in candidates.iter().filter_map(|p| p.parent()) {
        let config = base.join("riftsense.config.json");
        if let Ok(text) = std::fs::read_to_string(&config) {
            if let Ok(config) = serde_json::from_str::<RootConfig>(&text) {
                let root = if config.root.is_absolute() {
                    config.root
                } else {
                    base.join(config.root)
                };
                if is_project_root(&root) {
                    return Ok(normalize(&root));
                }
            }
        }
    }

    bail!(
        "No se encontro la raiz del proyecto RiftSense. Define RIFTSENSE_ROOT o crea rift-sense.config.json junto al ejecutable con {{\"root\": \"C:\\\\ruta\\\\al\\\\proyecto\"}}."
    )
}

fn find_models_dir(root: &Path) -> PathBuf {
    if let Some(value) = env::var_os("RIFTSENSE_MODELS_DIR") {
        return PathBuf::from(value);
    }
    let candidates = [
        root.join("riftsense-rs").join("models"),
        root.join("models"),
        env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("models")))
            .unwrap_or_else(|| PathBuf::from("models")),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"),
    ];
    for candidate in &candidates {
        if candidate.join("draft_net.onnx").is_file() || candidate.join("build_net.onnx").is_file()
        {
            return normalize(candidate);
        }
    }
    normalize(&candidates[0])
}

fn normalize(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Carga un JSON con un mensaje de error util.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("No se pudo leer {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("JSON invalido en {}", path.display()))
}

pub fn read_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.is_file() {
        return Ok(T::default());
    }
    read_json(path)
}
