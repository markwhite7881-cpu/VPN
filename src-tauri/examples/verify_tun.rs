//! Quick sanity check: generate a TUN config from the current
//! settings and run `sing-box check` on it.
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
                bypass_lan: true,
                reject_ipv6: true,
                block_quic: false,
                block_ads: false,
                bypass_cn: false,
                bypass_ru: false,
                final_outbound: "proxy".to_string(),
            },
            clash_api: singbox_client_lib::config::ClashApiOptions::default(),
            tun_interface_name: None,
            mixed_port: Some(2080),
            local_dns: Some("223.5.5.5".to_string()),
            remote_dns: Some("https://dns.google/dns-query".to_string()),
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
