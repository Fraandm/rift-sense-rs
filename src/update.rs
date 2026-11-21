use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const REPO: &str = "Fraandm/rift-sense-rs";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub notes: Option<String>,
    pub html_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
    pub error: Option<String>,
}

struct ReleaseInfo {
    status: UpdateStatus,
    asset: Option<GithubAsset>,
}

pub fn current_version() -> String {
    std::env::var("RIFTSENSE_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let latest = normalize_tag(latest);
    let current = normalize_tag(current);
    match (Version::parse(&latest), Version::parse(&current)) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => false,
    }
}

fn releases_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

async fn fetch_release() -> Result<ReleaseInfo> {
    let current = current_version();
    let client = reqwest::Client::builder()
        .user_agent(format!("RiftSense/{current}"))
        .timeout(Duration::from_secs(20))
        .build()?;
    let response = client
        .get(releases_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("No se pudo consultar GitHub Releases")?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(ReleaseInfo {
            status: UpdateStatus {
                current,
                latest: None,
                available: false,
                notes: None,
                html_url: None,
                asset_name: None,
                asset_size: None,
                error: Some("Todavía no hay releases publicadas.".to_string()),
            },
            asset: None,
        });
    }
    if !response.status().is_success() {
        bail!("GitHub respondió HTTP {}", response.status());
    }

    let release: GithubRelease = response
        .json()
        .await
        .context("GitHub devolvió una respuesta inválida")?;
    let latest = normalize_tag(&release.tag_name);
    let asset = release.assets.into_iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.contains("setup") && name.ends_with(".exe")
    });
    let asset_name = asset.as_ref().map(|asset| asset.name.clone());
    let asset_size = asset.as_ref().and_then(|asset| asset.size);
    let available = is_newer(&latest, &current) && asset.is_some();
    let error = if asset.is_none() {
        Some("La última release no incluye un instalador .exe.".to_string())
    } else {
        None
    };

    Ok(ReleaseInfo {
        status: UpdateStatus {
            current,
            latest: Some(latest),
            available,
            notes: release.body,
            html_url: release.html_url,
            asset_name,
            asset_size,
            error,
        },
        asset,
    })
}

/// Consulta la última release pública del repositorio.
pub async fn check() -> Result<UpdateStatus> {
    Ok(fetch_release().await?.status)
}

/// Descarga el instalador de la última release, lo ejecuta en modo silencioso
/// y termina este proceso para que Inno Setup pueda reemplazar los binarios.
pub async fn install() -> Result<PathBuf> {
    let release = fetch_release().await?;
    if !release.status.available {
        bail!(
            "{}",
            release
                .status
                .error
                .unwrap_or_else(|| "No hay una versión nueva disponible.".to_string())
        );
    }
    let asset = release.asset.context("La release no incluye instalador")?;
    let file_name = Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("RiftSense-Setup.exe")
        .to_string();
    let target = std::env::temp_dir().join(file_name);

    let client = reqwest::Client::builder()
        .user_agent(format!("RiftSense/{}", current_version()))
        .timeout(Duration::from_secs(300))
        .build()?;
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("No se pudo descargar el instalador")?;
    if !response.status().is_success() {
        bail!(
            "La descarga del instalador devolvió HTTP {}",
            response.status()
        );
    }
    let bytes = response
        .bytes()
        .await
        .context("No se pudo leer el instalador")?;
    tokio::fs::write(&target, &bytes)
        .await
        .with_context(|| format!("No se pudo guardar {}", target.display()))?;

    let mut command = Command::new(&target);
    command.args([
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .with_context(|| format!("No se pudo iniciar {}", target.display()))?;

    // Dar tiempo a que la respuesta HTTP salga antes de cerrar la app.
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        std::process::exit(0);
    });
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("no-version", "0.2.0"));
    }

    #[test]
    fn normalizes_tags() {
        assert_eq!(normalize_tag("v0.2.0"), "0.2.0");
        assert_eq!(normalize_tag("V1.2.3"), "1.2.3");
        assert_eq!(normalize_tag(" 0.2.0 "), "0.2.0");
    }
}
