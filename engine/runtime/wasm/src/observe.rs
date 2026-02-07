//! Observability — metrics, events, and runtime certificate.
//!
//! No user data is captured — only structural execution metadata.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Execution signal — the overall outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    Success,
    Failure,
    Indeterminate,
}

/// Reason for non-success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationReason {
    Ok,
    Timeout,
    FuelExhausted,
    EpochDeadline,
    MemoryExceeded,
    StdoutCapExceeded,
    ScratchQuotaExceeded,
    CapabilityDenied(String),
    TrapOrPanic(String),
    InvalidModule(String),
    InternalError(String),
}

/// Sandbox event for structured logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxEvent {
    SandboxStart,
    SandboxEnd,
    SandboxPreempt(String),
    SandboxOom,
    CapDenied(String),
    ModuleValidated,
    VfsMounted,
    FuelConsumed(u64),
    WallTimeElapsed(u64),
}

/// Metrics collected from a single sandbox execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub fuel_consumed: u64,
    pub fuel_limit: u64,
    pub peak_memory_bytes: usize,
    pub wall_time_ms: u64,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub exit_code: Option<i32>,
    pub signal: Signal,
    pub reason: TerminationReason,
    pub events: Vec<SandboxEvent>,
}

impl ExecutionMetrics {
    pub fn new(fuel_limit: u64) -> Self {
        Self {
            fuel_consumed: 0,
            fuel_limit,
            peak_memory_bytes: 0,
            wall_time_ms: 0,
            stdout_bytes: 0,
            stderr_bytes: 0,
            exit_code: None,
            signal: Signal::Indeterminate,
            reason: TerminationReason::Ok,
            events: vec![SandboxEvent::SandboxStart],
        }
    }

    pub fn record_event(&mut self, event: SandboxEvent) {
        self.events.push(event);
    }

    pub fn finalize(&mut self, start: Instant) {
        self.wall_time_ms = start.elapsed().as_millis() as u64;
        self.events.push(SandboxEvent::SandboxEnd);
    }
}

/// Runtime certificate — identifies the exact executor binary and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCertificate {
    /// BLAKE3 hash of the executor binary (self).
    pub runtime_hash: String,

    /// Wasmtime version string.
    pub wasmtime_version: String,

    /// Active configuration flags affecting determinism.
    pub flags: Vec<String>,
}

impl RuntimeCertificate {
    /// Generate a certificate for the current runtime.
    pub fn current(flags: Vec<String>) -> Self {
        // Hash of the current executable
        let exe_path = std::env::current_exe().unwrap_or_default();
        let runtime_hash = if let Ok(bytes) = std::fs::read(&exe_path) {
            format!("b3:{}", blake3::hash(&bytes).to_hex())
        } else {
            "b3:unknown".into()
        };

        Self {
            runtime_hash,
            wasmtime_version: format!("wasmtime-{}", env!("CARGO_PKG_VERSION")),
            flags,
        }
    }
}
