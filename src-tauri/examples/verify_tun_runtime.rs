//! Probe sing-box TUN-mode startup to diagnose the actual failure mode.
//!
//! What we do:
//!   1. Write a minimal TUN-only sing-box config to disk.
//!   2. Spawn `sing-box run -c <config>` for 3 seconds.
//!   3. Capture stdout/stderr and the exit code, then print a verdict.
//!
//! The point of this example: when the Tauri app fails to start in
//! TUN mode, the user wants to know *why*. The two common root causes
//! are:
//!   - "Access is denied" / "A required privilege is not held by the
//!     client" — Windows requires the process to be elevated
//!     (administrator) to configure a TUN interface. The fix is to
//!     add a `requireAdministrator` manifest to the Tauri app.
//!   - Driver / Wintun missing — the sing-box binary would log
//!     something about Wintun or a missing .dll.
//!
//! Run:
//!   cargo run --example verify_tun_runtime
//!
//! Note: this example does NOT need admin — the failure itself is
//! the signal. If the binary is run as admin, the sing-box process
//! will start successfully and the example will time out at 3s with
//! the sing-box process still running (we then kill it).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn main() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");

    if !singbox.exists() {
        eprintln!("!! sing-box sidecar not found at {}", singbox.display());
        return;
    }

    // ---- 1. Write a minimal TUN-only config --------------------------
    // We hand-roll the config (instead of going through
    // Config::build) to keep this example focused on the TUN
    // bring-up path. Using Config::build with an empty outbound
    // list would emit a config that fails for unrelated reasons.
    let config_json = r#"{
  "log": { "level": "info" },
  "inbounds": [
    {
      "type": "tun",
      "tag": "tun-in",
      "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
      "auto_route": true,
      "strict_route": true,
      "stack": "system",
      "mtu": 9000,
      "interface_name": "singbox-test"
    }
  ],
  "outbounds": [
    { "type": "direct", "tag": "direct" }
  ],
  "route": {
    "rules": [],
    "final": "direct"
  }
}"#;
    let out_path = here.join("examples").join("verify_tun_runtime_output.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    fs::write(&out_path, config_json).expect("write output");
    println!("  wrote {} ({} bytes)", out_path.display(), config_json.len());

    // ---- 2. Try to start sing-box ------------------------------------
    println!("\n=== Spawning sing-box in TUN mode (3s timeout) ===\n");
    let mut child = Command::new(&singbox)
        .arg("run")
        .arg("-c")
        .arg(&out_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sing-box");

    // Give it 3s to either crash with a clear error or settle.
    std::thread::sleep(Duration::from_secs(3));

    // Try a non-blocking kill so we can capture whatever it printed
    // before death. On Windows, kill() on a child of a non-elevated
    // process works fine even if the child is asking for elevation.
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait_with_output");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("--- sing-box stdout ---\n{stdout}");
    println!("--- sing-box stderr ---\n{stderr}");
    println!("--- exit code: {:?} ---\n", output.status.code());

    // ---- 3. Verdict -------------------------------------------------
    let combined = format!("{stdout}{stderr}");
    let verdict = if combined.contains("Access is denied")
        || combined.contains("A required privilege")
        || combined.contains("elevated")
        || combined.contains("administrator")
    {
        "ELEVATION_NEEDED"
    } else if combined.contains("Wintun")
        || combined.contains("wintun.dll")
        || combined.contains("driver")
    {
        "DRIVER_MISSING"
    } else if output.status.success() || output.status.code() == Some(0) {
        "OK"
    } else {
        "OTHER_ERROR"
    };
    println!("VERDICT: {verdict}");

    match verdict {
        "ELEVATION_NEEDED" => {
            println!(
                "\nThe sing-box binary is fine — TUN on Windows requires the
process to be elevated (administrator). The fix is to add a
requireAdministrator manifest to the Tauri app so the OS shows a UAC
prompt on launch."
            );
        }
        "DRIVER_MISSING" => {
            println!(
                "\nsing-box is failing to load the Wintun driver. The fix
depends on the build: with a statically-linked Wintun (most sing-box
distributions) the issue is almost always elevation. Otherwise, ship
wintun.dll alongside the binary."
            );
        }
        "OK" => {
            println!(
                "\nTUN mode started successfully. process_name routing is
now functional end-to-end."
            );
        }
        _ => {
            println!(
                "\nUnexpected failure mode. Read the stderr above and decide
manually — it might be a config issue, a missing rule-set, or
something else."
            );
        }
    }
}
