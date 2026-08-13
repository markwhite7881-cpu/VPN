//! Cloakwire library entry point.
//!
//! `main.rs` is a thin wrapper that calls `run()` from this crate. The
//! split makes it possible to add a `cdylib` target later for mobile
//! builds without touching the binary entry point.

// Force-rebuild marker: tauri::generate_context!() embeds dist/ at lib
// compile time. Touching this file makes cargo recompile the lib, which
// re-runs the macro and re-embeds the current dist/ (after every
// `npm run build`). 2026-08-13.

use std::sync::Arc;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

pub mod clash_api;
pub mod commands;
pub mod config;
pub mod error;
pub mod parser;
pub mod process;
pub mod traffic;
pub mod updates;

use process::ProcessManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Best-effort logger init. RUST_LOG=info turns it on by default.
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,cloakwire_lib=debug"),
    )
    .try_init();

    let manager = Arc::new(ProcessManager::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            // Forward the `--minimized` flag (no-op on Windows/Linux)
            // so the user can opt into starting in the background.
            Some(vec!["--minimized"]),
        ))
        .manage(manager)
        .setup(|app| {
            // We're now inside Tauri's tokio runtime, safe to spawn.
            let mgr = app.state::<Arc<ProcessManager>>();
            let mgr = Arc::clone(mgr.inner());
            mgr.spawn_watcher();
            // Make sure the main window is visible and focused.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            // Stale-proxy guard: if a previous run of this app crashed while
            // the system proxy was active, Windows would keep routing
            // traffic to 127.0.0.1:<port> even though sing-box is dead.
            // Clear it on startup so the user has working internet
            // immediately, even before they hit "Connect".
            #[cfg(windows)]
            {
                use crate::process::clear_system_proxy;
                if let Err(e) = clear_system_proxy() {
                    log::warn!("startup: failed to clear stale system proxy: {e}");
                } else {
                    log::info!("startup: cleared any stale system proxy from a previous run");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_binary_info,
            commands::get_singbox_version,
            commands::check_config,
            commands::start_singbox,
            commands::stop_singbox,
            commands::get_status,
            commands::get_logs,
            commands::is_running,
            commands::get_current_config,
            commands::write_default_config,
            commands::reset_state,
            commands::parse_link,
            commands::parse_links,
            commands::parse_input,
            commands::outbound_to_singbox_json,
            commands::generate_config,
            commands::save_config_to_path,
            commands::check_config_with_binary,
            commands::start_singbox_with_config,
            commands::list_proxies,
            commands::select_proxy,
            commands::test_delay,
            commands::ping_endpoint,
            commands::lookup_geoip,
            commands::start_traffic,
            commands::stop_traffic,
            commands::fetch_subscription,
            commands::get_autostart,
            commands::set_autostart,
            commands::apply_system_proxy,
            commands::clear_system_proxy,
            commands::list_processes,
            commands::check_singbox_update,
            commands::apply_singbox_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running singbox-client");
}
