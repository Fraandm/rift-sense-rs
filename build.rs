use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
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
        if let Err(error) = embed_icon(&out_dir) {
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

fn embed_icon(out_dir: &Path) -> std::io::Result<()> {
    let rc = Path::new("assets/app.rc");
    if !rc.is_file() {
        return Ok(());
    }
    let object = out_dir.join("riftsense-resource.o");
    let status = Command::new("windres")
        .arg(rc)
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
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/RiftSense.ico");
    Ok(())
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
