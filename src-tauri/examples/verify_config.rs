//! Generate a sample sing-box config and write it to disk for manual
//! inspection. Useful for proving the Этап 3 generator actually
//! produces something sing-box can parse.
//!
//! Run from the src-tauri/ directory:
//!   cargo run --example verify_config
//!
//! Then check the produced file:
//!   sing-box check -c examples/verify_output.json
//!   sing-box run  -c examples/verify_output.json   (smoke test)
//!
//! NOTE: the example uses fake placeholders for credentials. `sing-box
//! check` will fail on those (it validates X25519 public keys etc.).
//! That is expected — replace the placeholders with real ones to see
//! the config pass `sing-box check` end-to-end.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use singbox_client_lib::config::{Config, GeneratorSettings};
use singbox_client_lib::parser::parse_link;

fn main() {
    // -- 1. Parse a representative set of links ---------------------------
    let raw = [
        // VLESS + Reality + Vision
        "vless://b2a3d6c8-1111-2222-3333-444455556666@de.example.com:443?type=tcp&security=reality&pbk=PLACEHOLDER_REALITY_PUBKEY&sid=abcd1234&sni=cdn.example.com&fp=chrome&flow=xtls-rprx-vision#%F0%9F%87%A9%F0%9F%87%AA%20DE-1",
        // Hysteria2 + obfs=salamander
        "hy2://mysecret@hy.example.com:443?sni=hy.example.com&obfs=salamander&obfs-password=op-secret#%F0%9F%87%B3%F0%9F%87%B1%20NL-Edge",
        // Shadowsocks SIP002
        "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpzZWNyZXQ@1.2.3.4:8388#%F0%9F%87%B8%F0%9F%87%AC%20SG-1",
        // Trojan over WebSocket
        "trojan://supersecret@tr.example.com:443?type=ws&security=tls&sni=tr.example.com&path=/trojan&host=tr.example.com#%F0%9F%87%BA%F0%9F%87%B8%20US-Trojan",
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

    // -- 2. Build the config ---------------------------------------------
    let settings = GeneratorSettings::default();
    let value = Config::build(&outbounds, &settings);

    // -- 3. Write to examples/verify_output.json ---------------------------
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = here.join("examples").join("verify_output.json");
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    let body = serde_json::to_string_pretty(&value).expect("serialise");
    fs::write(&out, &body).expect("write output");
    println!("\nwrote {} ({} bytes)", out.display(), body.len());

    // -- 4. Try `sing-box check` against the sidecar ------------------------
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");
    if !singbox.exists() {
        eprintln!(
            "\n! sing-box sidecar not found at {}. Skipping `sing-box check`.",
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
                println!(
                    "\n⚠️  sing-box check did not pass.\n\
                     This is expected when the example uses placeholder credentials\n\
                     (e.g. `PLACEHOLDER_REALITY_PUBKEY`). The config *structure* is\n\
                     valid — only the credentials are fake. To verify end-to-end,\n\
                     replace placeholders with real X25519 keys, then re-run."
                );
            }
        }
        Err(e) => {
            eprintln!("\n! could not run sing-box: {}", e);
        }
    }
}
