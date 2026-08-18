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
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
#[cfg(test)]
use tokio::task::JoinHandle;

use crate::engine::{EngineKind, LaunchSpec};
use crate::error::{AppError, AppResult};
use crate::traffic::TrafficStream;
use crate::xray::stats::{XrayStatsSpec, XrayStatsStream};

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
    pub engine: Option<EngineKind>,
    pub profile_key: Option<String>,
    pub profile_name: Option<String>,
}

impl Default for StatusReport {
    fn default() -> Self {
        Self {
            status: Status::Stopped,
            pid: None,
            uptime_secs: None,
            last_exit_code: None,
            last_error: None,
            engine: None,
            profile_key: None,
            profile_name: None,
        }
    }
}

/// A child handle paired with the launch generation that owns it.
struct ChildSlot {
    run_id: u64,
    child: Child,
}

#[cfg(test)]
#[derive(Default)]
struct XrayTestState {
    run_id: Option<u64>,
    starts: usize,
    emits: usize,
    events: Vec<&'static str>,
}

/// Centralised state shared across Tauri commands.
pub struct ProcessManager {
    /// Currently running child process, if any.
    child: Mutex<Option<ChildSlot>>,
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
    /// Monotonically increasing ID for each launch attempt.
    next_run_id: AtomicU64,
    /// ID currently allowed to mutate runtime state; zero means no active run.
    active_run_id: AtomicU64,
    /// Run generation currently owning Xray telemetry, or zero when idle.
    xray_stats_run_id: AtomicU64,
    /// Serializes every public API, traffic, and lifecycle transition. Owned
    /// semaphore permits may cross I/O awaits without holding a mutex guard.
    transition: Arc<Semaphore>,
    /// Live traffic WebSocket reader. Started automatically when
    /// `controller_url` is set, stopped when the process exits.
    traffic: Arc<TrafficStream>,
    /// Private Xray StatsService owner; never exposed through command results.
    xray_stats: Arc<XrayStatsStream>,
    /// Test-only observable ownership seam for lifecycle assertions.
    #[cfg(test)]
    xray_test_state: Mutex<XrayTestState>,
    /// Test-only ownership of spawned stdout/stderr tasks, so tests can await
    /// EOF without relying on lifecycle watcher timing.
    #[cfg(test)]
    stdio_readers: Mutex<Vec<JoinHandle<()>>>,
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
            next_run_id: AtomicU64::new(0),
            active_run_id: AtomicU64::new(0),
            xray_stats_run_id: AtomicU64::new(0),
            transition: Arc::new(Semaphore::new(1)),
            traffic: Arc::new(TrafficStream::new()),
            xray_stats: Arc::new(XrayStatsStream::new()),
            #[cfg(test)]
            xray_test_state: Mutex::new(XrayTestState::default()),
            #[cfg(test)]
            stdio_readers: Mutex::new(Vec::new()),
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

    #[cfg(test)]
    pub async fn install_test_child(&self, engine: EngineKind) {
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 60",
        ]);
        let child = cmd.spawn().expect("test child starts");
        let pid = child.id();
        let run_id = self.next_run_id.fetch_add(1, Ordering::AcqRel) + 1;
        self.active_run_id.store(run_id, Ordering::Release);
        *self.child.lock().await = Some(ChildSlot { run_id, child });
        let mut status = self.status.lock().await;
        status.status = Status::Running;
        status.pid = pid;
        status.engine = Some(engine);
    }

    #[cfg(test)]
    async fn await_stdio_readers(&self) {
        let readers = std::mem::take(&mut *self.stdio_readers.lock().await);
        for reader in readers {
            reader.await.expect("stdio reader task completes");
        }
    }

    pub async fn snapshot_logs(&self, limit: usize) -> Vec<LogLine> {
        let logs = self.logs.lock().await;
        let start = logs.len().saturating_sub(limit);
        logs.iter().skip(start).cloned().collect()
    }

    /// Find the bundled sing-box binary using the established search order.
    pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
        crate::engine::singbox::locate_binary(app)
    }

    /// Start exactly one backend-owned engine process.
    pub async fn start_spec(self: &Arc<Self>, spec: LaunchSpec) -> AppResult<StatusReport> {
        self.start_spec_with_app(None, spec).await
    }

    pub async fn start_spec_with_app(
        self: &Arc<Self>,
        app: Option<&AppHandle>,
        spec: LaunchSpec,
    ) -> AppResult<StatusReport> {
        let _clash_transition = self.acquire_transition().await;
        {
            let mut status = self.status.lock().await;
            if matches!(
                status.status,
                Status::Starting | Status::Running | Status::Stopping
            ) {
                return Err(AppError::AlreadyRunning(status.pid.unwrap_or(0)));
            }
            status.status = Status::Starting;
            status.last_error = None;
        }
        let run_id = self.next_run_id.fetch_add(1, Ordering::AcqRel) + 1;
        self.active_run_id.store(run_id, Ordering::Release);
        let label = match spec.engine {
            EngineKind::Singbox => "starting sing-box profile",
            EngineKind::Xray => "starting Xray profile",
        };
        self.push_log(LogStream::System, label).await;
        if spec.engine == EngineKind::Singbox {
            if let Err(message) = check_tun_capabilities(&spec.binary, &spec.config_path).await {
                self.abort_start(run_id).await;
                self.push_log(
                    LogStream::System,
                    format!("TUN capability check failed: {message}"),
                )
                .await;
                return Err(AppError::TunCapabilities(message));
            }
        }
        let mut cmd = Command::new(&spec.binary);
        cmd.args(&spec.args)
            .envs(spec.env.iter().map(|(key, value)| (key, value)))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.abort_start(run_id).await;
                return Err(AppError::Spawn(format!(
                    "could not start sing-box process: {error}"
                )));
            }
        };
        let pid = child.id();
        if spec.engine == EngineKind::Singbox {
            let config_path = spec.config_path.clone();
            let manager = Arc::clone(self);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if !manager.is_active_singbox_run(run_id).await {
                    return;
                }
                if set_tun_dns_from_config(&manager, run_id, &config_path)
                    .await
                    .is_err()
                {
                    log::warn!("could not read runtime configuration for TUN DNS setup");
                }
            });
        }
        if let Some(stdout) = child.stdout.take() {
            let manager = Arc::clone(self);
            let engine = spec.engine;
            let reader = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(line) = frontend_log_line(engine, &line) {
                        manager.push_log(LogStream::Stdout, line).await;
                    }
                }
            });
            #[cfg(test)]
            self.stdio_readers.lock().await.push(reader);
            #[cfg(not(test))]
            drop(reader);
        }
        if let Some(stderr) = child.stderr.take() {
            let manager = Arc::clone(self);
            let engine = spec.engine;
            let reader = tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(line) = frontend_log_line(engine, &line) {
                        manager.push_log(LogStream::Stderr, line).await;
                    }
                }
            });
            #[cfg(test)]
            self.stdio_readers.lock().await.push(reader);
            #[cfg(not(test))]
            drop(reader);
        }
        *self.child.lock().await = Some(ChildSlot { run_id, child });
        *self.started_at.lock().await = Some(std::time::Instant::now());
        *self.current_config.lock().await = Some(spec.config_path);
        if spec.engine == EngineKind::Singbox {
            *self.controller_url.lock().await = spec.controller_url.clone();
        }
        let mut status = self.status.lock().await;
        status.status = Status::Running;
        status.pid = pid;
        status.last_exit_code = None;
        status.engine = Some(spec.engine);
        status.profile_key = spec.profile_key;
        status.profile_name = spec.profile_name;
        let report = status.clone();
        drop(status);
        if spec.engine == EngineKind::Xray {
            if let Some(stats_spec) = spec.xray_stats {
                self.start_xray_telemetry(run_id, app.cloned(), stats_spec)
                    .await;
            }
        }
        if spec.engine == EngineKind::Singbox {
            if let (Some(controller_url), Some(app)) = (spec.controller_url, app.cloned()) {
                if self.is_active_singbox_run(run_id).await {
                    if let Err(error) = self.traffic.start(app, &controller_url).await {
                        log::warn!("traffic stream start failed: {error}");
                    } else if !self.is_active_singbox_run(run_id).await {
                        // Finalization may have invalidated this run while
                        // `TrafficStream::start` was awaiting. Do not leave
                        // an old sing-box controller stream alive for Xray.
                        self.traffic.stop().await;
                    }
                }
            }
        }
        Ok(report)
    }

    /// Compatibility route for existing sing-box callers.
    pub async fn start(
        self: &Arc<Self>,
        app: &AppHandle,
        binary: &Path,
        config_path: &Path,
        controller_url: Option<String>,
    ) -> AppResult<StatusReport> {
        self.start_spec_with_app(
            Some(app),
            LaunchSpec {
                engine: EngineKind::Singbox,
                binary: binary.to_path_buf(),
                args: vec![
                    OsString::from("run"),
                    OsString::from("-c"),
                    config_path.as_os_str().to_os_string(),
                ],
                env: Vec::new(),
                config_path: config_path.to_path_buf(),
                controller_url,
                profile_key: None,
                profile_name: None,
                xray_stats: None,
            },
        )
        .await
    }

    /// Stop the running sing-box. Graceful: send SIGTERM/taskkill, then
    /// escalate to SIGKILL after `DEFAULT_TERMINATE_TIMEOUT`.
    pub async fn stop(self: &Arc<Self>) -> AppResult<StatusReport> {
        let _clash_transition = self.acquire_transition().await;
        let ChildSlot { run_id, mut child } =
            self.child.lock().await.take().ok_or(AppError::NotRunning)?;

        let engine = {
            let mut status = self.status.lock().await;
            if self.active_run_id.load(Ordering::Acquire) == run_id {
                status.status = Status::Stopping;
            }
            status.engine
        };
        let label = match engine {
            Some(EngineKind::Xray) => "stopping Xray",
            _ => "stopping sing-box",
        };
        self.push_log(LogStream::System, label).await;

        if engine == Some(EngineKind::Xray) {
            self.stop_xray_telemetry().await;
        }

        // The traffic stream belongs exclusively to sing-box's Clash controller.
        if engine == Some(EngineKind::Singbox) && self.is_active_run(run_id).await {
            if self.is_active_run(run_id).await {
                self.traffic.stop().await;
            }
        }

        // Try graceful first. The child is owned locally for the rest of this
        // async operation, so no child mutex guard can cross an await.
        let _ = child.start_kill();
        let deadline = std::time::Instant::now() + DEFAULT_TERMINATE_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.finalize_exit(run_id, Some(status.code().unwrap_or(-1)), None)
                        .await;
                    return Ok(self.status.lock().await.clone());
                }
                Ok(None) if std::time::Instant::now() >= deadline => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    self.finalize_exit(
                        run_id,
                        Some(-1),
                        Some("graceful shutdown timed out, force-killed".to_string()),
                    )
                    .await;
                    return Ok(self.status.lock().await.clone());
                }
                Ok(None) => {}
                Err(e) => {
                    self.finalize_exit(run_id, None, Some(format!("try_wait failed: {e}")))
                        .await;
                    return Ok(self.status.lock().await.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn is_running(&self) -> bool {
        let g = self.child.lock().await;
        g.is_some()
    }

    pub async fn current_config(&self) -> Option<PathBuf> {
        let engine = self.status.lock().await.engine;
        let path = self.current_config.lock().await.clone();
        path.and_then(|path| public_config_path(engine.unwrap_or(EngineKind::Singbox), path))
    }

    pub async fn controller_url(&self) -> Option<String> {
        self.clash_controller().await.ok().map(|(_, url)| url)
    }

    /// Acquire the manager-owned gate for a Clash API request. The owned
    /// permit intentionally remains held across the request await, while
    /// avoiding a mutex guard across await.
    pub async fn acquire_clash_api(&self) -> OwnedSemaphorePermit {
        self.acquire_transition().await
    }

    /// Capture the controller URL together with the sing-box launch generation
    /// that owns it. Callers must validate the generation again after I/O.
    pub async fn clash_controller(&self) -> AppResult<(u64, String)> {
        let run_id = self.active_run_id.load(Ordering::Acquire);
        let status = self.status.lock().await;
        if run_id == 0
            || status.status != Status::Running
            || status.engine != Some(EngineKind::Singbox)
        {
            return Err(AppError::Clash("sing-box is not running".to_string()));
        }
        drop(status);
        let url = self
            .controller_url
            .lock()
            .await
            .clone()
            .ok_or_else(|| AppError::Clash("sing-box controller is unavailable".to_string()))?;
        Ok((run_id, url))
    }

    /// Borrow the live traffic-stream handle (for the `traffic_*`
    /// Tauri commands).
    /// Start manual traffic streaming only for the active sing-box run.
    pub async fn start_traffic(self: &Arc<Self>, app: AppHandle) -> AppResult<()> {
        let run_id = self.active_run_id.load(Ordering::Acquire);
        let controller_url =
            {
                let status = self.status.lock().await;
                if status.status != Status::Running || status.engine != Some(EngineKind::Singbox) {
                    return Err(AppError::Clash("sing-box is not running".to_string()));
                }
                drop(status);
                self.controller_url.lock().await.clone().ok_or_else(|| {
                    AppError::Clash("sing-box controller is unavailable".to_string())
                })?
            };
        let _traffic_transition = self.acquire_transition().await;
        let result = self.traffic.start(app, &controller_url).await;
        if let Err(error) = result {
            return Err(error);
        }
        if !self.is_active_singbox_run(run_id).await {
            self.traffic.stop().await;
            return Err(AppError::Clash("sing-box is no longer running".to_string()));
        }
        Ok(())
    }

    /// Stop manual traffic streaming, without allowing a stale command to
    /// stop a stream belonging to a newer launch generation.
    pub async fn stop_traffic(&self) {
        let run_id = self.active_run_id.load(Ordering::Acquire);
        if self.is_active_run(run_id).await {
            let _traffic_transition = self.acquire_transition().await;
            if self.is_active_run(run_id).await {
                self.traffic.stop().await;
            }
        }
    }

    async fn start_xray_telemetry(&self, run_id: u64, app: Option<AppHandle>, spec: XrayStatsSpec) {
        if !self.is_active_run(run_id).await {
            return;
        }
        self.xray_stats.stop().await;
        self.xray_stats_run_id.store(0, Ordering::Release);
        if !self.is_active_run(run_id).await {
            return;
        }

        #[cfg(test)]
        if app.is_none() {
            let mut state = self.xray_test_state.lock().await;
            if self.active_run_id.load(Ordering::Acquire) == run_id {
                state.run_id = Some(run_id);
                state.starts += 1;
                state.events.push("start");
                self.xray_stats_run_id.store(run_id, Ordering::Release);
            }
            return;
        }

        if let Some(app) = app {
            if let Err(error) = self.xray_stats.start(app, spec).await {
                log::warn!("Xray traffic stream start failed: {error}");
                return;
            }
            if self.is_active_run(run_id).await {
                self.xray_stats_run_id.store(run_id, Ordering::Release);
            } else {
                self.xray_stats.stop().await;
            }
        }
    }

    async fn stop_xray_telemetry(&self) {
        let owned_run = self.xray_stats_run_id.swap(0, Ordering::AcqRel);
        self.xray_stats.stop().await;
        #[cfg(test)]
        if owned_run != 0 {
            let mut state = self.xray_test_state.lock().await;
            state.run_id = None;
            state.events.push("stop");
        }
    }

    #[cfg(test)]
    async fn xray_telemetry_run_id(&self) -> Option<u64> {
        let run_id = self.xray_stats_run_id.load(Ordering::Acquire);
        (run_id != 0).then_some(run_id)
    }

    #[cfg(test)]
    async fn test_xray_telemetry_start_count(&self) -> usize {
        self.xray_test_state.lock().await.starts
    }

    #[cfg(test)]
    async fn test_xray_telemetry_emit_count(&self) -> usize {
        self.xray_test_state.lock().await.emits
    }

    #[cfg(test)]
    async fn test_xray_telemetry_events(&self) -> Vec<&'static str> {
        self.xray_test_state.lock().await.events.clone()
    }

    #[cfg(test)]
    async fn test_emit_xray_sample(&self, run_id: u64) {
        let mut state = self.xray_test_state.lock().await;
        if self.xray_stats_run_id.load(Ordering::Acquire) == run_id
            && self.active_run_id.load(Ordering::Acquire) == run_id
        {
            state.emits += 1;
        }
    }

    async fn reset_with_proxy_cleanup<F>(&self, clear_proxy: F)
    where
        F: FnOnce() -> AppResult<()>,
    {
        if let Err(error) = clear_proxy() {
            self.push_log(
                LogStream::System,
                format!("proxy: failed to clear during reset ({error})"),
            )
            .await;
        }
        self.stop_xray_telemetry().await;
        self.active_run_id.store(0, Ordering::Release);
        *self.child.lock().await = None;
        *self.status.lock().await = StatusReport::default();
        #[cfg(test)]
        self.xray_test_state.lock().await.events.push("state_clear");
        *self.started_at.lock().await = None;
        *self.current_config.lock().await = None;
        self.controller_url.lock().await.take();
        self.traffic.stop().await;
        self.push_log(LogStream::System, "process manager state reset")
            .await;
    }

    /// Force-clear the manager state. Used by the `reset_state` command
    /// for manual recovery; the spawned child (if any) is dropped, which
    /// triggers `kill_on_drop` on the underlying Command.
    pub async fn reset(&self) {
        let _clash_transition = self.acquire_transition().await;
        self.reset_with_proxy_cleanup(clear_system_proxy).await;
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
    async fn finalize_exit(&self, run_id: u64, code: Option<i32>, err: Option<String>) {
        if self.active_run_id.load(Ordering::Acquire) != run_id {
            return;
        }

        // Unexpected exits bypass `stop`; cancel the old run before another
        // launch can start a sing-box traffic task.
        self.stop_xray_telemetry().await;
        self.traffic.stop().await;
        if self.active_run_id.load(Ordering::Acquire) != run_id {
            return;
        }
        *self.started_at.lock().await = None;
        *self.current_config.lock().await = None;
        *self.controller_url.lock().await = None;

        let line = {
            let mut status = self.status.lock().await;
            if self.active_run_id.load(Ordering::Acquire) != run_id {
                return;
            }
            let is_xray = status.engine == Some(EngineKind::Xray);
            let engine_label = if is_xray { "Xray" } else { "sing-box" };
            status.status = Status::Stopped;
            status.pid = None;
            status.uptime_secs = None;
            status.last_exit_code = code;
            status.last_error = (!is_xray).then(|| err.clone()).flatten();
            status.engine = None;
            status.profile_key = None;
            status.profile_name = None;
            // Status is the launch gate. Invalidate this ID before dropping it
            // so older async tasks cannot touch a later run.
            self.active_run_id.store(0, Ordering::Release);
            if is_xray {
                if code == Some(0) {
                    "Xray stopped".to_string()
                } else {
                    "Xray stopped unexpectedly".to_string()
                }
            } else {
                match (&err, code) {
                    (Some(error), _) => format!("{engine_label} exited unexpectedly: {error}"),
                    (None, Some(exit_code)) if exit_code != 0 => {
                        format!("{engine_label} exited with code {exit_code}")
                    }
                    (None, _) => format!("{engine_label} stopped"),
                }
            }
        };
        #[cfg(test)]
        self.xray_test_state.lock().await.events.push("state_clear");
        self.push_log(LogStream::System, line).await;
        if let Err(error) = clear_system_proxy() {
            self.push_log(
                LogStream::System,
                format!("proxy: failed to clear on exit ({error})"),
            )
            .await;
        }
    }

    async fn acquire_transition(&self) -> OwnedSemaphorePermit {
        self.transition
            .clone()
            .acquire_owned()
            .await
            .expect("process transition semaphore is never closed")
    }

    async fn take_child_if_run(&self, run_id: u64) -> bool {
        let mut child = self.child.lock().await;
        if child.as_ref().is_some_and(|slot| slot.run_id == run_id) {
            child.take();
            true
        } else {
            false
        }
    }

    async fn is_active_run(&self, run_id: u64) -> bool {
        run_id != 0 && self.active_run_id.load(Ordering::Acquire) == run_id
    }

    pub async fn is_active_singbox_run(&self, run_id: u64) -> bool {
        if self.active_run_id.load(Ordering::Acquire) != run_id {
            return false;
        }
        let status = self.status.lock().await;
        status.status == Status::Running && status.engine == Some(EngineKind::Singbox)
    }

    async fn abort_start(&self, run_id: u64) {
        if self.active_run_id.load(Ordering::Acquire) != run_id {
            return;
        }
        self.stop_xray_telemetry().await;
        let mut status = self.status.lock().await;
        if self.active_run_id.load(Ordering::Acquire) == run_id {
            *status = StatusReport::default();
            self.active_run_id.store(0, Ordering::Release);
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
                let exited = {
                    let mut guard = self.child.lock().await;
                    guard.as_mut().map(|slot| {
                        let run_id = slot.run_id;
                        let result = match slot.child.try_wait() {
                            Ok(Some(status)) => Some(Ok(status)),
                            Ok(None) => None,
                            Err(error) => Some(Err(error)),
                        };
                        (run_id, result)
                    })
                };
                match exited {
                    Some((run_id, Some(Ok(status)))) => {
                        let code = status.code();
                        // A reset/relaunch may have replaced the slot while we
                        // were outside the child mutex. Never take its newer
                        // handle for an older watcher observation.
                        let owns_slot = self.take_child_if_run(run_id).await;
                        if owns_slot {
                            let _clash_transition = self.acquire_transition().await;
                            self.finalize_exit(run_id, code, None).await;
                        }
                    }
                    Some((run_id, Some(Err(error)))) => {
                        let owns_slot = self.take_child_if_run(run_id).await;
                        if owns_slot {
                            let _clash_transition = self.acquire_transition().await;
                            self.finalize_exit(
                                run_id,
                                None,
                                Some(format!("try_wait failed: {error}")),
                            )
                            .await;
                        }
                    }
                    Some((_run_id, None)) => {
                        // Update uptime without retaining the child guard.
                        if self.child.lock().await.is_some() {
                            if let Some(start) = *self.started_at.lock().await {
                                let mut status = self.status.lock().await;
                                status.uptime_secs = Some(start.elapsed().as_secs());
                            }
                        }
                    }
                    None => {}
                }
            }
        });
    }
}

fn frontend_log_line(engine: EngineKind, line: &str) -> Option<String> {
    (engine == EngineKind::Singbox).then(|| line.to_owned())
}

fn public_config_path(engine: EngineKind, path: PathBuf) -> Option<PathBuf> {
    (engine == EngineKind::Singbox).then_some(path)
}

// --- System proxy management ---------------------------------------
// Windows uses WinINET registry keys, Linux uses desktop-environment
// commands, and macOS uses `networksetup` per active network service.

#[cfg(target_os = "android")]
pub fn apply_system_proxy(_host: &str, _port: u16) -> AppResult<()> {
    Ok(())
}

#[cfg(target_os = "android")]
pub fn clear_system_proxy() -> AppResult<()> {
    Ok(())
}

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

#[cfg(target_os = "linux")]
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
            .args([
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &port.to_string(),
            ])
            .status();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", host])
            .status();
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &port.to_string(),
            ])
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
        .args([
            "--file",
            "kioslaverc",
            "--group",
            "Proxy Settings",
            "--key",
            "ProxyType",
            "1",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if kwrite_ok {
        let _ = std::process::Command::new("kwriteconfig5")
            .args([
                "--file",
                "kioslaverc",
                "--group",
                "Proxy Settings",
                "--key",
                "httpProxy",
                &proxy_url,
            ])
            .status();
        let _ = std::process::Command::new("kwriteconfig5")
            .args([
                "--file",
                "kioslaverc",
                "--group",
                "Proxy Settings",
                "--key",
                "httpsProxy",
                &proxy_url,
            ])
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

#[cfg(target_os = "linux")]
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
        .args([
            "--file",
            "kioslaverc",
            "--group",
            "Proxy Settings",
            "--key",
            "ProxyType",
            "0",
        ])
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

// --- System proxy management (macOS) --------------------------------
//
// macOS exposes system proxies through `networksetup`. Apply the HTTP and
// HTTPS proxy to every enabled network service because the active interface
// can change between Wi-Fi, Ethernet, and other services while the app runs.
// Per-service failures are expected (for example, inactive bridge services),
// so they are logged and do not prevent other services from being updated.

#[cfg(target_os = "macos")]
fn enabled_network_services() -> Vec<String> {
    let output = match std::process::Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            log::warn!(
                "macos-proxy: networksetup -listallnetworkservices failed to spawn: {error}"
            );
            return Vec::new();
        }
    };
    if !output.status.success() {
        log::warn!(
            "macos-proxy: networksetup -listallnetworkservices exited with {:?}",
            output.status.code()
        );
        return Vec::new();
    }

    // The first line is the disabled-service legend; disabled services begin
    // with `*`. Preserve full service names because they may contain spaces.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter(|line| !line.starts_with('*') && !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "macos")]
fn run_networksetup(args: &[&str]) {
    match std::process::Command::new("networksetup")
        .args(args)
        .output()
    {
        Ok(output) if !output.status.success() => {
            log::warn!(
                "macos-proxy: networksetup {} exited {:?}: {}",
                args.join(" "),
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(error) => {
            log::warn!(
                "macos-proxy: networksetup {} failed to spawn: {error}",
                args.join(" ")
            );
        }
        Ok(_) => {}
    }
}

#[cfg(target_os = "macos")]
pub fn apply_system_proxy(host: &str, port: u16) -> AppResult<()> {
    let services = enabled_network_services();
    if services.is_empty() {
        return Err(AppError::Spawn(
            "no enabled network services (networksetup returned empty)".into(),
        ));
    }

    let port = port.to_string();
    for service in &services {
        log::info!("macos-proxy: applying to {service}");
        run_networksetup(&["-setwebproxy", service.as_str(), host, port.as_str()]);
        run_networksetup(&["-setsecurewebproxy", service.as_str(), host, port.as_str()]);
        run_networksetup(&["-setwebproxystate", service.as_str(), "on"]);
        run_networksetup(&["-setsecurewebproxystate", service.as_str(), "on"]);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn clear_system_proxy() -> AppResult<()> {
    for service in enabled_network_services() {
        log::info!("macos-proxy: clearing on {service}");
        run_networksetup(&["-setwebproxystate", service.as_str(), "off"]);
        run_networksetup(&["-setsecurewebproxystate", service.as_str(), "off"]);
    }
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
async fn set_tun_dns_from_config(
    manager: &ProcessManager,
    run_id: u64,
    config_path: &Path,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let content = tokio::fs::read_to_string(config_path)
            .await
            .map_err(|_| "could not read runtime configuration".to_string())?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|_| "runtime configuration is invalid".to_string())?;

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
            .and_then(|arr| {
                arr.iter()
                    .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("tun"))
            })
            .and_then(|i| i.get("interface_name"))
            .and_then(|n| n.as_str())
            .unwrap_or("singbox-tun")
            .to_string();

        // The config read above may have awaited long enough for this run to
        // be replaced. Validate ownership immediately before mutating DNS.
        if !manager.is_active_singbox_run(run_id).await {
            return Err("stale TUN DNS setup".to_string());
        }

        // `netsh interface ip set dns "<iface>" static <ip> primary`
        // requires elevation. The whole app is already running as
        // admin (TUN needs it), so this should just work.
        let output = std::process::Command::new("netsh")
            .args([
                "interface",
                "ip",
                "set",
                "dns",
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
        let _ = (manager, run_id, config_path);
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
        .map_err(|_| "could not read runtime configuration".to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|_| "runtime configuration is invalid".to_string())?;
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
    let output = match std::process::Command::new("getcap").arg(binary).output() {
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
        log::info!("sing-box TUN capabilities are ready");
        return Ok(());
    }

    let _ = (stdout, stderr);
    Err(
        "TUN mode needs CAP_NET_ADMIN and CAP_NET_RAW on the sing-box binary. \
         Reinstall the .deb (its postinstall restores these capabilities) or run:\n  \
         sudo setcap cap_net_admin,cap_net_raw=+ep <sing-box-binary>"
            .to_string(),
    )
}

#[cfg(target_os = "linux")]
fn has_required_tun_capabilities(getcap_output: &str) -> bool {
    // `getcap <path>` emits at most one record for the supplied binary. Reject
    // anything other than exactly one nonempty record, so a valid-looking line
    // cannot mask malformed or ambiguous extra output.
    let mut lines = getcap_output.lines().filter(|line| !line.trim().is_empty());
    let Some(line) = lines.next() else {
        return false;
    };
    if lines.next().is_some() {
        return false;
    }

    // `getcap` emits `<path> <capability-assignment>`. Paths with whitespace
    // are escaped by getcap; unescaped whitespace or extra fields are
    // ambiguous, so fail closed without treating the path as capability input.
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

    // A libcap text assignment has one `=` followed by a nonempty subset of
    // `eip`. Keep accepting additional valid capabilities, but reject unknown,
    // empty, repeated, or otherwise malformed tokens.
    if assignment.matches('=').count() != 1 {
        return false;
    }
    let Some((capabilities, flags)) = assignment.split_once('=') else {
        return false;
    };
    if capabilities.is_empty()
        || flags.is_empty()
        || !flags.contains('e')
        || !flags.chars().all(|flag| matches!(flag, 'e' | 'i' | 'p'))
        || !flags.chars().all(|flag| flags.matches(flag).count() == 1)
    {
        return false;
    }

    let mut has_net_admin = false;
    let mut has_net_raw = false;
    for capability in capabilities.split(',') {
        if !is_linux_capability_name(capability) {
            return false;
        }
        match capability {
            "cap_net_admin" => has_net_admin = true,
            "cap_net_raw" => has_net_raw = true,
            _ => {}
        }
    }
    has_net_admin && has_net_raw
}

#[cfg(target_os = "linux")]
fn is_linux_capability_name(capability: &str) -> bool {
    matches!(
        capability,
        "cap_chown"
            | "cap_dac_override"
            | "cap_dac_read_search"
            | "cap_fowner"
            | "cap_fsetid"
            | "cap_kill"
            | "cap_setgid"
            | "cap_setuid"
            | "cap_setpcap"
            | "cap_linux_immutable"
            | "cap_net_bind_service"
            | "cap_net_broadcast"
            | "cap_net_admin"
            | "cap_net_raw"
            | "cap_ipc_lock"
            | "cap_ipc_owner"
            | "cap_sys_module"
            | "cap_sys_rawio"
            | "cap_sys_chroot"
            | "cap_sys_ptrace"
            | "cap_sys_pacct"
            | "cap_sys_admin"
            | "cap_sys_boot"
            | "cap_sys_nice"
            | "cap_sys_resource"
            | "cap_sys_time"
            | "cap_sys_tty_config"
            | "cap_mknod"
            | "cap_lease"
            | "cap_audit_write"
            | "cap_audit_control"
            | "cap_setfcap"
            | "cap_mac_override"
            | "cap_mac_admin"
            | "cap_syslog"
            | "cap_wake_alarm"
            | "cap_block_suspend"
            | "cap_audit_read"
            | "cap_perfmon"
            | "cap_bpf"
            | "cap_checkpoint_restore"
    )
}

#[cfg(test)]
mod clash_controller_tests {
    use super::*;

    #[tokio::test]
    async fn clash_transition_serializes_api_requests() {
        let manager = ProcessManager::new();
        let permit = manager.acquire_clash_api().await;

        let blocked =
            tokio::time::timeout(Duration::from_millis(10), manager.acquire_clash_api()).await;
        assert!(blocked.is_err());
        drop(permit);

        let _permit = tokio::time::timeout(Duration::from_millis(100), manager.acquire_clash_api())
            .await
            .expect("gate opens after the request permit is released");
    }

    #[tokio::test]
    async fn clash_api_and_lifecycle_share_one_gate() {
        let manager = ProcessManager::new();
        let permit = manager.acquire_clash_api().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(10), manager.acquire_transition())
                .await
                .is_err()
        );
        drop(permit);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), manager.acquire_transition())
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    async fn clash_controller_rejects_xray_runs() {
        let manager = ProcessManager::new();
        let mut status = manager.status.lock().await;
        status.status = Status::Running;
        status.engine = Some(EngineKind::Xray);
        drop(status);
        *manager.controller_url.lock().await = Some("http://127.0.0.1:9090".to_string());

        let error = manager.clash_controller().await.unwrap_err();
        assert!(error.to_string().contains("sing-box is not running"));
    }

    #[tokio::test]
    async fn clash_controller_captures_generation_and_rejects_stale_generation() {
        let manager = ProcessManager::new();
        manager.active_run_id.store(7, Ordering::Release);
        let mut status = manager.status.lock().await;
        status.status = Status::Running;
        status.engine = Some(EngineKind::Singbox);
        drop(status);
        *manager.controller_url.lock().await = Some("http://127.0.0.1:9090".to_string());

        let (run_id, url) = manager
            .clash_controller()
            .await
            .expect("controller available");
        assert_eq!(run_id, 7);
        assert_eq!(url, "http://127.0.0.1:9090");
        assert!(manager.is_active_singbox_run(run_id).await);

        manager.active_run_id.store(8, Ordering::Release);
        assert!(!manager.is_active_singbox_run(run_id).await);
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;

    #[tokio::test]
    async fn reset_clears_proxy_before_forgetting_process_state() {
        let manager = ProcessManager::new();
        manager.status.lock().await.status = Status::Running;

        manager
            .reset_with_proxy_cleanup(|| {
                assert_eq!(
                    manager
                        .status
                        .try_lock()
                        .expect("status lock is available")
                        .status,
                    Status::Running
                );
                Ok(())
            })
            .await;

        assert_eq!(manager.snapshot_status().await.status, Status::Stopped);
    }
}

#[cfg(test)]
mod engine_runtime_tests {
    use super::*;
    use crate::subscriptions::EngineKind;
    use std::ffi::OsString;

    fn test_xray_spec() -> LaunchSpec {
        LaunchSpec {
            engine: EngineKind::Xray,
            binary: PathBuf::from("powershell.exe"),
            args: vec![
                OsString::from("-NoProfile"),
                OsString::from("-NonInteractive"),
                OsString::from("-Command"),
                OsString::from("Start-Sleep -Seconds 60"),
            ],
            env: Vec::new(),
            config_path: PathBuf::from("xray-test-config.json"),
            controller_url: Some("http://controller.test".into()),
            profile_key: Some("test-xray".into()),
            profile_name: Some("Test Xray".into()),
            xray_stats: None,
        }
    }

    #[tokio::test]
    async fn xray_spawned_stdio_is_not_exposed_to_frontend_logs() {
        const STDOUT_SECRET: &str = "xray-stdout-secret-marker";
        const STDERR_SECRET: &str = "xray-stderr-secret-marker";

        let pm = Arc::new(ProcessManager::new());
        let mut spec = test_xray_spec();
        spec.args = vec![
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-Command"),
            OsString::from(format!(
                "[Console]::Out.WriteLine('{STDOUT_SECRET}'); [Console]::Error.WriteLine('{STDERR_SECRET}')"
            )),
        ];

        pm.start_spec(spec).await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), pm.await_stdio_readers())
            .await
            .expect("Xray stdout and stderr readers drain before logs are inspected");

        let logs = pm.snapshot_logs(10).await;
        assert!(logs.iter().all(|log| !log.line.contains(STDOUT_SECRET)));
        assert!(logs.iter().all(|log| !log.line.contains(STDERR_SECRET)));
    }

    #[tokio::test]
    async fn xray_output_is_not_exposed_to_frontend_logs() {
        let manager = ProcessManager::new();
        let secret_output =
            "uuid=secret-uuid https://provider.example/sub C:\\runtime\\config.json";

        if let Some(line) = frontend_log_line(EngineKind::Xray, secret_output) {
            manager.push_log(LogStream::Stdout, line).await;
        }

        assert!(manager.snapshot_logs(10).await.is_empty());
    }

    #[tokio::test]
    async fn xray_runtime_config_path_is_not_public() {
        let manager = ProcessManager::new();
        let secret_path = PathBuf::from("C:\\runtime\\secret-xray-config.json");
        manager.status.lock().await.engine = Some(EngineKind::Xray);
        *manager.current_config.lock().await = Some(secret_path);

        assert_eq!(manager.current_config().await, None);
    }

    #[test]
    fn singbox_output_and_config_path_remain_public() {
        let line = "sing-box safe output";
        let path = PathBuf::from("C:\\runtime\\singbox-config.json");

        assert_eq!(
            frontend_log_line(EngineKind::Singbox, line),
            Some(line.to_owned())
        );
        assert_eq!(
            public_config_path(EngineKind::Singbox, path.clone()),
            Some(path)
        );
    }

    #[tokio::test]
    async fn start_rejects_second_engine_while_first_is_active() {
        let pm = Arc::new(ProcessManager::new());
        pm.install_test_child(EngineKind::Singbox).await;

        let err = pm.start_spec(test_xray_spec()).await.unwrap_err();

        assert!(matches!(err, AppError::AlreadyRunning(_)));
    }

    #[tokio::test]
    async fn xray_launch_does_not_expose_a_clash_controller() {
        let pm = Arc::new(ProcessManager::new());
        pm.start_spec(test_xray_spec()).await.unwrap();

        assert_eq!(pm.controller_url().await, None);
        pm.stop().await.unwrap();
        assert_eq!(pm.controller_url().await, None);
    }

    #[tokio::test]
    async fn spawn_failure_redacts_binary_path() {
        let pm = Arc::new(ProcessManager::new());
        let secret_path = PathBuf::from("/private/secret/sing-box");
        let mut spec = test_xray_spec();
        spec.binary = secret_path.clone();
        spec.args.clear();

        let error = pm.start_spec(spec).await.unwrap_err().to_string();

        assert!(!error.contains(secret_path.to_string_lossy().as_ref()));
        assert!(error.contains("could not start sing-box process"));
    }

    #[tokio::test]
    async fn stale_stop_finalizer_does_not_stop_newer_run() {
        let pm = ProcessManager::new();
        pm.active_run_id.store(2, Ordering::Release);
        {
            let mut status = pm.status.lock().await;
            status.status = Status::Running;
            status.engine = Some(EngineKind::Xray);
            status.profile_key = Some("newer".into());
        }

        pm.finalize_exit(1, Some(-1), Some("old stop".into())).await;

        let status = pm.snapshot_status().await;
        assert_eq!(status.status, Status::Running);
        assert_eq!(status.profile_key.as_deref(), Some("newer"));
        assert_eq!(pm.active_run_id.load(Ordering::Acquire), 2);
    }

    #[tokio::test]
    async fn stale_watcher_cannot_take_newer_child_slot() {
        let pm = ProcessManager::new();
        pm.install_test_child(EngineKind::Xray).await;
        let old_run_id = pm.active_run_id.load(Ordering::Acquire);
        pm.reset_with_proxy_cleanup(|| Ok(())).await;
        pm.install_test_child(EngineKind::Xray).await;
        let new_run_id = pm.active_run_id.load(Ordering::Acquire);

        assert_ne!(old_run_id, new_run_id);
        assert!(!pm.take_child_if_run(old_run_id).await);
        assert!(pm.is_running().await);
        assert!(pm.take_child_if_run(new_run_id).await);
        assert!(!pm.is_running().await);
    }

    #[tokio::test]
    async fn stale_exit_does_not_clear_a_newer_run() {
        let pm = ProcessManager::new();
        pm.active_run_id.store(2, Ordering::Release);
        {
            let mut status = pm.status.lock().await;
            status.status = Status::Running;
            status.engine = Some(EngineKind::Xray);
            status.profile_key = Some("newer".into());
        }

        pm.finalize_exit(1, Some(0), None).await;

        let status = pm.snapshot_status().await;
        assert_eq!(status.status, Status::Running);
        assert_eq!(status.engine, Some(EngineKind::Xray));
        assert_eq!(status.profile_key.as_deref(), Some("newer"));
    }

    #[tokio::test]
    async fn dns_ownership_gate_rejects_stale_or_non_singbox_runs() {
        let pm = ProcessManager::new();
        pm.active_run_id.store(7, Ordering::Release);
        {
            let mut status = pm.status.lock().await;
            status.status = Status::Running;
            status.engine = Some(EngineKind::Singbox);
        }
        assert!(pm.is_active_singbox_run(7).await);

        pm.active_run_id.store(8, Ordering::Release);
        assert!(!pm.is_active_singbox_run(7).await);

        pm.active_run_id.store(8, Ordering::Release);
        pm.status.lock().await.engine = Some(EngineKind::Xray);
        assert!(!pm.is_active_singbox_run(8).await);
    }

    #[tokio::test]
    async fn xray_exit_uses_xray_label_and_clears_metadata() {
        let pm = ProcessManager::new();
        {
            let mut status = pm.status.lock().await;
            status.engine = Some(EngineKind::Xray);
            status.profile_key = Some("profile".into());
            status.profile_name = Some("Xray profile".into());
        }

        pm.active_run_id.store(1, Ordering::Release);
        pm.finalize_exit(1, Some(0), None).await;

        let logs = pm.snapshot_logs(1).await;
        assert_eq!(logs[0].line, "Xray stopped");
        let status = pm.snapshot_status().await;
        assert_eq!(status.engine, None);
        assert_eq!(status.profile_key, None);
        assert_eq!(status.profile_name, None);
    }

    #[tokio::test]
    async fn xray_launch_starts_one_telemetry_owner() {
        let pm = Arc::new(ProcessManager::new());
        let status = pm.start_spec(test_xray_spec_with_stats()).await.unwrap();
        let run_id = pm.active_run_id.load(Ordering::Acquire);

        assert_eq!(status.status, Status::Running);
        assert_eq!(pm.xray_telemetry_run_id().await, Some(run_id));
        assert_eq!(pm.test_xray_telemetry_start_count().await, 1);
        pm.stop().await.unwrap();
    }

    #[tokio::test]
    async fn stopping_xray_cancels_telemetry_before_process_state_is_cleared() {
        let pm = Arc::new(ProcessManager::new());
        pm.start_spec(test_xray_spec_with_stats()).await.unwrap();

        pm.stop().await.unwrap();

        assert_eq!(pm.xray_telemetry_run_id().await, None);
        assert_eq!(pm.snapshot_status().await.status, Status::Stopped);
        assert_eq!(
            pm.test_xray_telemetry_events().await,
            vec!["start", "stop", "state_clear"]
        );
    }

    #[tokio::test]
    async fn replacing_xray_with_singbox_cannot_emit_from_old_xray_run() {
        let pm = Arc::new(ProcessManager::new());
        pm.start_spec(test_xray_spec_with_stats()).await.unwrap();
        let old_run_id = pm.active_run_id.load(Ordering::Acquire);
        pm.stop().await.unwrap();

        pm.start_spec(test_singbox_spec()).await.unwrap();
        pm.test_emit_xray_sample(old_run_id).await;

        assert_eq!(pm.test_xray_telemetry_emit_count().await, 0);
        pm.stop().await.unwrap();
    }

    #[tokio::test]
    async fn failed_xray_spawn_does_not_leave_telemetry_running() {
        let pm = Arc::new(ProcessManager::new());
        let mut spec = test_xray_spec_with_stats();
        spec.binary = PathBuf::from("definitely-missing-xray-binary");

        assert!(pm.start_spec(spec).await.is_err());
        assert_eq!(pm.xray_telemetry_run_id().await, None);
        assert_eq!(pm.test_xray_telemetry_start_count().await, 0);
    }

    fn test_xray_spec_with_stats() -> LaunchSpec {
        let mut spec = test_xray_spec();
        spec.xray_stats = Some(crate::xray::stats::XrayStatsSpec {
            api_host: "127.0.0.1".into(),
            api_port: 29001,
            traffic_tag: "xray-test".into(),
        });
        spec
    }

    fn test_singbox_spec() -> LaunchSpec {
        let mut spec = test_xray_spec();
        spec.engine = EngineKind::Singbox;
        spec.xray_stats = None;
        spec
    }

    #[test]
    fn stopped_status_has_no_engine_or_profile() {
        let status = StatusReport::default();

        assert_eq!(status.engine, None);
        assert_eq!(status.profile_key, None);
        assert_eq!(status.profile_name, None);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tun_capability_tests {
    use super::has_required_tun_capabilities;

    #[test]
    fn accepts_a_well_formed_effective_tun_capability_assignment() {
        assert!(has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,cap_net_raw=ep"
        ));
        assert!(has_required_tun_capabilities(
            "/usr/bin/sing-box cap_chown,cap_net_admin,cap_net_raw=eip"
        ));
    }

    #[test]
    fn rejects_incomplete_or_path_derived_capabilities() {
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

    #[test]
    fn rejects_ambiguous_or_malformed_getcap_output() {
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,cap_net_raw=ep\n/usr/bin/other malformed"
        ));
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,cap_net_raw=e=garbage"
        ));
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,cap_net_raw=ez"
        ));
        assert!(!has_required_tun_capabilities(
            "/usr/bin/sing-box cap_net_admin,,cap_net_raw=ep"
        ));
    }
}

#[cfg(not(target_os = "linux"))]
async fn check_tun_capabilities(_binary: &Path, _config_path: &Path) -> Result<(), String> {
    Ok(())
}
