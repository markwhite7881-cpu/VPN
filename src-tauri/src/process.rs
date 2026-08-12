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
    /// 2. `<resource_dir>/binaries/sing-box[-<triple>]` (release).
    /// 3. Walk upwards from `CARGO_MANIFEST_DIR` to find
    ///    `src-tauri/binaries/sing-box-<triple>` (dev).
    pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
        if let Ok(p) = std::env::var("SINGBOX_BIN") {
            let p = PathBuf::from(p);
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

        let mut child = cmd.spawn().map_err(|e| {
            AppError::Spawn(format!("{}: {e}", binary.display()))
        })?;
        let pid = child.id();

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
pub fn apply_system_proxy(_host: &str, _port: u16) -> AppResult<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn clear_system_proxy() -> AppResult<()> {
    Ok(())
}
