// Simple build script that copies static assets to `dist/` after wasm-pack build.
use std::process::Command;
use std::{env, fs, path::Path};

fn main() {
    // Only run the heavy wasm-pack build when targeting wasm32.
    let target = env::var("TARGET").unwrap_or_default();
    if target == "wasm32-unknown-unknown" {
        // wasm-pack is assumed available. If not, emit warning.
        let status = Command::new("wasm-pack")
            .args(["build", "--release", "--target", "web"])
            .status();

        if let Ok(st) = status {
            if !st.success() {
                println!("cargo:warning=wasm-pack build failed");
            }
        } else {
            println!("cargo:warning=wasm-pack not installed – skipping");
        }
    }

    // Copy static/ to dist/
    let out_dir = Path::new("dist");
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).ok();
    }
    fs::create_dir_all(out_dir).ok();

    let static_dir = Path::new("static");
    if static_dir.exists() {
        fn copy_dir(src: &Path, dst: &Path) {
            fs::create_dir_all(dst).ok();
            for entry in fs::read_dir(src).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let dst_path = dst.join(entry.file_name());
                if path.is_dir() {
                    copy_dir(&path, &dst_path);
                } else {
                    fs::copy(&path, &dst_path).ok();
                }
            }
        }
        copy_dir(static_dir, out_dir);
    }
}

