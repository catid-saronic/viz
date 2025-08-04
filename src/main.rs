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

    // Build wasm bundle
    println!("Building WASM pkg …");
    match Command::new("wasm-pack")
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
        Err(_) => {
            eprintln!("wasm-pack not found in PATH. Skipping wasm build; the site may serve stale artifacts.");
        }
    }

    // 2. Start simple HTTP server serving `static/` on 8000
    println!("Launching local server at http://127.0.0.1:8000 …");
    let _server = Command::new("python3")
        .args(["-m", "http.server", "8000", "--directory", "static"])
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
