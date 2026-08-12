//! Quick sanity check: generate a TUN config from the current
//! settings and run `sing-box check` on it.
use serde_json::json;
use singbox_client_lib::config::{Config, GeneratorSettings, RoutingOptions, TunnelMode};
use singbox_client_lib::parser::parse_link;

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
            tunnel_mode: TunnelMode::Tun,
            routing: RoutingOptions {
                // Routing 2.0: mirror a typical "safe defaults" rule list.
                rules: vec![
                    json!({
                        "ip_cidr": [
                            "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16",
                            "127.0.0.0/8", "169.254.0.0/16"
                        ],
                        "action": "route",
                        "outbound": "direct"
                    }),
                    json!({ "ip_version": 6, "action": "reject" }),
                ],
                rule_sets: vec![],
                sniff: true,
                final_outbound: "proxy".to_string(),
                auto_detect_interface: true,
                default_domain_resolver: "local".to_string(),
            },
            clash_api: singbox_client_lib::config::ClashApiOptions::default(),
            tun_interface_name: None,
            mixed_port: Some(2080),
            local_dns: Some("1.1.1.1".to_string()),
            remote_dns: Some("https://8.8.8.8/dns-query".to_string()),
            default_outbound: None,
        },
    );
    let out_path = std::env::temp_dir().join("verify_tun.json");
    std::fs::write(&out_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    eprintln!("wrote {}", out_path.display());

    // Print the inbound block so we can eyeball it.
    if let Some(inbounds) = cfg.get("inbounds").and_then(|v| v.as_array()) {
        eprintln!("inbounds:");
        for ib in inbounds {
            eprintln!("  {}", serde_json::to_string(ib).unwrap());
        }
    }
}
