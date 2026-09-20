//! Lifecycle tests for `llm::local::engine` (`BangoAiEngine`).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` in
//! `src/llm/local/engine.rs` to keep the source file compact. The spawn,
//! health, and port-reservation seams (`ServerSpawner` / `HealthProbe` /
//! `LoopbackReserver`) let these run without a real `llama-server` binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bango_lib::error::AppError;
use bango_lib::llm::local::engine::{
    BangoAiEngine, EngineState, HealthProbe, LoopbackReserver, ServerProcess, ServerSpawner,
    ServerSpec, SystemReserver, TcpHealthProbe, PORT_RETRIES,
};
use bango_lib::llm::local::policy::EngineSettings;
use bango_lib::llm::local::profile::LOCAL_LLM_PROFILE_ID;

use std::sync::atomic::AtomicBool;

#[derive(Default)]
struct FakeProcess {
    exited: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    drain_flag: Option<Arc<AtomicBool>>,
    killed_after_drain: Arc<AtomicBool>,
}

impl ServerProcess for FakeProcess {
    fn has_exited(&mut self) -> Result<bool, AppError> {
        Ok(self.exited.load(Ordering::Relaxed))
    }

    fn kill(&mut self) -> Result<(), AppError> {
        if let Some(flag) = &self.drain_flag {
            self.killed_after_drain.store(flag.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        self.killed.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Default)]
struct FakeSpawner {
    calls: Mutex<Vec<Vec<String>>>,
    fail_first: AtomicUsize,
    exited: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    drain_flag: Option<Arc<AtomicBool>>,
    killed_after_drain: Arc<AtomicBool>,
}

impl ServerSpawner for FakeSpawner {
    fn spawn(
        &self,
        _spec: &ServerSpec,
        args: &[String],
    ) -> Result<Box<dyn ServerProcess>, AppError> {
        self.calls.lock().expect("calls lock").push(args.to_vec());
        let remaining = self.fail_first.load(Ordering::Relaxed);
        if remaining > 0 {
            self.fail_first.store(remaining - 1, Ordering::Relaxed);
            return Err(AppError::Import("spawn failed (simulated)".to_string()));
        }
        Ok(Box::new(FakeProcess {
            exited: self.exited.clone(),
            killed: self.killed.clone(),
            drain_flag: self.drain_flag.clone(),
            killed_after_drain: self.killed_after_drain.clone(),
        }))
    }
}

struct FakeProbe {
    healthy: bool,
}

impl HealthProbe for FakeProbe {
    fn healthy(&self, _port: u16) -> bool {
        self.healthy
    }
}

fn spec() -> ServerSpec {
    ServerSpec {
        binary: PathBuf::from("/nonexistent/llama-server"),
        model: PathBuf::from("/nonexistent/model.gguf"),
        log: PathBuf::from("/nonexistent/engine.log"),
    }
}

fn arg_port(args: &[String]) -> u16 {
    let index = args.iter().position(|a| a == "--port").expect("--port present");
    args[index + 1].parse().expect("port parses")
}

#[tokio::test]
async fn engine_starts_lazily_and_publishes_effective_endpoint() {
    let spawner = Arc::new(FakeSpawner::default());
    let engine = BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true }));
    assert_eq!(engine.state().expect("state"), EngineState::Stopped);
    assert!(!engine.is_busy().expect("busy"));

    let config = engine.ensure_started(&spec()).await.expect("starts");
    assert!(config.endpoint.starts_with("http://127.0.0.1:"));
    assert!(config.endpoint.ends_with("/v1"));
    assert!(config.api_key.starts_with("bango-ai-"));
    assert_eq!(config.model, LOCAL_LLM_PROFILE_ID);
    assert_eq!(engine.state().expect("state"), EngineState::Ready);
    assert_eq!(spawner.calls.lock().expect("calls").len(), 1);
}

#[tokio::test]
async fn engine_stop_releases_the_process() {
    let spawner = Arc::new(FakeSpawner::default());
    let engine = BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true }));
    engine.ensure_started(&spec()).await.expect("starts");
    engine.stop().await.expect("stops");
    assert_eq!(engine.state().expect("state"), EngineState::Stopped);
    assert!(spawner.killed.load(Ordering::Relaxed), "child must be killed");
}

#[tokio::test]
async fn engine_restarts_once_then_reports_failed() {
    let spawner = Arc::new(FakeSpawner::default());
    spawner.exited.store(true, Ordering::Relaxed);
    let engine = BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true }));
    let err = engine.ensure_started(&spec()).await.expect_err("must fail");
    // Start attempts are bounded by PORT_RETRIES; the crash budget only
    // counts a previously Ready server dying (aifixes1 F4).
    assert_eq!(spawner.calls.lock().expect("calls").len(), PORT_RETRIES, "attempt budget");
    assert!(err.to_string().contains("Check the log"), "got: {err}");
    assert!(matches!(engine.state().expect("state"), EngineState::Failed(_)));
}

#[tokio::test]
async fn failed_state_is_sticky_until_reset() {
    let spawner = Arc::new(FakeSpawner::default());
    spawner.exited.store(true, Ordering::Relaxed);
    let engine =
        Arc::new(BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true })));
    let first = engine.ensure_started(&spec()).await.expect_err("fails");
    let calls_after_first = spawner.calls.lock().expect("calls").len();
    assert!(calls_after_first >= 1);

    // Sticky: the stored error returns without new spawn attempts.
    let second = engine.ensure_started(&spec()).await.expect_err("sticky");
    assert_eq!(second.to_string(), first.to_string());
    assert_eq!(
        spawner.calls.lock().expect("calls").len(),
        calls_after_first,
        "no new spawns while Failed"
    );

    // Reset clears the failure; a healthy start succeeds.
    engine.reset_off_thread().await.expect("reset");
    spawner.exited.store(false, Ordering::Relaxed);
    engine.ensure_started(&spec()).await.expect("starts after reset");
    assert_eq!(engine.state().expect("state"), EngineState::Ready);
}

#[tokio::test]
async fn stale_stop_does_not_kill_new_generation() {
    let spawner = Arc::new(FakeSpawner::default());
    let engine =
        Arc::new(BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true })));
    engine.ensure_started(&spec()).await.expect("starts");
    let current = engine.generation().expect("generation");

    // A stale observed generation (older reset) must not kill the server.
    engine.stop_if_unchanged(current - 1).await.expect("no-op");
    assert!(!spawner.killed.load(Ordering::Relaxed), "stale stop is a no-op");

    // The matching generation stops as usual.
    engine.stop_if_unchanged(current).await.expect("stops");
    assert!(spawner.killed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn reset_then_restart_leaves_one_child() {
    let spawner = Arc::new(FakeSpawner::default());
    let engine =
        Arc::new(BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true })));
    engine.ensure_started(&spec()).await.expect("first start");
    engine.reset_off_thread().await.expect("reset");
    assert_eq!(engine.state().expect("state"), EngineState::Stopped);
    engine.ensure_started(&spec()).await.expect("second start");
    assert_eq!(engine.state().expect("state"), EngineState::Ready);
    assert_eq!(spawner.calls.lock().expect("calls").len(), 2, "exactly two spawns");
    assert!(engine.generation().expect("generation") >= 2);
}

#[tokio::test]
async fn engine_reset_is_off_thread_and_waits_for_in_flight() {
    let drained = Arc::new(AtomicBool::new(false));
    let spawner = Arc::new(FakeSpawner { drain_flag: Some(drained.clone()), ..Default::default() });
    let engine =
        Arc::new(BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true })));
    engine.ensure_started(&spec()).await.expect("starts");
    engine.note_request_start();
    let engine_ref = engine.clone();
    let drained_ref = drained.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        drained_ref.store(true, Ordering::Relaxed);
        engine_ref.note_request_end();
    });
    engine.reset_off_thread().await.expect("resets");
    assert_eq!(engine.state().expect("state"), EngineState::Stopped);
    assert!(
        spawner.killed_after_drain.load(Ordering::Relaxed),
        "reset must wait for in-flight requests before killing"
    );
}

#[tokio::test]
async fn port_selection_retries_a_lost_race() {
    let spawner = Arc::new(FakeSpawner::default());
    spawner.fail_first.store(2, Ordering::Relaxed);
    let engine = BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true }));
    let config = engine.ensure_started(&spec()).await.expect("starts after retries");
    assert_eq!(engine.state().expect("state"), EngineState::Ready);
    assert_eq!(spawner.calls.lock().expect("calls").len(), 3, "two failures + success");
    let port = engine.port().expect("port");
    assert!(config.endpoint.contains(&format!(":{port}/")));
}

#[tokio::test]
async fn reserved_port_is_stable_before_start_and_rereleased_on_retry() {
    let spawner = Arc::new(FakeSpawner::default());
    spawner.fail_first.store(1, Ordering::Relaxed);
    let engine = BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true }));
    let initial = engine.port().expect("port");
    assert!(initial > 0, "a port is reserved before any start");

    // The reservation is REAL: the held listener keeps the port bound, so
    // a second bind must fail until the spawn hands the port over (F4).
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", initial)).is_err(),
        "reserved port must be held by the engine's listener"
    );

    engine.ensure_started(&spec()).await.expect("starts");
    let calls = spawner.calls.lock().expect("calls");
    assert_eq!(calls.len(), 2, "failed attempt + success");
    assert_eq!(arg_port(&calls[0]), initial, "first attempt uses the reserved port");
    assert_eq!(
        arg_port(&calls[1]),
        engine.port().expect("port"),
        "the retry spawns with the engine's current (re-reserved) port"
    );
    assert_ne!(arg_port(&calls[1]), initial, "the retry re-reserved a fresh port");
    // NOTE: the RESERVED_PORT static publish-on-re-reserve is not asserted
    // here - it is a process-global and parallel engine tests republish it.
}

#[tokio::test]
async fn reserve_failure_resets_starting_state() {
    /// Succeeds once (construction), fails every re-reserve (aifixes1 F4).
    struct FailReReserve {
        first: std::sync::atomic::AtomicBool,
    }
    impl LoopbackReserver for FailReReserve {
        fn reserve(&self) -> Result<(u16, std::net::TcpListener), AppError> {
            if self.first.swap(false, Ordering::AcqRel) {
                return SystemReserver.reserve();
            }
            Err(AppError::Import("reservation failed (simulated)".to_string()))
        }
    }

    let spawner = Arc::new(FakeSpawner::default());
    spawner.fail_first.store(1, Ordering::Relaxed);
    let engine = BangoAiEngine::with_seams_and_reserver(
        spawner.clone(),
        Arc::new(FakeProbe { healthy: true }),
        Arc::new(FailReReserve { first: std::sync::atomic::AtomicBool::new(true) }),
    );
    let err = engine.ensure_started(&spec()).await.expect_err("reserve failure surfaces");
    assert!(err.to_string().contains("reservation failed"), "got: {err}");
    assert_eq!(
        engine.state().expect("state"),
        EngineState::Stopped,
        "a failed re-reserve must never strand Starting"
    );
    assert_eq!(spawner.calls.lock().expect("calls").len(), 1, "no retry after reserve failure");
}

#[tokio::test]
async fn ready_engine_serves_spawned_settings_until_restart() {
    let spawner = Arc::new(FakeSpawner::default());
    let engine =
        Arc::new(BangoAiEngine::with_seams(spawner.clone(), Arc::new(FakeProbe { healthy: true })));
    let first = engine.ensure_started(&spec()).await.expect("starts");
    let spawned_context = first.context;

    // A settings change while Ready must not leak into the live config.
    engine
        .update_settings(EngineSettings { context: 8_192, threads: 2, reasoning: false })
        .expect("update");
    let still = engine.ensure_started(&spec()).await.expect("fast path");
    assert_eq!(still.context, spawned_context, "Ready serves the spawned snapshot");

    // A restart applies the new settings.
    engine.reset_off_thread().await.expect("reset");
    let restarted = engine.ensure_started(&spec()).await.expect("restart");
    assert_eq!(restarted.context, 8_192, "restart spawns with current settings");
}

#[tokio::test]
async fn busy_engine_reports_in_flight_and_queued_state() {
    // F12: the probe tolerates a split status-line read and any HTTP/* 200.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        if let Ok((mut conn, _)) = listener.accept() {
            use std::io::{Read, Write};
            let mut scratch = [0u8; 128];
            let _ = conn.read(&mut scratch);
            let _ = conn.write_all(b"HTTP/1.1 2");
            let _ = conn.flush();
            std::thread::sleep(Duration::from_millis(40));
            let _ = conn.write_all(b"00 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });
    assert!(
        TcpHealthProbe.healthy(port),
        "split status line with HTTP/1.1 200 must read as healthy"
    );
    let engine = BangoAiEngine::with_seams(
        Arc::new(FakeSpawner::default()),
        Arc::new(FakeProbe { healthy: true }),
    );
    assert!(!engine.is_busy().expect("busy"));
    engine.note_request_start();
    assert!(engine.is_busy().expect("busy"));
    engine.note_request_end();
    assert!(!engine.is_busy().expect("busy"));
    // Saturating end never underflows.
    engine.note_request_end();
    assert!(!engine.is_busy().expect("busy"));
}
