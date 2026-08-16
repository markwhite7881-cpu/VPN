//! sing-box process lifecycle management.
//!
//! Responsibilities:
//! - Locate the bundled sidecar binary (dev + release).
//! - Spawn it with a config file, capture stdout/stderr to a ring buffer.
//! - Health-check loop, automatic restart on unexpected exit (configurable).
//! - Graceful shutdown (SIGTERM on Unix, taskkill on Windows).
//!
//! Why not tauri_plugin_shell's sidecar API?
//!   We need full control over stdin/stdout (live log streaming) and a
//!   child handle we can hold across commands. tokio::process gives us
//!   that with no extra ceremony.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::traffic::TrafficStream;

const LOG_BUFFER_CAPACITY: usize = 2000;
const DEFAULT_TERMINATE_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(3);

/// One log line from sing-box's stdout/stderr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub stream: LogStream,
    pub line: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Crashed,
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub status: Status,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_exit_code: Option<i32>,
    pub last_error: Option<String>,
}

impl Default for StatusReport {
    fn default() -> Self {
        Self {
            status: Status::Stopped,
            pid: None,
            uptime_secs: None,
            last_exit_code: None,
            last_error: None,
        }
    }
}

/// Centralised state shared across Tauri commands.
pub struct ProcessManager {
    /// Currently running child process, if any.
    child: Mutex<Option<Child>>,
    /// Ring buffer of recent log lines.
    logs: Mutex<VecDeque<LogLine>>,
    /// Current snapshot used by the frontend.
    status: Mutex<StatusReport>,
    /// When the current process started.
    started_at: Mutex<Option<std::time::Instant>>,
    /// Config path the running process was started with.
    current_config: Mutex<Option<PathBuf>>,
    /// URL of the Clash API (if any). Set when start() is called.
    controller_url: Mutex<Option<String>>,
    /// Live traffic WebSocket reader. Started automatically when
    /// `controller_url` is set, stopped when the process exits.
    traffic: Arc<TrafficStream>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            logs: Mutex::new(VecDeque::with_capacity(LOG_BUFFER_CAPACITY)),
            status: Mutex::new(StatusReport::default()),
            started_at: Mutex::new(None),
            current_config: Mutex::new(None),
            controller_url: Mutex::new(None),
            traffic: Arc::new(TrafficStream::new()),
        }
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn push_log(&self, stream: LogStream, line: impl Into<String>) {
        let line = LogLine {
            ts: chrono::Utc::now(),
            stream,
            line: line.into(),
        };
        let mut logs = self.logs.lock().await;
        if logs.len() >= LOG_BUFFER_CAPACITY {
            logs.pop_front();
        }
        logs.push_back(line);
    }

    pub async fn snapshot_status(&self) -> StatusReport {
        self.status.lock().await.clone()
    }

    pub async fn snapshot_logs(&self, limit: usize) -> Vec<LogLine> {
        let logs = self.logs.lock().await;
        let start = logs.len().saturating_sub(limit);
        logs.iter().skip(start).cloned().collect()
    }

    /// Find the bundled sing-box binary.
    ///
    /// Order:
    /// 1. `SINGBOX_BIN` env override (useful for tests / custom builds).
    /// 2. `<app_data_dir>/singbox-runtime/sing-box.exe` — the
    ///    user-writable copy placed by the sing-box auto-update
    ///    (see `updates::apply_singbox_update`). This is what the
    ///    user gets after they accept an auto-update.
    /// 3. `<resource_dir>/binaries/sing-box[-<triple>]` (release).
    /// 4. Walk upwards from `CARGO_MANIFEST_DIR` to find
    ///    `src-tauri/binaries/sing-box-<triple>` (dev).
    pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
        if let Ok(p) = std::env::var("SINGBOX_BIN") {
            let p = PathBuf::from(p);
            if p.exists() {
                return Ok(p);
            }
        }

        // User-writable runtime copy (set by updates::apply_singbox_update).
        // On a fresh install this doesn't exist, so we fall through to
        // the bundled binary. After an auto-update it wins, which is
        // exactly what the user wants — the "newer" version is the
        // one they accepted.
        if let Ok(p) = crate::updates::runtime_bin_path(app) {
            if p.exists() {
                return Ok(p);
            }
        }

        let triple = current_target_triple();
        let exe_name = if cfg!(windows) {
            format!("sing-box-{triple}.exe")
        } else {
            format!("sing-box-{triple}")
        };
        let plain_name = if cfg!(windows) {
            "sing-box.exe".to_string()
        } else {
            "sing-box".to_string()
        };

        // First: same directory as the running executable. On a Linux
        // .deb install, Tauri 2 puts both the main binary and the
        // sidecar in `/usr/bin/` (no `/usr/lib/<pkg>/binaries/`), so
        // this is the only place to look. On Windows NSIS / MSI the
        // same pattern holds. We do this BEFORE the `resource_dir`
        // lookup because on Linux the resource_dir path simply does
        // not exist.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in [&exe_name, &plain_name] {
                    let p = dir.join(name);
                    if p.exists() {
                        return Ok(p);
                    }
                }
            }
        }

        // Release: resource_dir/binaries/...
        if let Ok(resource_dir) = app.path().resource_dir() {
            for name in [&exe_name, &plain_name] {
                let p = resource_dir.join("binaries").join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }

        // Dev: walk upwards from the manifest dir (src-tauri) to find binaries/.
        if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
            let manifest = PathBuf::from(manifest);
            // try: src-tauri/binaries/, ../src-tauri/binaries/, etc.
            let mut cursor: Option<&Path> = Some(manifest.as_path());
            for _ in 0..4 {
                let Some(dir) = cursor else { break };
                for name in [&exe_name, &plain_name] {
                    let p = dir.join("binaries").join(name);
                    if p.exists() {
                        return Ok(p);
                    }
                }
                cursor = dir.parent();
            }
        }

        Err(AppError::BinaryNotFound(format!(
            "expected one of: {exe_name}, {plain_name}"
        )))
    }

    /// Start sing-box with the given config file.
    pub async fn start(
        self: &Arc<Self>,
        app: &AppHandle,
        binary: &Path,
        config_path: &Path,
        controller_url: Option<String>,
    ) -> AppResult<StatusReport> {
        {
            let mut status = self.status.lock().await;
            if matches!(status.status, Status::Starting | Status::Running | Status::Stopping) {
                return Err(AppError::AlreadyRunning(status.pid.unwrap_or(0)));
            }
            status.status = Status::Starting;
            status.last_error = None;
        }
        self.push_log(
            LogStream::System,
            format!("starting sing-box with config {}", config_path.display()),
        )
        .await;

        let mut cmd = Command::new(binary);
        cmd.arg("run")
            .arg("-c")
            .arg(config_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        // Don't pop a console window on Windows release builds.
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        // On Linux, TUN mode requires the sing-box binary itself to hold
        // `cap_net_admin` + `cap_net_raw` (it opens `/dev/net/tun` and
        // configures addresses / routes via netlink, all of which need
        // admin caps). The .deb's postinst applies these via `setcap`
        // on install, but they can be stripped by an `apt upgrade`
        // race, a manual `chmod`, or a package re-install. Detect the
        // situation up front and surface a clear remediation message
        // instead of letting sing-box die with "operation not
        // permitted" half a second later. Best-effort: missing
        // `getcap` or a non-TUN config are no-ops.
        if let Err(msg) = check_tun_capabilities(binary, config_path).await {
            self.push_log(
                LogStream::System,
                format!("TUN capability check failed: {msg}"),
            )
            .await;
            return Err(AppError::TunCapabilities(msg));
        }

        let mut child = cmd.spawn().map_err(|e| {
            AppError::Spawn(format!("{}: {e}", binary.display()))
        })?;
        let pid = child.id();

        // After sing-box brings the TUN interface up, explicitly assign
        // the OS-level DNS server on that interface. Without this,
        // Windows auto-derives a DNS server from the TUN's own address
        // range (e.g. 172.19.0.1/30 → 172.19.0.2), treats it as an
        // on-link neighbour, and tries ARP/Neighbor Discovery instead
        // of routing — the ARP never succeeds, the DNS query never
        // reaches sing-box, and `Resolve-DnsName` (or any direct DNS
        // call by an app) hangs until timeout. Setting an external IP
        // (e.g. 77.88.8.8) as the adapter's DNS server forces
        // Windows to route the query normally through the TUN →
        // sing-box → upstream.
        //
        // We fire-and-forget this on a separate task with a small
        // delay so it doesn't block sing-box's own startup, and so
        // the TUN interface has time to be created by the kernel
        // driver before we try to mutate it.
        let config_path_for_dns = config_path.to_path_buf();
        tokio::spawn(async move {
            // TUN creation typically takes < 100 ms on Windows;
            // 500 ms is a safe margin without making the user wait.
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Err(e) = set_tun_dns_from_config(&config_path_for_dns).await {
                log::warn!("failed to set TUN adapter DNS: {e}");
            }
        });

        // Wire stdout/stderr into the log buffer.
        if let Some(stdout) = child.stdout.take() {
            let me = Arc::clone(self);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    me.push_log(LogStream::Stdout, line).await;
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let me = Arc::clone(self);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    me.push_log(LogStream::Stderr, line).await;
                }
            });
        }

        // Stash the child + bookkeeping.
        {
            let mut guard = self.child.lock().await;
            *guard = Some(child);
        }
        *self.started_at.lock().await = Some(std::time::Instant::now());
        *self.current_config.lock().await = Some(config_path.to_path_buf());
        *self.controller_url.lock().await = controller_url.clone();

        // Auto-start the traffic stream whenever we have a controller
        // URL. The stream emits `traffic` events to the webview.
        if let Some(url) = controller_url {
            let stream = Arc::clone(&self.traffic);
            let app_for_stream = app.clone();
            // Spawn the start on a fire-and-forget task; we don't
            // surface failures here — the stream has its own
            // exponential backoff inside.
            tokio::spawn(async move {
                if let Err(e) = stream.start(app_for_stream, &url).await {
                    log::warn!("traffic stream start failed: {e}");
                }
            });
        }

        let mut status = self.status.lock().await;
        status.status = Status::Running;
        status.pid = pid;
        status.last_exit_code = None;
        Ok(status.clone())
    }

    /// Stop the running sing-box. Graceful: send SIGTERM/taskkill, then
    /// escalate to SIGKILL after `DEFAULT_TERMINATE_TIMEOUT`.
    pub async fn stop(self: &Arc<Self>) -> AppResult<StatusReport> {
        let mut guard = self.child.lock().await;
        let Some(child) = guard.as_mut() else {
            return Err(AppError::NotRunning);
        };

        {
            let mut status = self.status.lock().await;
            status.status = Status::Stopping;
        }
        self.push_log(LogStream::System, "stopping sing-box").await;

        // Stop the traffic stream first so the WS task exits cleanly.
        self.traffic.stop().await;

        // Try graceful first.
        let _ = child.start_kill();
        let pid = child.id();
        drop(guard);

        // Wait for it to exit (with timeout), escalate if needed.
        let deadline = std::time::Instant::now() + DEFAULT_TERMINATE_TIMEOUT;
        loop {
            {
                let mut g = self.child.lock().await;
                if let Some(ch) = g.as_mut() {
                    match ch.try_wait() {
                        Ok(Some(status)) => {
                            *g = None;
                            self.finalize_exit(Some(status.code().unwrap_or(-1)), None).await;
                            return Ok(self.status.lock().await.clone());
                        }
                        Ok(None) => {
                            if std::time::Instant::now() >= deadline {
                                let _ = ch.start_kill();
                                let _ = ch.wait().await;
                                *g = None;
                                self.finalize_exit(
                                    Some(-1),
                                    Some("graceful shutdown timed out, force-killed".to_string()),
                                )
                                .await;
                                return Ok(self.status.lock().await.clone());
                            }
                        }
                        Err(e) => {
                            *g = None;
                            self.finalize_exit(None, Some(format!("try_wait failed: {e}"))).await;
                            return Ok(self.status.lock().await.clone());
                        }
                    }
                } else {
                    return Ok(self.status.lock().await.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = pid; // silence unused warning on platforms where id() is meaningful
        }
    }

    pub async fn is_running(&self) -> bool {
        let g = self.child.lock().await;
        g.is_some()
    }

    pub async fn current_config(&self) -> Option<PathBuf> {
        self.current_config.lock().await.clone()
    }

    pub async fn controller_url(&self) -> Option<String> {
        self.controller_url.lock().await.clone()
    }

    /// Borrow the live traffic-stream handle (for the `traffic_*`
    /// Tauri commands).
    pub fn traffic(&self) -> Arc<TrafficStream> {
        Arc::clone(&self.traffic)
    }

    /// Force-clear the manager state. Used by the `reset_state` command
    /// for manual recovery; the spawned child (if any) is dropped, which
    /// triggers `kill_on_drop` on the underlying Command.
    pub async fn reset(&self) {
        *self.child.lock().await = None;
        *self.status.lock().await = StatusReport::default();
        *self.started_at.lock().await = None;
        *self.current_config.lock().await = None;
        *self.controller_url.lock().await = None;
        self.traffic.stop().await;
        self.push_log(LogStream::System, "process manager state reset").await;
    }

    /// Update the StatusReport after a process has exited.
    ///
    /// Also unconditionally clears the Windows system proxy — the
    /// sing-box process is gone so the proxy setting we wrote on
    /// `start` must come off, otherwise Windows keeps sending traffic
    /// to 127.0.0.1:<port> which is now dead and the user's internet
    /// appears to be down. Idempotent: no-op when there's nothing to
    /// clear (e.g. on platforms without a system proxy or when the
    /// current session was a TUN-only run).
    async fn finalize_exit(&self, code: Option<i32>, err: Option<String>) {
        let mut status = self.status.lock().await;
        status.status = Status::Stopped;
        status.pid = None;
        status.uptime_secs = None;
        status.last_exit_code = code;
        status.last_error = err.clone();
        *self.started_at.lock().await = None;
        *self.current_config.lock().await = None;
        let line = match (&err, code) {
            (Some(e), _) => format!("sing-box exited unexpectedly: {e}"),
            (None, Some(c)) if c != 0 => format!("sing-box exited with code {c}"),
            (None, _) => "sing-box stopped".to_string(),
        };
        drop(status);
        self.push_log(LogStream::System, line).await;
        // Best-effort: roll back the system proxy so Windows doesn't
        // keep trying to talk to a listener that no longer exists.
        // This is the only thing standing between the user and a
        // working internet after a crash.
        if let Err(e) = clear_system_proxy() {
            self.push_log(
                LogStream::System,
                format!("proxy: failed to clear on exit ({e})"),
            )
            .await;
        }
    }

    /// Background watcher: polls the child and surfaces a crash.
    ///
    /// Must be called from inside a tokio runtime (e.g. Tauri's
    /// `setup` callback), NOT at top-level of `run()`.
    pub fn spawn_watcher(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
                let mut guard = self.child.lock().await;
                if let Some(child) = guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code();
                            *guard = None;
                            drop(guard);
                            self.finalize_exit(code, None).await;
                        }
                        Ok(None) => {
                            // Update uptime.
                            if let Some(start) = *self.started_at.lock().await {
                                let mut s = self.status.lock().await;
                                s.uptime_secs = Some(start.elapsed().as_secs());
                            }
                        }
                        Err(e) => {
                            *guard = None;
                            drop(guard);
                            self.finalize_exit(None, Some(format!("try_wait failed: {e}"))).await;
                        }
                    }
                }
            }
        });
    }
}

fn current_target_triple() -> &'static str {
    // We can't easily query rustc at runtime, so we use the build-time hint.
    // The CARGO_CFG_TARGET_TRIPLE env var is set during build.
    option_env!("CARGO_CFG_TARGET_TRIPLE").unwrap_or("x86_64-pc-windows-msvc")
}



// --- System proxy management (Windows only) -----------------------
// When sing-box is in system_proxy mode we have to also tell Windows
// to send HTTP/HTTPS traffic through 127.0.0.1:<port>. Without this,
// the browser etc. go straight to the internet and the proxy has
// nothing to forward.
//
// We use the WinINET registry keys under HKCU and broadcast a
// WM_SETTINGCHANGE so most apps pick it up immediately.

#[cfg(windows)]
pub fn apply_system_proxy(host: &str, port: u16) -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let proxy = format!("{host}:{port}");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            KEY_SET_VALUE,
        )
        .map_err(|e| AppError::Spawn(format!("open Internet Settings: {e}")))?;
    settings
        .set_value("ProxyEnable", &1u32)
        .map_err(|e| AppError::Spawn(format!("set ProxyEnable: {e}")))?;
    settings
        .set_value("ProxyServer", &proxy)
        .map_err(|e| AppError::Spawn(format!("set ProxyServer: {e}")))?;
    Ok(())
}

#[cfg(windows)]
pub fn clear_system_proxy() -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            KEY_SET_VALUE,
        )
        .map_err(|e| AppError::Spawn(format!("open Internet Settings: {e}")))?;
    settings
        .set_value("ProxyEnable", &0u32)
        .map_err(|e| AppError::Spawn(format!("clear ProxyEnable: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_system_proxy(host: &str, port: u16) -> AppResult<()> {
    // On Linux the system proxy is per-desktop-environment. We use
    // gsettings (GNOME, MATE, Cinnamon, XFCE) and fall back to
    // KDE's kwriteconfig if gsettings isn't available. Other DEs
    // (raw i3, sway) don't have a system-wide proxy concept and the
    // user is expected to configure their browser / curl / etc.
    // manually. We log a warning for unsupported DEs and continue
    // — the user can still point individual apps at the proxy.
    //
    // This is best-effort: if every approach fails, we still return
    // Ok(()) so the rest of the start path isn't blocked. The
    // recommendation for full traffic coverage on Linux is TUN mode
    // (not system_proxy), which captures at the network layer
    // rather than relying on per-app proxy support.
    let scheme = if host == "127.0.0.1" || host == "::1" || host == "localhost" {
        "http"
    } else {
        "http"
    };
    let proxy_url = format!("{scheme}://{host}:{port}");

    // Try gsettings (GNOME / MATE / Cinnamon / XFCE / Budgie / Pantheon).
    let gsettings_ok = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "manual"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if gsettings_ok {
        // mode=manual succeeded — set the actual proxy endpoints.
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", host])
            .status();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "port", &port.to_string()])
            .status();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", host])
            .status();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "port", &port.to_string()])
            .status();
        log::info!("set GNOME system proxy to {proxy_url}");
        return Ok(());
    }

    // Try KDE (`kwriteconfig5` writes to kdeglobals; `dbus-send` to
    // kioslave would also work but is more involved). kwriteconfig5
    // doesn't trigger immediate re-read by running apps; users need
    // to re-login or call `dbus-send --session --print-reply
    // --dest=org.kde.kioslaves / kioslave5 reparseConfiguration`
    // themselves. We still set it so newly-spawned apps pick it up.
    let kwrite_ok = std::process::Command::new("kwriteconfig5")
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if kwrite_ok {
        let _ = std::process::Command::new("kwriteconfig5")
            .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "httpProxy", &proxy_url])
            .status();
        let _ = std::process::Command::new("kwriteconfig5")
            .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "httpsProxy", &proxy_url])
            .status();
        log::info!("set KDE system proxy to {proxy_url} (re-login may be required)");
        return Ok(());
    }

    // Last resort: drop a hint in the log. Per-DE and per-app proxy
    // settings vary so much that blanket env-var writes (which only
    // affect new processes spawned by the same shell) aren't worth
    // silently confusing the user.
    log::warn!(
        "system_proxy: no gsettings and no kwriteconfig5 — cannot set a \
         system-wide HTTP proxy on this desktop environment. Use TUN mode \
         for full traffic coverage, or configure your apps to use \
         {proxy_url} manually."
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn clear_system_proxy() -> AppResult<()> {
    // Reverse of apply_system_proxy: revert gsettings to 'none' and
    // kwriteconfig5 to ProxyType=0. Best-effort, same caveats.
    let gsettings_ok = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "none"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if gsettings_ok {
        log::info!("cleared GNOME system proxy");
        return Ok(());
    }
    let kwrite_ok = std::process::Command::new("kwriteconfig5")
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "0"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if kwrite_ok {
        log::info!("cleared KDE system proxy");
        return Ok(());
    }
    log::debug!(
        "clear_system_proxy: no gsettings and no kwriteconfig5 on this \
         system — nothing to clear."
    );
    Ok(())
}


// --- TUN adapter DNS (Windows only) --------------------------------
//
// After sing-box brings the TUN interface up, the OS auto-derives a
// DNS server address from the TUN's own address range (e.g. for
// 172.19.0.1/30 it picks 172.19.0.2). Because that address is in the
// same /30 as the TUN itself, Windows treats it as an on-link
// neighbour and tries ARP/Neighbor Discovery instead of routing —
// the ARP never succeeds, the DNS query never reaches sing-box, and
// apps that resolve names directly (e.g. PowerShell's
// `Resolve-DnsName` without `-Server`) hang on the call.
//
// Fix: explicitly set the TUN adapter's DNS server to an external
// IP (e.g. 77.88.8.8, the same upstream we use in the sing-box
// `dns.servers[0]` block). That IP is NOT in 172.19.0.0/30, so
// Windows routes the DNS query normally through the TUN → sing-box
// → upstream, and the whole resolution path works end-to-end.
//
// On macOS and Linux this is a no-op: the TUN device on those
// platforms doesn't auto-derive a DNS server, so the bug doesn't
// occur.

/// Read the sing-box config we just wrote, find the TUN interface
/// name and the local-DNS server, and apply that DNS server to the
/// adapter at the OS level.
///
/// Best-effort: returns `Err` on any failure (missing fields, netsh
/// not available, etc.) — the caller logs the error and continues.
async fn set_tun_dns_from_config(config_path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let content = tokio::fs::read_to_string(config_path)
            .await
            .map_err(|e| format!("read config {config_path:?}: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("parse config JSON: {e}"))?;

        // Pull the local-DNS server out of `dns.servers[0]`. Falls back
        // to 1.1.1.1 if for any reason the field is missing (e.g. a
        // hand-edited config) — the goal here is "any reachable IP
        // outside the TUN's own /30", not "a specific provider".
        let dns = json
            .get("dns")
            .and_then(|d| d.get("servers"))
            .and_then(|s| s.as_array())
            .and_then(|arr| arr.first())
            .and_then(|s| s.get("server"))
            .and_then(|s| s.as_str())
            .unwrap_or("1.1.1.1")
            .to_string();

        // Pull the TUN interface name. Default matches the generator
        // (`build_inbounds` in `config::mod`).
        let interface = json
            .get("inbounds")
            .and_then(|i| i.as_array())
            .and_then(|arr| arr.iter().find(|i| i.get("type").and_then(|t| t.as_str()) == Some("tun")))
            .and_then(|i| i.get("interface_name"))
            .and_then(|n| n.as_str())
            .unwrap_or("singbox-tun")
            .to_string();

        // `netsh interface ip set dns "<iface>" static <ip> primary`
        // requires elevation. The whole app is already running as
        // admin (TUN needs it), so this should just work.
        let output = std::process::Command::new("netsh")
            .args([
                "interface", "ip", "set", "dns",
                &interface,
                "static",
                &dns,
                "primary",
            ])
            .output()
            .map_err(|e| format!("spawn netsh: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "netsh exit {:?}: stderr={} stdout={}",
                output.status.code(),
                stderr.trim(),
                stdout.trim()
            ));
        }
        log::info!("set TUN adapter '{interface}' DNS to {dns}");
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = config_path;
        Ok(())
    }
}

/// On Linux, TUN mode requires the sing-box binary itself to hold
/// `cap_net_admin` + `cap_net_raw` (it opens `/dev/net/tun` and
/// configures addresses / routes via netlink, all of which need
/// admin caps). The .deb's postinst applies these via `setcap` on
/// install, but they can be stripped by an `apt upgrade` race, a
/// manual `chmod`, or a package re-install. Detect the situation up
/// front and surface a clear remediation message instead of letting
/// sing-box die with "operation not permitted" half a second later.
///
/// Best-effort: missing `getcap` (rare — comes from `libcap2-bin`)
/// or a non-TUN config are no-ops; we only block the spawn when
/// TUN is actually requested AND the caps are missing.
#[cfg(target_os = "linux")]
async fn check_tun_capabilities(binary: &Path, config_path: &Path) -> Result<(), String> {
    // 1) Read the config and check whether any inbound is TUN.
    //    If not, there's nothing to verify — system_proxy and
    //    "None" modes don't need cap_net_admin.
    let content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| format!("read config {}: {e}", config_path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse config JSON: {e}"))?;
    let has_tun = json
        .get("inbounds")
        .and_then(|i| i.as_array())
        .map(|arr| {
            arr.iter()
                .any(|i| i.get("type").and_then(|t| t.as_str()) == Some("tun"))
        })
        .unwrap_or(false);
    if !has_tun {
        return Ok(());
    }

    // 2) `getcap` is shipped by `libcap2-bin` (optional on Debian,
    //    standard on Ubuntu desktop). If it's not installed, fail
    //    soft — sing-box itself will produce a clearer EPERM error
    //    when it tries to open /dev/net/tun.
    let output = match std::process::Command::new("getcap")
        .arg(binary)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::warn!("`getcap` is unavailable; skipping TUN capability preflight");
            return Ok(());
        }
        Err(error) => {
            return Err(format!(
                "could not run `getcap` to verify TUN capabilities: {error}"
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 3) `getcap` output looks like:
    //       /usr/bin/sing-box cap_net_admin,cap_net_raw=ep
    //    TUN requires both capabilities; a partial report must be rejected.
    if output.status.success() && has_required_tun_capabilities(&stdout) {
        log::info!(
            "sing-box {} has cap_net_admin and cap_net_raw — TUN mode is ready",
            binary.display()
        );
        return Ok(());
    }

    Err(format!(
        "TUN mode needs CAP_NET_ADMIN and CAP_NET_RAW on the sing-box binary, but \
         `{}` doesn't have both. Reinstall the .deb (its postinst applies these caps \
         automatically) or run manually:\n  \
         sudo setcap cap_net_admin,cap_net_raw=+ep {}\n  \
         getcap stdout: {}\n  \
         getcap stderr: {}",
        binary.display(),
        binary.display(),
        stdout.trim(),
        stderr.trim(),
    ))
}

#[cfg(target_os = "linux")]
fn has_required_tun_capabilities(getcap_output: &str) -> bool {
    getcap_output.lines().any(|line| {
        // `getcap` emits `<path> <capability-assignment>`. Paths with
        // whitespace are escaped by getcap; unescaped whitespace or extra
        // fields are ambiguous, so fail closed rather than inspect the path.
        let mut fields = line.split_whitespace();
        let Some(_path) = fields.next() else {
            return false;
        };
        let Some(assignment) = fields.next() else {
            return false;
        };
        if fields.next().is_some() {
            return false;
        }
        let Some((capabilities, flags)) = assignment.split_once('=') else {
            return false;
        };
        let has_net_admin = capabilities
            .split(',')
            .any(|capability| capability == "cap_net_admin");
        let has_net_raw = capabilities
            .split(',')
            .any(|capability| capability == "cap_net_raw");
        has_net_admin && has_net_raw && flags.contains('e')
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tun_capability_tests {
    use super::has_required_tun_capabilities;

    #[test]
    fn requires_both_tun_capabilities() {
        assert!(has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,cap_net_raw=ep"
        ));
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin=ep"
        ));
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_raw=ep"
        ));
        assert!(!has_required_tun_capabilities(
            "/tmp/cap_net_admin,cap_net_raw=ep =ep"
        ));
        assert!(!has_required_tun_capabilities(
            "/tmp/cap_net_admin-cap_net_raw/sing-box =ep"
        ));
    }
}


#[cfg(not(target_os = "linux"))]
async fn check_tun_capabilities(_binary: &Path, _config_path: &Path) -> Result<(), String> {
    Ok(())
}
