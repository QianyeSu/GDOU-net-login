use std::fs;
use std::path::Path;

fn main() {
    watch_frontend_dist(Path::new("../frontend/dist"));
    tauri_build::build()
}

fn watch_frontend_dist(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch_frontend_dist(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
