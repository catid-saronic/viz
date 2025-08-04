// Simple build script that copies static assets to `dist/` after wasm-pack build.
use std::process::Command;
use std::{env, fs, path::Path};

fn main() {
    // ----------------------------------------------------------------------------------
    // 1. Avoid invoking `wasm-pack` from within the build-script
    // ----------------------------------------------------------------------------------
    // Running `wasm-pack` here starts a *second* Cargo build for the same crate which
    // dramatically increases compile times (and can even lead to recursive builds on
    // some setups).  Instead, invoke `wasm-pack` explicitly from CI scripts or a make
    // recipe so that it is only executed when you really need the JS bindings.

    // We still keep the possibility to opt-in during local development via an env-var:
    //     WASM_BUILD=1 cargo build --target wasm32-unknown-unknown --release
    // This way the default path is fast, but developers can request the full build.

    let run_wasm_pack = env::var("WASM_BUILD").ok().filter(|v| v == "1").is_some();
    let is_wasm_target = env::var("TARGET").map(|t| t == "wasm32-unknown-unknown").unwrap_or(false);

    if run_wasm_pack && is_wasm_target {
        let status = Command::new("wasm-pack")
            .args(["build", "--release", "--target", "web", "--out-dir", "pkg", "--mode", "no-install"])
            .status();

        match status {
            Ok(st) if !st.success() => {
                println!("cargo:warning=wasm-pack build failed");
            }
            Err(err) => {
                println!("cargo:warning=failed to spawn wasm-pack: {err}");
            }
            _ => {}
        }
    }

    // ----------------------------------------------------------------------------------
    // 2. Copy the static assets to the distributable directory
    // ----------------------------------------------------------------------------------
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

    // Ensure Cargo only re-runs this script when the *inputs* change, not every build.
    println!("cargo:rerun-if-changed=static");
    println!("cargo:rerun-if-env-changed=WASM_BUILD");
}
