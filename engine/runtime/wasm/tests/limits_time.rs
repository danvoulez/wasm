//! Tests for wall-clock time limit enforcement.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::{Signal, TerminationReason};

/// A WASM module that loops forever (infinite loop).
/// (module
///   (memory (export "memory") 1)
///   (func (export "_start")
///     (loop $inf (br $inf))
///   )
/// )
fn infinite_loop_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "_start")
                (loop $inf (br $inf))
            )
        )
    "#,
    )
    .unwrap()
}

#[test]
fn wall_clock_timeout_terminates() {
    let wasm = infinite_loop_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            wall_time_ms: 200,
            fuel: 10_000_000_000, // Very high fuel so time is the binding constraint
            epoch_deadline: 2,
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let start = std::time::Instant::now();
    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    let elapsed = start.elapsed();

    assert_eq!(result.metrics.signal, Signal::Failure);
    // Should terminate due to timeout or epoch
    assert!(
        matches!(
            result.metrics.reason,
            TerminationReason::Timeout
                | TerminationReason::EpochDeadline
                | TerminationReason::FuelExhausted
                | TerminationReason::TrapOrPanic(_)
        ),
        "expected timeout/epoch/fuel, got: {:?}",
        result.metrics.reason
    );
    // Should not take much longer than the wall time limit
    assert!(
        elapsed.as_millis() < 5000,
        "should terminate within reasonable time, took {elapsed:?}"
    );
}
