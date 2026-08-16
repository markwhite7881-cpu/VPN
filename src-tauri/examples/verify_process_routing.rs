//! Verify that a `process_name` routing rule round-trips through our
//! config builder AND that sing-box 1.14+ accepts it as a valid matcher.
//!
//! Run:
//!   cargo run --example verify_process_routing
//!
//! What we test:
//!   1. Build a minimal config with one `process_name` rule (match
//!      Telegram.exe and Discord.exe → direct) and one `process_path`
//!      rule (match by full path → proxy).
//!   2. Run `sing-box check` on the generated JSON. sing-box will
//!      reject any unknown matcher fields at this stage, so a passing
//!      check proves the field is recognised by the binary.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::json;
use singbox_client_lib::config::{Config, GeneratorSettings, RoutingOptions, TunnelMode};

fn main() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");

    // ---- 1. Build a config with process_name + process_path rules ---
    println!("=== 1. Building config with process_name / process_path rules ===\n");

    let settings = GeneratorSettings {
        tunnel_mode: TunnelMode::SystemProxy,
        routing: RoutingOptions {
            rules: vec![
                // Bypass the user's messaging apps — they should not
                // go through the proxy.
                json!({
                    "process_name": ["Telegram.exe", "Discord.exe"],
                    "action": "route",
                    "outbound": "direct"
                }),
                // Force a specific browser through the proxy by full path.
                json!({
                    "process_path": ["C:\\Program Files\\Mozilla Firefox\\firefox.exe"],
                    "action": "route",
                    "outbound": "proxy"
                }),
                // An empty rule that should be stripped (sanity check).
                json!({
                    "enabled": false,
                    "process_name": ["should-be-skipped.exe"],
                    "action": "route",
                    "outbound": "direct"
                }),
            ],
            rule_sets: vec![],
            sniff: false,
            final_outbound: "proxy".to_string(),
            auto_detect_interface: true,
            default_domain_resolver: "local".to_string(),
        },
        ..GeneratorSettings::default()
    };

    let value = Config::build(&[], &settings);

    // ---- 2. Dump & inspect the generated `route.rules` ---------------
    let route_rules = value
        .get("route")
        .and_then(|r| r.get("rules"))
        .and_then(|r| r.as_array())
        .expect("route.rules must be an array");

    println!("  generated {} route rules:", route_rules.len());
    for (i, r) in route_rules.iter().enumerate() {
        println!("    [{}] {}", i, r);
    }

    // Hard assertions on the output shape.
    let user_rules: Vec<_> = route_rules
        .iter()
        .filter(|r| r.get("process_name").is_some() || r.get("process_path").is_some())
        .collect();
    assert_eq!(
        user_rules.len(),
        2,
        "expected exactly 2 user-defined process rules (the disabled one must be dropped), got {}",
        user_rules.len()
    );

    // The first user rule must still carry `Telegram.exe` and `Discord.exe`.
    let first = user_rules[0];
    let names = first
        .get("process_name")
        .and_then(|n| n.as_array())
        .expect("first user rule must have process_name array");
    let name_strs: Vec<String> = names
        .iter()
        .map(|n| n.as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        name_strs.contains(&"Telegram.exe".to_string()),
        "Telegram.exe must be preserved in process_name array, got {:?}",
        name_strs
    );
    assert!(
        name_strs.contains(&"Discord.exe".to_string()),
        "Discord.exe must be preserved in process_name array, got {:?}",
        name_strs
    );
    assert_eq!(first.get("action").and_then(|a| a.as_str()), Some("route"));
    assert_eq!(
        first.get("outbound").and_then(|a| a.as_str()),
        Some("direct")
    );
    println!("  ✓ process_name array preserved (Telegram.exe, Discord.exe)");

    // The second user rule must carry the full process_path.
    let second = user_rules[1];
    let path = second
        .get("process_path")
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|p| p.as_str())
        .expect("second user rule must have process_path[0]");
    assert_eq!(path, "C:\\Program Files\\Mozilla Firefox\\firefox.exe");
    println!("  ✓ process_path array preserved");

    // The disabled rule must not appear anywhere.
    let dropped = route_rules
        .iter()
        .any(|r| r.to_string().contains("should-be-skipped.exe"));
    assert!(
        !dropped,
        "disabled rules must be dropped from the generated config"
    );
    println!("  ✓ disabled rule correctly dropped\n");

    // ---- 3. Write the config to disk ---------------------------------
    let out_path = here
        .join("examples")
        .join("verify_process_routing_output.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    let body = serde_json::to_string_pretty(&value).expect("serialise");
    fs::write(&out_path, &body).expect("write output");
    println!("  wrote {} ({} bytes)\n", out_path.display(), body.len());

    // ---- 4. sing-box check -------------------------------------------
    if !singbox.exists() {
        eprintln!("!! sing-box sidecar not found at {}", singbox.display());
        eprintln!("   (skipping sing-box check; assertions above still passed)");
        return;
    }
    println!("=== 4. Running `sing-box check` ===\n");
    let output = Command::new(&singbox)
        .arg("check")
        .arg("-c")
        .arg(&out_path)
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            print!("{stdout}{stderr}");
            if o.status.success() {
                println!("\n✅ sing-box check passed — process_name / process_path are valid matchers in this sing-box build");
            } else {
                panic!(
                    "❌ sing-box check failed (exit {:?}) — process_name / process_path are NOT supported by this sing-box build",
                    o.status.code()
                );
            }
        }
        Err(e) => panic!("!! could not run sing-box: {e}"),
    }
}
