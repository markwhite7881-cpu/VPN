fn main() {
    // Custom build attributes for Tauri.
    //
    // The big one: `app_manifest` overrides Tauri's default Windows
    // manifest (which only declares the Common Controls v6
    // dependency) with our own that ALSO requests
    // `requireAdministrator`. sing-box TUN mode on Windows needs
    // elevation to create the TUN interface and modify the routing
    // table, and the elevation has to be on the Tauri process
    // itself — Windows won't let a non-elevated parent spawn an
    // elevated sing-box child without a manifest like this one.
    //
    // We read the manifest from `app.manifest` at build time so the
    // XML stays in its own file (easier to read / validate) instead
    // of being embedded as a raw string in build.rs.
    // Register the app-local Android VPN plugin ("vpn") so its permission
    // manifest (src-tauri/permissions/vpn/**) is embedded into the ACL
    // runtime authority — without this the webview gets
    // "vpn.<cmd> not allowed. Plugin not found".
    let mut attrs = tauri_build::Attributes::new().plugin(
        "vpn",
        tauri_build::InlinedPlugin::new(),
    );
    println!("cargo:rerun-if-changed=permissions");
    #[cfg(windows)]
    {
        let manifest_path = std::path::Path::new("app.manifest");
        if !manifest_path.exists() {
            panic!(
                "app.manifest not found at {} — required for UAC elevation on Windows",
                manifest_path.display()
            );
        }
        let manifest = std::fs::read_to_string(manifest_path)
            .expect("read app.manifest");
        // `windows().app_manifest(...)` REPLACES the default
        // Tauri-provided manifest entirely, so we must include the
        // Common-Controls dependency ourselves. See:
        //   tauri-build/src/lib.rs ("if you are using tauri's
        //   dialog APIs, you need to specify a dependency on
        //   Common Control v6 by adding the following to your
        //   custom manifest")
        let combined = if manifest.contains("Microsoft.Windows.Common-Controls") {
            manifest
        } else {
            inject_common_controls(manifest)
        };
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(combined),
        );
        println!("cargo:rerun-if-changed=app.manifest");
    }
    tauri_build::try_build(attrs).expect("tauri build");
}

/// Prepend the Microsoft.Windows.Common-Controls v6 dependency to
/// a user manifest. Tauri apps that use any of the standard dialog
/// APIs (file open, save, message box) need this so the dialogs
/// render with the modern control styles. Tauri's default manifest
/// includes it; when we override the manifest we have to keep it
/// ourselves or dialogs revert to the Windows 95 look.
#[cfg(windows)]
fn inject_common_controls(manifest: String) -> String {
    let dep = r#"  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
"#;
    // Insert the dependency just before </assembly>.
    match manifest.find("</assembly>") {
        Some(idx) => {
            let mut out = String::with_capacity(manifest.len() + dep.len());
            out.push_str(&manifest[..idx]);
            out.push_str(dep);
            out.push_str(&manifest[idx..]);
            out
        }
        None => panic!("app.manifest missing </assembly> closing tag"),
    }
}
