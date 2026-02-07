//! Determinism controls — configure the wasmtime engine for bit-reproducible execution.
//!
//! This module sets all the flags needed to guarantee that the same WASM module
//! with the same inputs produces the exact same output, byte for byte.

use wasmtime::Config;

/// Fixed epoch used when a workload asks for "current time".
pub const FIXED_EPOCH_SECS: i64 = 0; // 1970-01-01T00:00:00Z
pub const FIXED_LOCALE: &str = "en_US.UTF-8";
pub const FIXED_TZ: &str = "UTC";

/// Apply determinism-critical settings to a wasmtime Config.
pub fn configure_deterministic(config: &mut Config) {
    // NaN canonicalization: ensure all NaN values have a single canonical bit pattern.
    config.cranelift_nan_canonicalization(true);

    // Disable parallel compilation for determinism (compilation order matters).
    config.parallel_compilation(false);

    // Enable fuel metering (required for preemption).
    config.consume_fuel(true);

    // Enable epoch interruption (secondary preemption mechanism).
    config.epoch_interruption(true);

    // Disable features that break determinism.
    config.wasm_threads(false);
    config.wasm_simd(true); // SIMD is fine with NaN canonicalization
    config.wasm_relaxed_simd(false); // Relaxed SIMD is non-deterministic

    // Memory configuration.
    config.memory_reservation(0);
    config.memory_guard_size(0x10000); // 64KB guard
}

/// Environment variables to set for deterministic execution.
pub fn deterministic_env_vars() -> Vec<(String, String)> {
    vec![
        ("LC_ALL".into(), FIXED_LOCALE.into()),
        ("LANG".into(), FIXED_LOCALE.into()),
        ("TZ".into(), FIXED_TZ.into()),
    ]
}
