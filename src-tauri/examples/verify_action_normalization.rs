//! Verify the action-normalization fix end-to-end.
//!
//! Background: a user reported that sing-box refused to start with
//!   route.rules[2].action: json: cannot unmarshal object into Go
//!   struct field _RuleAction.action of type string
//!
//! Root cause: the UI stores rules with `action: {kind: "route",
//! outbound: "X"}` (object, ergonomic for editing) but sing-box 1.14+
//! wants `action: "route"` (string) with `outbound` lifted to the
//! top level.
//!
//! This example builds a config with the UI's object form and runs
//! `sing-box check` to confirm the fix produces a sing-box-valid
//! file. Before the fix this would fail with the exact error above.

use std::env;
use std::fs;
use std::path::PathBuf;

use serde_json::json;
use singbox_client_lib::config::{Config, GeneratorSettings, RoutingOptions, TunnelMode};

fn main() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");

    if !singbox.exists() {
        eprintln!("!! sing-box sidecar not found at {}", singbox.display());
        return;
    }

    // --- 1. Build a config with the UI's OBJECT form of action -----
    let settings = GeneratorSettings {
        tunnel_mode: TunnelMode::SystemProxy,
        routing: RoutingOptions {
            rules: vec![
                // Each rule uses the UI's object form: action is
                // {kind, outbound} instead of sing-box's {action,
                // outbound} siblings. Before the fix this would
                // fail sing-box check.
                json!({
                    "ip_cidr": ["10.0.0.0/8", "192.168.0.0/16"],
                    "action": {"kind": "route", "outbound": "direct"},
                }),
                json!({
                    "domain_suffix": ["example.com"],
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "process_name": ["Telegram.exe"],
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "ip_version": 6,
                    "action": {"kind": "reject"},
                }),
                json!({
                    "network": "udp",
                    "action": {"kind": "hijack-dns"},
                }),
            ],
            rule_sets: vec![],
            sniff: true,
            final_outbound: "proxy".to_string(),
            auto_detect_interface: true,
            default_domain_resolver: "local".to_string(),
        },
        ..GeneratorSettings::default()
    };
    let cfg = Config::build(&[], &settings);

    // --- 2. Assert: all user rules have action as a STRING ----------
    let rules = cfg["route"]["rules"].as_array().expect("route.rules");
    println!("Generated {} route rules:", rules.len());
    for (i, r) in rules.iter().enumerate() {
        println!("  [{}] {}", i, r);
    }
    let user_rules = &rules[2..]; // first two are system: dns-bypass + sniff
    for (i, r) in user_rules.iter().enumerate() {
        let action = &r["action"];
        assert!(
            action.is_string(),
            "user rule {}: action must be a string for sing-box, got: {}",
            i,
            action
        );
    }
    println!("\n  ✓ all user rules have action as a STRING");

    // --- 3. Run `sing-box check` ---------------------------------
    let out_path = here
        .join("examples")
        .join("verify_action_normalization_output.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    let body = serde_json::to_string_pretty(&cfg).expect("serialise");
    fs::write(&out_path, &body).expect("write output");
    println!("  wrote {} ({} bytes)", out_path.display(), body.len());

    println!("\n=== Running `sing-box check` ===\n");
    let output = std::process::Command::new(&singbox)
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
                println!(
                    "\n✅ sing-box check passed — action normalization is correct end-to-end"
                );
            } else {
                panic!(
                    "❌ sing-box check failed (exit {:?}) — the action-normalization fix is incomplete",
                    o.status.code()
                );
            }
        }
        Err(e) => panic!("!! could not run sing-box: {e}"),
    }
}
