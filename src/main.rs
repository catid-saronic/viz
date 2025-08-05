//! Host-side helper: `cargo run` builds the WASM site, starts a local HTTP
//! server for `dist/`, and (if available) exposes it via ngrok.

use std::process::{Command, Stdio};
use std::{env, thread, time::Duration};

fn main() {
    // Only meaningful on non-wasm targets.
    if env::var("TARGET").unwrap_or_default() == "wasm32-unknown-unknown" {
        return;
    }

    // 1. Ensure crate builds (cargo build) then compile wasm via wasm-pack into static/pkg
    println!("Running cargo build …");
    let cargo_status = Command::new("cargo")
        .args(["build", "--release"])
        .status()
        .expect("failed to run cargo build");
    if !cargo_status.success() {
        eprintln!("cargo build failed");
        std::process::exit(1);
    }

    // Require wasm-pack to be present.
    if Command::new("wasm-pack")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("wasm-pack not found. Please install it first – see README.md.");
        std::process::exit(1);
    }

    let wasm_pack_exe = "wasm-pack";

    // Ensure wasm32 target is added
    // Ensure wasm32 target present; if missing instruct user and exit
    if Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"))
        .unwrap_or(false)
        == false
    {
        eprintln!("Rust target wasm32-unknown-unknown not installed. Run `rustup target add wasm32-unknown-unknown` and retry. See README.md.");
        std::process::exit(1);
    }

    // Build wasm bundle
    println!("Building WASM pkg …");
    match Command::new(&wasm_pack_exe)
        .args([
            "build",
            "--release",
            "--target",
            "web",
            "--out-dir",
            "static/pkg",
        ])
        .status()
    {
        Ok(st) if st.success() => {},
        Ok(_) => {
            eprintln!("wasm-pack finished with errors. Ensure wasm-pack is installed (https://rustwasm.github.io/wasm-pack/).");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run wasm-pack: {e}. You may need to install it manually.");
            std::process::exit(1);
        }
    }

    // Ensure bundle produced
    if !std::path::Path::new("static/pkg/viz_wasm.js").exists() {
        eprintln!("WASM bundle missing after build – aborting server start.");
        std::process::exit(1);
    }

    // 2. Start simple HTTP server serving `static/` on 8000
    println!("Launching local server at http://0.0.0.0:8000 …");
    // Bind explicitly to 0.0.0.0 so the service is reachable from outside
    // the host machine (e.g. mobile devices on the same network) without
    // requiring ngrok or a similar tunnel.
    let _server = Command::new("python3")
        .args([
            "-m",
            "http.server",
            "8000",
            "--bind",
            "0.0.0.0",
            "--directory",
            "static",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start http server");

    // 3. Try to start ngrok if installed
    let ngrok = Command::new("ngrok")
        .args(["http", "8000"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    match ngrok {
        Ok(_) => println!("ngrok tunnel starting …"),
        Err(_) => eprintln!("ngrok not found. Install it to expose the site over the internet."),
    }

    // Keep process alive
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
