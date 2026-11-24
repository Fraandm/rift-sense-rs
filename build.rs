use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // La SPA se embebe con RustEmbed: sin esta linea, un cambio de CSS/JS no
    // siempre recompila el binario y la release puede servir la UI anterior.
    println!("cargo:rerun-if-changed=web");
    if !cfg!(windows) {
        return;
    }
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let out_dir = PathBuf::from(out_dir);
    // target/<profile>/build/<crate>/<hash>/out
    let profile_dir = out_dir
        .ancestors()
        .take(6)
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "debug" || name == "release")
        })
        .or_else(|| out_dir.ancestors().nth(4))
        .map(Path::to_path_buf);
    let Some(profile_dir) = profile_dir else {
        return;
    };

    if env::var_os("RIFTSENSE_SKIP_ICON").is_none() {
        if let Err(error) = embed_resources(&out_dir) {
            println!("cargo:warning=No se pudo incrustar el icono de RiftSense: {error}");
        }
    }

    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")));
    let Some(cargo_home) = cargo_home else {
        return;
    };
    let registry = cargo_home.join("registry").join("src");
    if let Some(dll) = find_loader(&registry) {
        let _ = fs::copy(dll, profile_dir.join("WebView2Loader.dll"));
    }
}

fn embed_resources(out_dir: &Path) -> std::io::Result<()> {
    let icon_rc = Path::new("assets/app.rc");
    if !icon_rc.is_file() {
        return Ok(());
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let version_rc = out_dir.join("riftsense-version.rc");
    fs::write(&version_rc, version_info(&version))?;
    for (rc, object_name) in [
        (icon_rc.to_path_buf(), "riftsense-icon.o"),
        (version_rc, "riftsense-version.o"),
    ] {
        let object = out_dir.join(object_name);
        let status = Command::new("windres")
            .arg(&rc)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&object)
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other(format!(
                "windres termino con codigo {status}"
            )));
        }
        println!("cargo:rustc-link-arg={}", object.display());
    }
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/RiftSense.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    Ok(())
}

fn version_info(version: &str) -> String {
    let mut fields = version.split('.').map(|field| {
        field
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
    });
    let mut next = || {
        fields
            .next()
            .filter(|field| !field.is_empty())
            .unwrap_or_else(|| "0".into())
    };
    let (major, minor, patch) = (next(), next(), next());
    format!(
        r#"1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "RiftSense"
      VALUE "FileDescription", "RiftSense"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "riftsense"
      VALUE "OriginalFilename", "riftsense.exe"
      VALUE "ProductName", "RiftSense"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

fn find_loader(root: &Path) -> Option<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("webview2-com-sys-"))
        {
            let dll = path.join("x64").join("WebView2Loader.dll");
            if dll.is_file() {
                return Some(dll);
            }
        }
        if let Some(found) = find_loader(&path) {
            return Some(found);
        }
    }
    None
}
