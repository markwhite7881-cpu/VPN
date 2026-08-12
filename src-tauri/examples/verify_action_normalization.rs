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

    // --- 1. Build a config with the UI's full real-world shape -----
    // Each rule carries:
    //   - `id` (React key) — must be stripped before sing-box
    //   - `label` (display name) — must be stripped before sing-box
    //   - `enabled` (on/off toggle) — must be stripped before sing-box
    //   - `action` as an OBJECT (UI form) — must be normalized to
    //     sing-box's string form
    // Both bugs (action object + UI-only fields) are exercised here.
    let settings = GeneratorSettings {
        tunnel_mode: TunnelMode::SystemProxy,
        routing: RoutingOptions {
            rules: vec![
                json!({
                    "id": "rule-001",
                    "label": "Bypass LAN",
                    "enabled": true,
                    "ip_cidr": ["10.0.0.0/8", "192.168.0.0/16"],
                    "action": {"kind": "route", "outbound": "direct"},
                }),
                json!({
                    "id": "rule-002",
                    "label": "Route example.com via proxy",
                    "enabled": true,
                    "domain_suffix": ["example.com"],
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "id": "rule-003",
                    "label": "Telegram through proxy",
                    "enabled": true,
                    "process_name": ["Telegram.exe"],
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "id": "rule-004",
                    "label": "Block IPv6",
                    "enabled": true,
                    "ip_version": 6,
                    "action": {"kind": "reject"},
                }),
                json!({
                    "id": "rule-005",
                    "label": "Hijack DNS",
                    "enabled": true,
                    "network": "udp",
                    "action": {"kind": "hijack-dns"},
                }),
                // A disabled rule — should be DROPPED entirely (not
                // passed through to sing-box with `enabled: false`).
                json!({
                    "id": "rule-006",
                    "label": "disabled rule",
                    "enabled": false,
                    "ip_cidr": ["8.8.8.8/32"],
                    "action": {"kind": "route", "outbound": "direct"},
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

    // --- 2. Assert: all user rules have action as a STRING, and no
    //         UI-only fields leak through, and the disabled rule
    //         is dropped entirely.
    let rules = cfg["route"]["rules"].as_array().expect("route.rules");
    println!("Generated {} route rules:", rules.len());
    for (i, r) in rules.iter().enumerate() {
        println!("  [{}] {}", i, r);
    }
    // System rules come first: dns-bypass + sniff. The user rules
    // follow, except the disabled one which is dropped.
    // So: 2 system + 5 user (not 6, because rule-006 is disabled) = 7.
    assert_eq!(
        rules.len(),
        7,
        "expected 2 system + 5 user (the disabled rule must be dropped)"
    );
    let user_rules = &rules[2..];
    for (i, r) in user_rules.iter().enumerate() {
        let action = &r["action"];
        assert!(
            action.is_string(),
            "user rule {}: action must be a string for sing-box, got: {}",
            i,
            action
        );
        // UI-only fields must NOT leak through
        assert!(
            r.get("id").is_none(),
            "user rule {}: id must be stripped (UI-only), got: {}",
            i,
            r.get("id").unwrap()
        );
        assert!(
            r.get("label").is_none(),
            "user rule {}: label must be stripped (UI-only)",
            i
        );
        assert!(
            r.get("enabled").is_none(),
            "user rule {}: enabled must be stripped (UI-only)",
            i
        );
    }
    // The disabled rule (id rule-006) must not appear anywhere in the
    // emitted config.
    for r in rules {
        let s = r.to_string();
        assert!(
            !s.contains("8.8.8.8"),
            "disabled rule with 8.8.8.8/32 must be dropped, found: {s}"
        );
        assert!(
            !s.contains("rule-006"),
            "disabled rule id must be dropped, found in: {s}"
        );
    }
    println!("\n  ✓ all user rules: action is STRING, no UI-only fields leak");
    println!("  ✓ disabled rule dropped entirely");

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
