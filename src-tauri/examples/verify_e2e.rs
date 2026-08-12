//! End-to-end repro: generate a system_proxy config from a real
//! share-link, run `sing-box check`, then `sing-box run` for a
//! few seconds, capture stdout, and report what we saw. Prints
//! the inbound + outbound blocks so we can eyeball the diff vs
//! what Tauri generates.
use singbox_client_lib::config::{Config, GeneratorSettings, RoutingOptions, TunnelMode};
use singbox_client_lib::parser::parse_link;
use std::process::{Command, Stdio};
use std::time::Duration;

fn main() {
    let link = "vless://be0589e6-eac2-48cd-94f4-e41ceb8aa3c8@138.124.33.206:443\
?authority=&encryption=none&fp=firefox\
&pbk=GH4VZBxIED3RD2GmW68vdPFj9OaTNnoRRa8X7iWXt3M\
&security=reality&serviceName=&sid=b3b2138fa73c0827\
&sni=www.nvidia.com&spx=%2FNzDAT3TGEOWKLm0&type=grpc\
#rfgrfgr32412-qt9htqyal6ys";
    let outbound = parse_link(link).expect("parse");
    let cfg = Config::build(
        &[outbound],
        &GeneratorSettings {
            tunnel_mode: TunnelMode::SystemProxy,
            routing: RoutingOptions::default(),
            clash_api: singbox_client_lib::config::ClashApiOptions::default(),
            tun_interface_name: None,
            mixed_port: Some(2080),
            local_dns: Some("223.5.5.5".to_string()),
            remote_dns: Some("https://dns.google/dns-query".to_string()),
        },
    );
    let out_path = std::env::temp_dir().join("verify_e2e.json");
    std::fs::write(&out_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    eprintln!("wrote {}", out_path.display());

    // 1. sing-box check
    eprintln!("\n--- sing-box check ---");
    let sb = std::env::current_dir()
        .unwrap()
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");
    let _ = Command::new(&sb)
        .arg("check")
        .arg("-c")
        .arg(&out_path)
        .status();

    // 2. sing-box run for 3 seconds, then kill
    eprintln!("\n--- sing-box run (3s) ---");
    let mut child = Command::new(&sb)
        .arg("run")
        .arg("-c")
        .arg(&out_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sing-box");
    std::thread::sleep(Duration::from_secs(3));
    let _ = child.kill();
    let out = child.wait_with_output().expect("wait");
    eprintln!("exit status: {:?}", out.status);
    eprintln!("--- stdout ---");
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    eprintln!("--- stderr ---");
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
}
