//! Bango AI engine manager: a single managed `llama-server` child process.
//!
//! Lifecycle (plan sections 5-6): the loopback port is reserved once at
//! construction (B1) so callers can receive a complete effective config
//! before the server starts; the process spawns lazily on first use, binds
//! only `127.0.0.1`, carries a per-start random API key, sleeps natively
//! after 10 minutes idle (`--sleep-idle-seconds`), and is stopped on backend
//! switch, remove, and app exit. Spawn and health probing are trait seams so
//! lifecycle tests run without a real server binary.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

use crate::db::connection::lock_state;
use crate::error::AppError;
use crate::llm::local::profile::LOCAL_LLM_PROFILE_ID;
use crate::models::llm_config::{LlmConfig, LlmProvider};

/// Seconds after which the server unloads the model in place while idle.
pub const IDLE_SLEEP_SECS: u64 = 600;

/// Effective `--sleep-idle-seconds` value. The test-only
/// `BANGO_AI_IDLE_SLEEP_SECS` override mirrors the existing TEST-ONLY env
/// conventions and is compiled out of release builds (aifixes1 F9).
#[must_use]
pub fn idle_sleep_secs() -> u64 {
    #[cfg(debug_assertions)]
    if let Some(secs) = std::env::var("BANGO_AI_IDLE_SLEEP_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|secs| *secs > 0)
    {
        return secs;
    }
    IDLE_SLEEP_SECS
}

/// How long a cold start (model load included) may take before failing.
pub const START_TIMEOUT_SECS: u64 = 180;

/// Health poll cadence while starting.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Spawn attempts before the port reservation is abandoned.
pub const PORT_RETRIES: usize = 3;

/// One crash restart per failure streak, then `Failed`.
const MAX_RESTARTS: u8 = 1;

/// Fast drain window before a reset stops waiting in-line; a busy engine
/// finishes its stop on a detached task so callers return promptly.
const IN_FLIGHT_DRAIN_FAST: Duration = Duration::from_secs(5);

/// Poll cadence while draining in-flight local requests.
const IN_FLIGHT_DRAIN_POLL: Duration = Duration::from_millis(25);

/// Ceiling for the detached drain: no local request may outlive its own
/// 1800 s wall-clock budget (`orchestrator::LOCAL_TIMEOUT_SECS`).
const LOCAL_DRAIN_MAX: Duration = Duration::from_secs(crate::llm::orchestrator::LOCAL_TIMEOUT_SECS);

/// Server state as reported to the command layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    Stopped,
    Starting,
    Ready,
    Failed(String),
}

impl EngineState {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Failed(_) => "failed",
        }
    }
}

/// User-tunable engine settings live in the pure policy module (shared with
/// the config resolver without a module cycle).
pub use crate::llm::local::policy::EngineSettings;

/// The process-wide reserved loopback port (set by the first engine
/// construction) so the T6 config resolver can build a complete local config
/// before any server start.
static RESERVED_PORT: AtomicU16 = AtomicU16::new(0);

/// The reserved loopback port, once an engine has been constructed (updated
/// on every re-reserve so pre-start config never advertises a stale port).
#[must_use]
pub fn reserved_port() -> Option<u16> {
    let port = RESERVED_PORT.load(Ordering::Acquire);
    (port != 0).then_some(port)
}

fn publish_reserved_port(port: u16) {
    RESERVED_PORT.store(port, Ordering::Release);
}

/// Effective loopback config handed to the orchestrator for local calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub context: i32,
}

/// Paths the spawn needs.
#[derive(Debug, Clone)]
pub struct ServerSpec {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub log: PathBuf,
}

/// A spawned server process (seam for tests).
pub trait ServerProcess: Send {
    /// `Ok(true)` when the process has already exited.
    fn has_exited(&mut self) -> Result<bool, AppError>;
    /// Best-effort kill + reap.
    fn kill(&mut self) -> Result<(), AppError>;
}

/// Spawn seam so tests run without a real binary.
pub trait ServerSpawner: Send + Sync {
    fn spawn(&self, spec: &ServerSpec, args: &[String])
        -> Result<Box<dyn ServerProcess>, AppError>;
}

/// Health seam (`GET /health` == 200).
pub trait HealthProbe: Send + Sync {
    fn healthy(&self, port: u16) -> bool;
}

struct EngineInner {
    port: u16,
    state: EngineState,
    process: Option<Box<dyn ServerProcess>>,
    api_key: String,
    restarts: u8,
    /// Monotomic start generation: bumped on every spawn attempt so stale
    /// stops (detached drains from an earlier reset) can be detected.
    generation: u64,
    /// The held loopback listener guarding `port` until the child spawns.
    port_listener: Option<std::net::TcpListener>,
    /// Settings snapshot taken at the successful spawn: a Ready engine always
    /// serves the config it spawned with until it restarts (aifixes1 F2).
    spawned_settings: EngineSettings,
}

/// The managed Bango AI engine (registered as `Arc<BangoAiEngine>`).
pub struct BangoAiEngine {
    inner: Mutex<EngineInner>,
    settings: Mutex<EngineSettings>,
    /// Serializes cold starts so concurrent callers share one startup
    /// operation (single-flight, plan section 9) instead of double-spawning.
    startup_lock: AsyncMutex<()>,
    in_flight: AtomicUsize,
    spawner: Arc<dyn ServerSpawner>,
    probe: Arc<dyn HealthProbe>,
    reserver: Arc<dyn LoopbackReserver>,
}

impl Default for BangoAiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BangoAiEngine {
    /// Production engine: system spawner + TCP health probe + reserved port.
    #[must_use]
    pub fn new() -> Self {
        Self::with_seams(Arc::new(SystemSpawner), Arc::new(TcpHealthProbe))
    }

    /// Test seam constructor.
    #[must_use]
    pub fn with_seams(spawner: Arc<dyn ServerSpawner>, probe: Arc<dyn HealthProbe>) -> Self {
        Self::with_seams_and_reserver(spawner, probe, Arc::new(SystemReserver))
    }

    /// Test seam constructor with an injectable loopback reserver (aifixes1 F4).
    #[must_use]
    pub fn with_seams_and_reserver(
        spawner: Arc<dyn ServerSpawner>,
        probe: Arc<dyn HealthProbe>,
        reserver: Arc<dyn LoopbackReserver>,
    ) -> Self {
        let reserved = reserver.reserve().ok();
        let port = reserved.as_ref().map_or(0, |(port, _)| *port);
        let listener = reserved.map(|(_, listener)| listener);
        if port != 0 {
            publish_reserved_port(port);
        }
        Self {
            inner: Mutex::new(EngineInner {
                port,
                state: EngineState::Stopped,
                process: None,
                api_key: String::new(),
                restarts: 0,
                generation: 0,
                port_listener: listener,
                spawned_settings: EngineSettings::default(),
            }),
            settings: Mutex::new(EngineSettings::default()),
            startup_lock: AsyncMutex::new(()),
            in_flight: AtomicUsize::new(0),
            spawner,
            probe,
            reserver,
        }
    }

    /// The reserved loopback port (stable until a spawn retry re-reserves).
    pub fn port(&self) -> Result<u16, AppError> {
        Ok(lock_state(&self.inner)?.port)
    }

    /// Current engine state.
    pub fn state(&self) -> Result<EngineState, AppError> {
        Ok(lock_state(&self.inner)?.state.clone())
    }

    /// Whether the engine is starting or has requests in flight.
    pub fn is_busy(&self) -> Result<bool, AppError> {
        let starting = matches!(lock_state(&self.inner)?.state, EngineState::Starting);
        Ok(starting || self.in_flight.load(Ordering::Relaxed) > 0)
    }

    /// In-flight request accounting (used by the status payload).
    pub fn note_request_start(&self) {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    /// In-flight request accounting; saturates at zero.
    pub fn note_request_end(&self) {
        let _ = self
            .in_flight
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(1)));
    }

    /// Replace engine settings (context/threads/reasoning). Callers stop the
    /// engine when the process-visible settings changed.
    pub fn update_settings(&self, settings: EngineSettings) -> Result<(), AppError> {
        *lock_state(&self.settings)? = settings;
        Ok(())
    }

    /// Current engine settings.
    pub fn settings(&self) -> Result<EngineSettings, AppError> {
        Ok(lock_state(&self.settings)?.clone())
    }

    /// Build the effective config from borrowed state (private by design:
    /// aifixes1 D2 removed the lock-taking public variant; every caller
    /// builds it while already holding the guards it needs).
    fn config_from(inner: &EngineInner, settings: &EngineSettings) -> EngineConfig {
        EngineConfig {
            endpoint: format!("http://127.0.0.1:{}/v1", inner.port),
            api_key: inner.api_key.clone(),
            model: LOCAL_LLM_PROFILE_ID.to_string(),
            context: settings.context,
        }
    }

    /// Ensure the server is running and return the effective config.
    pub async fn ensure_started(&self, spec: &ServerSpec) -> Result<EngineConfig, AppError> {
        // Single-flight: a second cold caller waits here and then takes the
        // Ready fast-path below instead of spawning a second server.
        let _startup = self.startup_lock.lock().await;
        {
            let mut inner = lock_state(&self.inner)?;
            // Sticky failure (aifixes1 F5): after `Failed`, every start returns
            // the stored actionable error until an explicit stop/reset clears it.
            if let EngineState::Failed(message) = inner.state.clone() {
                return Err(AppError::Import(message));
            }
            if inner.state == EngineState::Ready {
                let alive = inner.process.as_mut().is_some_and(|p| !p.has_exited().unwrap_or(true));
                if alive {
                    // Guard held: build via `config_from`; `effective_config`
                    // would re-lock `inner` and self-deadlock. The spawned
                    // snapshot is authoritative while Ready (aifixes1 F2).
                    return Ok(Self::config_from(&inner, &inner.spawned_settings));
                }
                inner.process = None;
                inner.restarts = inner.restarts.saturating_add(1);
                if inner.restarts > MAX_RESTARTS {
                    let message = "Bango AI stopped unexpectedly. Test Bango AI again, or \
                                   Verify the installation."
                        .to_string();
                    inner.state = EngineState::Failed(message.clone());
                    return Err(AppError::Import(message));
                }
            }
            inner.state = EngineState::Starting;
            inner.api_key = new_api_key();
            inner.generation = inner.generation.saturating_add(1);
        }
        let settings = lock_state(&self.settings)?.clone();
        let mut last_err: Option<AppError> = None;

        for attempt in 0..PORT_RETRIES {
            let (port, args) = {
                let mut inner = lock_state(&self.inner)?;
                if attempt > 0 || inner.port == 0 {
                    // Re-reserve for this attempt (the previous listener was
                    // dropped before the failed spawn). A reserve failure
                    // leaves the engine Stopped - never a stranded Starting.
                    match self.reserver.reserve() {
                        Ok((new_port, listener)) => {
                            inner.port = new_port;
                            inner.port_listener = Some(listener);
                            publish_reserved_port(new_port);
                        }
                        Err(e) => {
                            inner.state = EngineState::Stopped;
                            inner.process = None;
                            return Err(e);
                        }
                    }
                }
                // Hand the port over: drop the held listener right before the
                // spawn (F4: the port is never unguarded before then).
                if let Some(listener) = inner.port_listener.take() {
                    drop(listener);
                }
                let args = build_args(spec, &inner.api_key, &settings, inner.port);
                (inner.port, args)
            };
            match self.spawner.spawn(spec, &args) {
                Ok(process) => {
                    lock_state(&self.inner)?.process = Some(process);
                    match self.wait_healthy(port).await {
                        Ok(()) => {
                            let mut inner = lock_state(&self.inner)?;
                            inner.state = EngineState::Ready;
                            inner.restarts = 0;
                            inner.spawned_settings = settings.clone();
                            // Guard held: `config_from` avoids the reentrant
                            // `effective_config` lock.
                            return Ok(Self::config_from(&inner, &settings));
                        }
                        Err(e) => {
                            // Start-attempt failure: the attempt budget governs
                            // retries; the crash budget (`restarts`) only counts
                            // a previously Ready server dying (aifixes1 F4).
                            let mut inner = lock_state(&self.inner)?;
                            if let Some(mut process) = inner.process.take() {
                                let _ = process.kill();
                            }
                            last_err = Some(e);
                        }
                    }
                }
                Err(e) => last_err = Some(e),
            }
        }

        let err = last_err
            .unwrap_or_else(|| AppError::Import("Bango AI could not be started.".to_string()));
        let mut inner = lock_state(&self.inner)?;
        let message = format!("{err} Check the log in Component Details.");
        inner.state = EngineState::Failed(message.clone());
        Err(AppError::Import(message))
    }

    /// Stop the server and return to `Stopped` (used by switch/remove).
    /// Takes the startup lock first so a stop never interleaves with a spawn
    /// (aifixes1 F5).
    pub async fn stop(&self) -> Result<(), AppError> {
        let _startup = self.startup_lock.lock().await;
        let process = {
            let mut inner = lock_state(&self.inner)?;
            inner.state = EngineState::Stopped;
            inner.restarts = 0;
            inner.process.take()
        };
        if let Some(mut process) = process {
            tokio::task::spawn_blocking(move || {
                let _ = process.kill();
            })
            .await
            .map_err(|e| AppError::Import(format!("engine stop task panicked: {e}")))?;
        }
        Ok(())
    }

    /// Monotonic start generation (diagnostics + stale-stop guards).
    pub fn generation(&self) -> Result<u64, AppError> {
        Ok(lock_state(&self.inner)?.generation)
    }

    /// Stop only when no newer start superseded `observed` (aifixes1 F5: a
    /// stale detached drain must never kill a freshly started server).
    pub async fn stop_if_unchanged(self: &Arc<Self>, observed: u64) -> Result<(), AppError> {
        if self.generation()? != observed {
            return Ok(());
        }
        self.stop().await
    }

    /// Reset for backend switch / settings change: wait briefly for in-flight
    /// requests, then stop the server. If a local generation is still running
    /// past the fast window, the stop continues on a detached task that waits
    /// for it to finish (bounded by the local request budget) - plan section
    /// 19: an in-flight local request finishes, and the UI never blocks on
    /// teardown.
    pub async fn reset_off_thread(self: &Arc<Self>) -> Result<(), AppError> {
        let deadline = Instant::now() + IN_FLIGHT_DRAIN_FAST;
        while self.in_flight.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
            tokio::time::sleep(IN_FLIGHT_DRAIN_POLL).await;
        }
        if self.in_flight.load(Ordering::Relaxed) == 0 {
            return self.stop().await;
        }
        let engine = Arc::clone(self);
        let observed = engine.generation().unwrap_or(0);
        tokio::spawn(async move {
            let deadline = Instant::now() + LOCAL_DRAIN_MAX;
            while engine.in_flight.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
                tokio::time::sleep(IN_FLIGHT_DRAIN_POLL).await;
            }
            // A newer start superseded this reset: leave the new server alone.
            let _ = engine.stop_if_unchanged(observed).await;
        });
        Ok(())
    }

    /// Synchronous best-effort stop for app-exit paths and `Drop`.
    pub fn kill_blocking(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(mut process) = inner.process.take() {
                let _ = process.kill();
            }
            inner.state = EngineState::Stopped;
            inner.restarts = 0;
        }
    }

    async fn wait_healthy(&self, port: u16) -> Result<(), AppError> {
        let deadline = Instant::now() + Duration::from_secs(START_TIMEOUT_SECS);
        loop {
            {
                let mut inner = lock_state(&self.inner)?;
                let alive = match inner.process.as_mut() {
                    Some(process) => !process.has_exited()?,
                    None => false,
                };
                if !alive {
                    return Err(AppError::Import(
                        "Bango AI server exited during startup.".to_string(),
                    ));
                }
            }
            let probe = self.probe.clone();
            let healthy = tokio::task::spawn_blocking(move || probe.healthy(port))
                .await
                .map_err(|e| AppError::Import(format!("health probe task panicked: {e}")))?;
            if healthy {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AppError::Import(format!(
                    "Bango AI did not become ready within {START_TIMEOUT_SECS} seconds."
                )));
            }
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    }
}

impl Drop for BangoAiEngine {
    fn drop(&mut self) {
        self.kill_blocking();
    }
}

/// Reserve a loopback port by binding `:0`; the listener is RETURNED and must
/// be held until just before the child spawns, so no other process can steal
/// the port in between (aifixes1 F4).
fn reserve_loopback() -> Result<(u16, std::net::TcpListener), AppError> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| AppError::Import(format!("could not reserve a loopback port: {e}")))?;
    let port = listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| AppError::Import(format!("could not read the reserved port: {e}")))?;
    Ok((port, listener))
}

/// Loopback reservation seam (tests inject failures; aifixes1 F4).
pub trait LoopbackReserver: Send + Sync {
    /// Reserve a port and return the held listener.
    fn reserve(&self) -> Result<(u16, std::net::TcpListener), AppError>;
}

/// Production reserver: real loopback bind.
pub struct SystemReserver;

impl LoopbackReserver for SystemReserver {
    fn reserve(&self) -> Result<(u16, std::net::TcpListener), AppError> {
        reserve_loopback()
    }
}

/// Per-start random API key so only Bango can drive the loopback server.
fn new_api_key() -> String {
    format!("bango-ai-{}", uuid::Uuid::new_v4().simple())
}

/// Server command line (validated against `b10964` in T4).
fn build_args(
    spec: &ServerSpec,
    api_key: &str,
    settings: &EngineSettings,
    port: u16,
) -> Vec<String> {
    vec![
        "--model".to_string(),
        spec.model.display().to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--ctx-size".to_string(),
        settings.context.to_string(),
        "--threads".to_string(),
        settings.threads.to_string(),
        "--threads-batch".to_string(),
        settings.threads.to_string(),
        "--no-webui".to_string(),
        "--jinja".to_string(),
        "--api-key".to_string(),
        api_key.to_string(),
        "--sleep-idle-seconds".to_string(),
        idle_sleep_secs().to_string(),
    ]
}

/// Production spawner: hidden console, stdout/stderr appended to the log.
pub struct SystemSpawner;
impl ServerSpawner for SystemSpawner {
    fn spawn(
        &self,
        spec: &ServerSpec,
        args: &[String],
    ) -> Result<Box<dyn ServerProcess>, AppError> {
        if let Some(parent) = spec.log.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(std::io::Error::other(format!("engine log dir: {e}"))))?;
        }
        let stdout =
            std::fs::OpenOptions::new().create(true).append(true).open(&spec.log).map_err(|e| {
                AppError::Io(std::io::Error::other(format!("engine log open: {e}")))
            })?;
        let stderr = stdout
            .try_clone()
            .map_err(|e| AppError::Io(std::io::Error::other(format!("engine log clone: {e}"))))?;
        let mut command = std::process::Command::new(&spec.binary);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command
            .spawn()
            .map_err(|e| AppError::Import(format!("could not start Bango AI: {e}")))?;
        Ok(Box::new(SystemProcess { child }))
    }
}

struct SystemProcess {
    child: std::process::Child,
}

impl ServerProcess for SystemProcess {
    fn has_exited(&mut self) -> Result<bool, AppError> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|e| AppError::Import(format!("could not poll Bango AI: {e}")))
    }

    fn kill(&mut self) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            // Process-tree kill (aifixes1 F13): `taskkill /T /F` takes down the
            // server plus any children; `Child::kill` is direct-descendant only.
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let _ = self.child.wait();
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
            Ok(())
        }
    }
}

/// Production health probe: raw HTTP `GET /health` over loopback.
pub struct TcpHealthProbe;

impl HealthProbe for TcpHealthProbe {
    fn healthy(&self, port: u16) -> bool {
        use std::io::{Read, Write};
        use std::net::{SocketAddr, TcpStream};
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        if stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            return false;
        }
        // Accumulate the status line until CRLF (a split first read of
        // "HTTP/1.1 2" + "00 OK" must not false-negative); accept any
        // `HTTP/*` version reporting 200 (aifixes1 F12).
        let mut head: Vec<u8> = Vec::with_capacity(64);
        let mut chunk = [0u8; 32];
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    head.extend_from_slice(&chunk[..n]);
                    if head.contains(&b'\n') || head.len() >= 64 {
                        break;
                    }
                }
                Err(_) => break,
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let text = String::from_utf8_lossy(&head);
        text.starts_with("HTTP/") && text.contains(" 200")
    }
}

/// The pinned model path inside a model root (used by the command layer).
#[must_use]
pub fn model_path(model_root: &Path) -> PathBuf {
    model_root
        .join(crate::llm::local::profile::LOCAL_LLM_PROFILE_DIR)
        .join(crate::llm::local::profile::LOCAL_LLM_MODEL_FILE)
}

/// Engine-backed `LocalConfigProvider`: starts the server on demand and
/// returns the live config (endpoint + per-start API key). Never falls back
/// to the configured provider.
pub struct EngineLocalConfigProvider {
    engine: Arc<BangoAiEngine>,
    spec: ServerSpec,
}

impl EngineLocalConfigProvider {
    #[must_use]
    pub fn new(engine: Arc<BangoAiEngine>, spec: ServerSpec) -> Self {
        Self { engine, spec }
    }
}

impl crate::llm::orchestrator::LocalConfigProvider for EngineLocalConfigProvider {
    fn effective_local_config<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<LlmConfig>, AppError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if crate::local_ai::manifest::current_target().is_none() {
                return Err(AppError::Validation(
                    "Bango AI is not available on this machine. Install components or switch \
                     back to the configured provider in Settings."
                        .to_string(),
                ));
            }
            let live = self.engine.ensure_started(&self.spec).await?;
            Ok(Some(LlmConfig {
                provider: LlmProvider::BangoAi,
                endpoint_url: live.endpoint,
                api_key_encrypted: Some(live.api_key),
                model_name: live.model,
                temperature: crate::llm::effective_config::LOCAL_TEMPERATURE,
                skip_temperature: false,
                max_concurrent_requests: 1,
                request_delay_ms: 0,
                context_window_tokens: live.context,
            }))
        })
    }

    fn reasoning_enabled(&self) -> bool {
        self.engine.settings().map(|settings| settings.reasoning).unwrap_or(false)
    }

    fn note_request_start(&self) {
        self.engine.note_request_start();
    }

    fn note_request_end(&self) {
        self.engine.note_request_end();
    }
}
