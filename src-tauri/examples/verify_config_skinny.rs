//! Generate a *minimal* sing-box config and run `sing-box check` against
//! the real sidecar. Skips profiles that need X25519 public keys (VLESS
//! Reality) so the placeholder credentials don't break validation.
//!
//! Run from src-tauri/:
//!   cargo run --example verify_config_skinny

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use singbox_client_lib::config::{Config, GeneratorSettings};
use singbox_client_lib::parser::parse_link;

fn main() {
    // -- Links that don't require a real X25519 / Reality public key ----
    // SS:  userinfo is method:password (no curve key).
    // VMess: uuid (no curve key).
    // TUIC: uuid + password.
    let raw = [
        // Shadowsocks SIP002
        "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpzZWNyZXQ@127.0.0.1:8388#SS-local",
        // VMess over WebSocket
        "vmess://eyJ2IjoiMiIsInBzIjoiVk0tdG9rZW4iLCJhZGQiOiIxMjcuMC4wLjEiLCJwb3J0IjoiODA4MCIsImlkIjoiMTExMTExMTEtMjIyMi0zMzMzLTQ0NDQtNTU1NTU1NTU1NTU1IiwiYWlkIjoiMCIsInNjeSI6ImF1dG8iLCJuZXQiOiJ3cyIsInBhdGgiOiIvIiwidGxzIjoidGxzIiwic25pIjoiMTI3LjAuMC4xIn0=",
        // TUIC v5
        "tuic://11111111-2222-3333-4444-555555555555:pw@127.0.0.1:443?congestion_control=bbr&sni=example.com#TUIC-local",
    ];

    let mut outbounds = Vec::new();
    for (i, link) in raw.iter().enumerate() {
        match parse_link(link) {
            Ok(o) => {
                println!(
                    "  [{}] parsed: tag={} protocol={}",
                    i,
                    o.display_name(),
                    o.protocol()
                );
                outbounds.push(o);
            }
            Err(e) => eprintln!("  [{}] parse error: {}", i, e),
        }
    }
    println!("\nparsed {} outbounds", outbounds.len());

    // Use only "mixed" inbound so we don't need admin / Wintun.
    let settings = GeneratorSettings {
        tunnel_mode: singbox_client_lib::config::TunnelMode::SystemProxy,
        ..GeneratorSettings::default()
    };
    let value = Config::build(&outbounds, &settings);

    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = here.join("examples").join("verify_output_skinny.json");
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    let body = serde_json::to_string_pretty(&value).expect("serialise");
    fs::write(&out, &body).expect("write output");
    println!("\nwrote {} ({} bytes)", out.display(), body.len());

    // `sing-box check` — only valid if all credentials are real.
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");
    if !singbox.exists() {
        eprintln!(
            "\n! sing-box sidecar not found at {}. Skipping check.",
            singbox.display()
        );
        return;
    }
    println!(
        "\nrunning: {} check -c {}",
        singbox.display(),
        out.display()
    );
    let output = Command::new(&singbox)
        .arg("check")
        .arg("-c")
        .arg(&out)
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            print!("stdout: {}", stdout);
            print!("stderr: {}", stderr);
            println!("exit: {}", o.status);
            if o.status.success() {
                println!("\n✅ sing-box check passed");
            } else {
                eprintln!("\n❌ sing-box check failed — credentials still need real values");
                std::process::exit(2);
            }
        }
        Err(e) => eprintln!("\n! could not run sing-box: {}", e),
    }
}
