use std::env;
use std::fs;
use std::path::PathBuf;

fn ensure_external_bin_stub() {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let Ok(target_triple) = env::var("TARGET") else {
        return;
    };

    let extension = if target_triple.contains("windows") {
        ".exe"
    } else {
        ""
    };

    let binaries_dir = PathBuf::from(manifest_dir).join("binaries");
    if fs::create_dir_all(&binaries_dir).is_err() {
        return;
    }

    let node_path = binaries_dir.join(format!("browser-node-{}{}", target_triple, extension));
    if node_path.exists() {
        return;
    }

    // Keep local cargo check/test working before scripts/browser/prepare-sidecar.mjs runs.
    let _ = fs::write(node_path, [] as [u8; 0]);
}

fn main() {
    ensure_external_bin_stub();
    tauri_build::build()
}
