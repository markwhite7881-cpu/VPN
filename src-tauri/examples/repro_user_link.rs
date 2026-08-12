//! Quick repro for the user's exact vless link.
use singbox_client_lib::parser::parse_link;

fn main() {
    let raw = "vless://be0589e6-eac2-48cd-94f4-e41ceb8aa3c8@138.124.33.206:443\
?authority=&encryption=none&fp=firefox\
&pbk=GH4VZBxIED3RD2GmW68vdPFj9OaTNnoRRa8X7iWXt3M\
&security=reality&serviceName=&sid=b3b2138fa73c0827\
&sni=www.nvidia.com&spx=%2FNzDAT3TGEOWKLm0&type=grpc\
#rfgrfgr32412-qt9htqyal6ys";

    match parse_link(raw) {
        Ok(o) => {
            println!("OK: protocol={} tag={}", o.protocol(), o.display_name());
            println!("server:port = {:?}", o.server().zip(Some(o.port().unwrap_or(0))));
            println!("full = {}", serde_json::to_string_pretty(&o).unwrap());
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }
}
