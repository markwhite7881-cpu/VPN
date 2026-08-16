//! Verify the action-normalization + matcher-flattening fixes
//! end-to-end.
//!
//! Three bugs were reported, all caused by UI-side data shapes that
//! don't match sing-box's rule schema:
//!
//! 1. `action` is an OBJECT in the UI ({kind, outbound}), but
//!    sing-box 1.14+ wants a STRING with `outbound` lifted to the
//!    top level.
//! 2. UI-only fields `id`, `label`, `enabled` were leaking through
//!    to sing-box → "unknown field" errors.
//! 3. UI nests matchers inside a `matchers: {...}` object, but
//!    sing-box wants them flat on the rule itself.
//!
//! This example builds a config with the UI's full real-world shape
//! and runs `sing-box check` to confirm the fixes produce a
//! sing-box-valid file.

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
    //   - `id` (React key) — stripped
    //   - `label` (display name) — stripped
    //   - `enabled` (on/off toggle) — stripped (disabled rules are
    //      dropped entirely, not marked in-place)
    //   - `matchers` wrapping object — flattened into top-level
    //      sibling fields
    //   - `action` as an OBJECT — normalized to sing-box's string
    //      form, with `outbound` lifted to a top-level field
    let settings = GeneratorSettings {
        tunnel_mode: TunnelMode::SystemProxy,
        routing: RoutingOptions {
            rules: vec![
                json!({
                    "id": "rule-001",
                    "label": "Bypass LAN",
                    "enabled": true,
                    "matchers": {
                        "ip_cidr": ["10.0.0.0/8", "192.168.0.0/16"],
                    },
                    "action": {"kind": "route", "outbound": "direct"},
                }),
                json!({
                    "id": "rule-002",
                    "label": "Route example.com via proxy",
                    "enabled": true,
                    "matchers": {
                        "domain_suffix": ["example.com"],
                    },
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "id": "rule-003",
                    "label": "Telegram through proxy",
                    "enabled": true,
                    "matchers": {
                        "process_name": ["Telegram.exe"],
                    },
                    "action": {"kind": "route", "outbound": "proxy"},
                }),
                json!({
                    "id": "rule-004",
                    "label": "Block IPv6",
                    "enabled": true,
                    "matchers": {
                        "ip_version": 6,
                    },
                    "action": {"kind": "reject"},
                }),
                json!({
                    "id": "rule-005",
                    "label": "Hijack DNS",
                    "enabled": true,
                    "matchers": {
                        "network": "udp",
                    },
                    "action": {"kind": "hijack-dns"},
                }),
                // A disabled rule — should be DROPPED entirely (not
                // passed through to sing-box with `enabled: false`).
                json!({
                    "id": "rule-006",
                    "label": "disabled rule",
                    "enabled": false,
                    "matchers": {
                        "ip_cidr": ["8.8.8.8/32"],
                    },
                    "action": {"kind": "route", "outbound": "direct"},
                }),
                // A rule with an empty matchers object — should also
                // be dropped (no meaningful matchers → sing-box would
                // reject it).
                json!({
                    "id": "rule-007",
                    "label": "empty matchers",
                    "enabled": true,
                    "matchers": {},
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

    // --- 2. Assert: all user rules are clean sing-box shape --------
    let rules = cfg["route"]["rules"].as_array().expect("route.rules");
    println!("Generated {} route rules:", rules.len());
    for (i, r) in rules.iter().enumerate() {
        println!("  [{}] {}", i, r);
    }
    // System rules come first: dns-bypass + sniff. Then 5 user
    // rules (rule-006 disabled, rule-007 empty matchers, both
    // dropped). Total: 2 + 5 = 7.
    assert_eq!(
        rules.len(),
        7,
        "expected 2 system + 5 user (disabled + empty-matcher rules must be dropped)"
    );
    let user_rules = &rules[2..];
    for (i, r) in user_rules.iter().enumerate() {
        // action must be a string
        let action = &r["action"];
        assert!(
            action.is_string(),
            "user rule {}: action must be a string for sing-box, got: {}",
            i,
            action
        );
        // No `matchers` wrapper allowed
        assert!(
            r.get("matchers").is_none(),
            "user rule {}: matchers wrapper must be flattened",
            i
        );
        // UI-only fields must NOT leak through
        assert!(
            r.get("id").is_none(),
            "user rule {}: id must be stripped",
            i
        );
        assert!(
            r.get("label").is_none(),
            "user rule {}: label must be stripped",
            i
        );
        assert!(
            r.get("enabled").is_none(),
            "user rule {}: enabled must be stripped",
            i
        );
    }
    // Disabled and empty-matcher rules must not appear anywhere.
    for r in rules {
        let s = r.to_string();
        assert!(
            !s.contains("8.8.8.8"),
            "disabled rule (8.8.8.8) must be dropped, found: {s}"
        );
        assert!(
            !s.contains("rule-006"),
            "disabled rule id must be dropped, found in: {s}"
        );
        assert!(
            !s.contains("rule-007"),
            "empty-matcher rule id must be dropped, found in: {s}"
        );
    }
    println!("\n  ✓ all user rules: action is STRING, matchers flattened, no UI fields leak");
    println!("  ✓ disabled + empty-matcher rules dropped");

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
                    "\n✅ sing-box check passed — action normalization + matcher flattening are correct end-to-end"
                );
            } else {
                panic!(
                    "❌ sing-box check failed (exit {:?}) — the fix is incomplete",
                    o.status.code()
                );
            }
        }
        Err(e) => panic!("!! could not run sing-box: {e}"),
    }
}
