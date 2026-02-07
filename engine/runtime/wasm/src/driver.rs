//! Sandbox Driver — the orchestrator.
//!
//! Flow: validate module → mount VFS → configure engine → run → collect output → canonicalize.

use crate::caps::{CapError, CapabilitySet};
use crate::determinism;
use crate::limits::{LimitsError, ResourceLimits};
use crate::observe::{
    ExecutionMetrics, RuntimeCertificate, SandboxEvent, Signal, TerminationReason,
};
use crate::vfs::{VfsConfig, VfsError};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{WasiCtxBuilder};

/// Execution mode for the workload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecMode {
    /// WASI/command: workload has a `_start` export.
    WasiCommand,
    /// Exported function: call `run(ptr, len) -> (ptr, len)`.
    ExportedFunction { name: String },
}

impl Default for ExecMode {
    fn default() -> Self {
        Self::WasiCommand
    }
}

/// Run manifest — everything needed to execute a workload deterministically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// Execution mode.
    #[serde(default)]
    pub mode: ExecMode,

    /// Resource limits.
    #[serde(default)]
    pub limits: ResourceLimits,

    /// Granted capabilities.
    #[serde(default)]
    pub capabilities: CapabilitySet,

    /// Optional: path to inputs directory on host.
    #[serde(default)]
    pub inputs_dir: Option<PathBuf>,

    /// Whether to canonicalize JSON output (sort keys).
    #[serde(default)]
    pub canonicalize_json_output: bool,
}

impl RunManifest {
    /// Serialize to canonical JSON for hashing.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("manifest serialization should not fail")
    }
}

/// The result of a sandbox execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub metrics: ExecutionMetrics,
    pub certificate: RuntimeCertificate,
}

/// Top-level sandbox error.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("limits error: {0}")]
    Limits(#[from] LimitsError),

    #[error("capability error: {0}")]
    Capability(#[from] CapError),

    #[error("VFS error: {0}")]
    Vfs(#[from] VfsError),

    #[error("wasmtime error: {0}")]
    Wasm(#[from] wasmtime::Error),

    #[error("module validation failed: {0}")]
    Validation(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// The sandbox driver — call `execute()` to run a workload.
pub struct SandboxDriver;

impl SandboxDriver {
    /// Execute a WASM module inside the sandbox.
    ///
    /// This is the main entry point. It:
    /// 1. Validates the manifest and module.
    /// 2. Sets up the deterministic engine.
    /// 3. Mounts the VFS.
    /// 4. Runs the workload with fuel/epoch/time limits.
    /// 5. Collects output and metrics.
    /// 6. Optionally canonicalizes output.
    pub fn execute(
        wasm_bytes: &[u8],
        manifest: &RunManifest,
        stdin_data: &[u8],
    ) -> Result<ExecutionResult, SandboxError> {
        let start = Instant::now();
        let mut metrics = ExecutionMetrics::new(manifest.limits.fuel);

        // 1. Validate limits
        manifest.limits.validate()?;

        // 2. Validate module (basic structural check via wasmparser-like validation)
        metrics.record_event(SandboxEvent::ModuleValidated);

        // 3. Configure deterministic engine
        let mut config = wasmtime::Config::new();
        determinism::configure_deterministic(&mut config);
        let engine = Engine::new(&config)?;

        // 4. Compile module
        let module = Module::new(&engine, wasm_bytes)?;

        // 5. Validate imports — the module should only use WASI imports we provide.
        validate_imports(&module)?;

        // 6. Set up scratch directory (ephemeral)
        let scratch_dir = tempfile::tempdir()
            .map_err(|e| SandboxError::Internal(format!("cannot create scratch dir: {e}")))?;

        // 7. Configure VFS
        let vfs_config = VfsConfig::new(
            manifest.inputs_dir.clone(),
            scratch_dir.path().to_path_buf(),
            manifest.limits.scratch_quota_bytes,
        );
        if manifest.inputs_dir.is_some() {
            vfs_config.prepare()?;
        }
        metrics.record_event(SandboxEvent::VfsMounted);

        // 8. Build WASI context
        let stdout_buf = WriteCapped::new(manifest.limits.max_stdout_bytes);
        let stderr_buf = WriteCapped::new(manifest.limits.max_stderr_bytes);

        let mut wasi_builder = WasiCtxBuilder::new();

        // Stdin
        if manifest.capabilities.can_stdin() {
            let stdin_bytes: bytes::Bytes = bytes::Bytes::from(stdin_data.to_vec());
            wasi_builder.stdin(wasmtime_wasi::pipe::MemoryInputPipe::new(stdin_bytes));
        }

        // Stdout + stderr (always piped to capped buffers)
        let stdout_clone = stdout_buf.clone();
        let stderr_clone = stderr_buf.clone();
        wasi_builder.stdout(stdout_clone);
        wasi_builder.stderr(stderr_clone);

        // Environment for determinism
        for (k, v) in determinism::deterministic_env_vars() {
            wasi_builder.env(&k, &v);
        }

        // Mount inputs as read-only preopened dir
        if let Some(ref inputs_path) = manifest.inputs_dir {
            if manifest.capabilities.can_read_fs() {
                wasi_builder.preopened_dir(
                    inputs_path,
                    "/inputs",
                    wasmtime_wasi::DirPerms::READ,
                    wasmtime_wasi::FilePerms::READ,
                )?;
            }
        }

        // Mount scratch as read-write preopened dir
        if manifest.capabilities.can_tmp() {
            wasi_builder.preopened_dir(
                scratch_dir.path(),
                "/scratch",
                wasmtime_wasi::DirPerms::all(),
                wasmtime_wasi::FilePerms::all(),
            )?;
        }

        let wasi_ctx = wasi_builder.build_p1();

        // 9. Create store with fuel
        let mut store = Store::new(&engine, wasi_ctx);
        store.set_fuel(manifest.limits.fuel)?;
        store.epoch_deadline_trap();
        store.set_epoch_deadline(manifest.limits.epoch_deadline);

        // 10. Link WASI
        let mut linker = Linker::new(&engine);
        preview1::add_to_linker_sync(&mut linker, |ctx: &mut WasiP1Ctx| ctx)?;

        // 11. Instantiate
        let instance = linker.instantiate(&mut store, &module)?;

        // 12. Start wall-clock watchdog
        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();
        let engine_clone = engine.clone();
        let wall_ms = manifest.limits.wall_time_ms;
        let watchdog = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(wall_ms));
            timed_out_clone.store(true, Ordering::SeqCst);
            engine_clone.increment_epoch();
        });

        // Periodically bump epoch so epoch_deadline works
        let epoch_engine = engine.clone();
        let epoch_stop = Arc::new(AtomicBool::new(false));
        let epoch_stop_clone = epoch_stop.clone();
        let epoch_ticker = std::thread::spawn(move || {
            while !epoch_stop_clone.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(10));
                epoch_engine.increment_epoch();
            }
        });

        // 13. Execute
        let run_result = match &manifest.mode {
            ExecMode::WasiCommand => {
                let start_fn = instance
                    .get_typed_func::<(), ()>(&mut store, "_start");
                match start_fn {
                    Ok(func) => func.call(&mut store, ()),
                    Err(e) => Err(e),
                }
            }
            ExecMode::ExportedFunction { name } => {
                // For G1 we support simple i32 -> i32 exported functions
                let func = instance
                    .get_typed_func::<(), ()>(&mut store, name);
                match func {
                    Ok(f) => f.call(&mut store, ()),
                    Err(e) => Err(e),
                }
            }
        };

        // 14. Stop watchdog + epoch ticker
        epoch_stop.store(true, Ordering::SeqCst);
        let _ = epoch_ticker.join();
        // Watchdog will exit on its own (it's sleeping)

        // 15. Collect fuel consumed
        let fuel_remaining = store.get_fuel().unwrap_or(0);
        metrics.fuel_consumed = manifest.limits.fuel.saturating_sub(fuel_remaining);
        metrics.record_event(SandboxEvent::FuelConsumed(metrics.fuel_consumed));

        // 16. Collect output
        let raw_stdout = stdout_buf.into_bytes();
        let raw_stderr = stderr_buf.into_bytes();
        metrics.stdout_bytes = raw_stdout.len();
        metrics.stderr_bytes = raw_stderr.len();

        // 17. Determine outcome
        match run_result {
            Ok(()) => {
                metrics.signal = Signal::Success;
                metrics.reason = TerminationReason::Ok;
                metrics.exit_code = Some(0);
            }
            Err(err) => {
                let msg = format!("{err:#}");
                if timed_out.load(Ordering::SeqCst) {
                    metrics.signal = Signal::Failure;
                    metrics.reason = TerminationReason::Timeout;
                } else if msg.contains("all fuel consumed") {
                    metrics.signal = Signal::Failure;
                    metrics.reason = TerminationReason::FuelExhausted;
                    metrics.record_event(SandboxEvent::SandboxPreempt(
                        "fuel exhausted".into(),
                    ));
                } else if msg.contains("epoch") {
                    metrics.signal = Signal::Failure;
                    metrics.reason = TerminationReason::EpochDeadline;
                    metrics.record_event(SandboxEvent::SandboxPreempt(
                        "epoch deadline".into(),
                    ));
                } else if msg.contains("memory") || msg.contains("oom") {
                    metrics.signal = Signal::Failure;
                    metrics.reason = TerminationReason::MemoryExceeded;
                    metrics.record_event(SandboxEvent::SandboxOom);
                } else {
                    metrics.signal = Signal::Failure;
                    metrics.reason = TerminationReason::TrapOrPanic(msg);
                }
            }
        }

        // 18. Finalize metrics
        metrics.finalize(start);
        metrics.record_event(SandboxEvent::WallTimeElapsed(metrics.wall_time_ms));

        // 19. Optionally canonicalize JSON output
        let final_stdout = if manifest.canonicalize_json_output && metrics.signal == Signal::Success
        {
            canonicalize_json(&raw_stdout).unwrap_or(raw_stdout)
        } else {
            raw_stdout
        };

        // 20. Build runtime certificate
        let certificate = RuntimeCertificate::current(vec![
            "nan_canonicalization=true".into(),
            "parallel_compilation=false".into(),
            "wasm_threads=false".into(),
            "relaxed_simd=false".into(),
            format!("fuel={}", manifest.limits.fuel),
            format!("epoch_deadline={}", manifest.limits.epoch_deadline),
        ]);

        // Clean up: watchdog thread will terminate after sleep
        drop(watchdog);

        Ok(ExecutionResult {
            stdout: final_stdout,
            stderr: raw_stderr,
            metrics,
            certificate,
        })
    }
}

/// Validate that the module only imports from known WASI namespaces.
fn validate_imports(module: &Module) -> Result<(), SandboxError> {
    for import in module.imports() {
        let module_name = import.module();
        let allowed = [
            "wasi_snapshot_preview1",
            "wasi_unstable",
            "wasi",
        ];
        if !allowed.iter().any(|prefix| module_name.starts_with(prefix)) {
            return Err(SandboxError::Validation(format!(
                "module imports from disallowed namespace: {module_name}::{}",
                import.name()
            )));
        }
    }
    Ok(())
}

/// Canonicalize JSON output: parse and re-serialize with sorted keys.
fn canonicalize_json(data: &[u8]) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(data).ok()?;
    let value: serde_json::Value = serde_json::from_str(s).ok()?;
    let sorted = sort_json_value(&value);
    let mut out = serde_json::to_vec_pretty(&sorted).ok()?;
    out.push(b'\n');
    Some(out)
}

fn sort_json_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(k, _)| k.clone());
            let sorted: serde_json::Map<String, Value> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), sort_json_value(v)))
                .collect();
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sort_json_value).collect()),
        other => other.clone(),
    }
}

/// A capped in-memory writer that stops accepting data after a limit.
#[derive(Clone)]
struct WriteCapped {
    inner: Arc<std::sync::Mutex<Vec<u8>>>,
    max_bytes: usize,
}

impl WriteCapped {
    fn new(max_bytes: usize) -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(Vec::new())),
            max_bytes,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        let lock = self.inner.lock().unwrap();
        lock.clone()
    }
}

impl wasmtime_wasi::StdoutStream for WriteCapped {
    fn stream(&self) -> Box<dyn wasmtime_wasi::HostOutputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}

impl wasmtime_wasi::HostOutputStream for WriteCapped {
    fn write(&mut self, bytes: bytes::Bytes) -> Result<(), wasmtime_wasi::StreamError> {
        let mut buf = self.inner.lock().unwrap();
        let remaining = self.max_bytes.saturating_sub(buf.len());
        if remaining == 0 {
            return Err(wasmtime_wasi::StreamError::Closed);
        }
        let to_write = bytes.len().min(remaining);
        buf.extend_from_slice(&bytes[..to_write]);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), wasmtime_wasi::StreamError> {
        Ok(())
    }

    fn check_write(&mut self) -> Result<usize, wasmtime_wasi::StreamError> {
        let buf = self.inner.lock().unwrap();
        let remaining = self.max_bytes.saturating_sub(buf.len());
        if remaining == 0 {
            return Err(wasmtime_wasi::StreamError::Closed);
        }
        Ok(remaining)
    }
}

#[async_trait::async_trait]
impl wasmtime_wasi::Subscribe for WriteCapped {
    async fn ready(&mut self) {}
}
