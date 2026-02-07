//! Resource limits for sandbox execution.
//!
//! All limits are hard — exceeding any of them terminates the workload immediately.

use serde::{Deserialize, Serialize};

/// Hard resource limits applied to every sandbox execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum fuel (instruction budget). 0 = unlimited (not recommended).
    #[serde(default = "default_fuel")]
    pub fuel: u64,

    /// Epoch deadline — number of epoch ticks before preemption.
    #[serde(default = "default_epoch_deadline")]
    pub epoch_deadline: u64,

    /// Maximum wall-clock time in milliseconds.
    #[serde(default = "default_wall_time_ms")]
    pub wall_time_ms: u64,

    /// Maximum linear memory in bytes.
    #[serde(default = "default_max_memory_bytes")]
    pub max_memory_bytes: usize,

    /// Maximum stack size in bytes.
    #[serde(default = "default_max_stack_bytes")]
    pub max_stack_bytes: usize,

    /// Maximum scratch (tmp) filesystem usage in bytes.
    #[serde(default = "default_scratch_quota_bytes")]
    pub scratch_quota_bytes: u64,

    /// Maximum number of simultaneously open file descriptors.
    #[serde(default = "default_max_open_files")]
    pub max_open_files: u32,

    /// Maximum bytes writable to stdout.
    #[serde(default = "default_max_stdout_bytes")]
    pub max_stdout_bytes: usize,

    /// Maximum bytes writable to stderr.
    #[serde(default = "default_max_stderr_bytes")]
    pub max_stderr_bytes: usize,
}

fn default_fuel() -> u64 {
    100_000_000
}
fn default_epoch_deadline() -> u64 {
    100
}
fn default_wall_time_ms() -> u64 {
    5_000
}
fn default_max_memory_bytes() -> usize {
    64 * 1024 * 1024 // 64 MB
}
fn default_max_stack_bytes() -> usize {
    1024 * 1024 // 1 MB
}
fn default_scratch_quota_bytes() -> u64 {
    16 * 1024 * 1024 // 16 MB
}
fn default_max_open_files() -> u32 {
    32
}
fn default_max_stdout_bytes() -> usize {
    4 * 1024 * 1024 // 4 MB
}
fn default_max_stderr_bytes() -> usize {
    1024 * 1024 // 1 MB
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            fuel: default_fuel(),
            epoch_deadline: default_epoch_deadline(),
            wall_time_ms: default_wall_time_ms(),
            max_memory_bytes: default_max_memory_bytes(),
            max_stack_bytes: default_max_stack_bytes(),
            scratch_quota_bytes: default_scratch_quota_bytes(),
            max_open_files: default_max_open_files(),
            max_stdout_bytes: default_max_stdout_bytes(),
            max_stderr_bytes: default_max_stderr_bytes(),
        }
    }
}

impl ResourceLimits {
    /// Validate limits are sane.
    pub fn validate(&self) -> Result<(), LimitsError> {
        if self.fuel == 0 {
            return Err(LimitsError::InvalidLimit("fuel must be > 0".into()));
        }
        if self.max_memory_bytes == 0 {
            return Err(LimitsError::InvalidLimit("max_memory_bytes must be > 0".into()));
        }
        if self.wall_time_ms == 0 {
            return Err(LimitsError::InvalidLimit("wall_time_ms must be > 0".into()));
        }
        Ok(())
    }
}

/// Errors related to resource limits.
#[derive(Debug, thiserror::Error)]
pub enum LimitsError {
    #[error("invalid limit: {0}")]
    InvalidLimit(String),

    #[error("fuel exhausted")]
    FuelExhausted,

    #[error("epoch deadline exceeded")]
    EpochDeadline,

    #[error("wall-clock timeout ({0}ms)")]
    WallTimeout(u64),

    #[error("memory limit exceeded ({0} bytes)")]
    MemoryExceeded(usize),

    #[error("stdout cap exceeded ({0} bytes)")]
    StdoutCapExceeded(usize),

    #[error("scratch quota exceeded")]
    ScratchQuotaExceeded,

    #[error("too many open files (limit: {0})")]
    TooManyOpenFiles(u32),
}
