//! Tests for fuel limit enforcement.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::{Signal, TerminationReason};

fn echo_wasm() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/echo.wasm")).unwrap()
}

#[test]
fn fuel_exhaustion_terminates_workload() {
    let wasm = echo_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            fuel: 100, // Extremely low — will exhaust before completing
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"hello").unwrap();
    assert_eq!(result.metrics.signal, Signal::Failure);
    assert!(
        matches!(result.metrics.reason, TerminationReason::FuelExhausted | TerminationReason::TrapOrPanic(_)),
        "expected fuel exhaustion or trap, got: {:?}",
        result.metrics.reason
    );
}

#[test]
fn sufficient_fuel_succeeds() {
    let wasm = echo_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            fuel: 100_000_000,
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"hello").unwrap();
    assert_eq!(result.metrics.signal, Signal::Success);
    assert!(result.metrics.fuel_consumed > 0, "should consume some fuel");
    assert!(
        result.metrics.fuel_consumed < 100_000_000,
        "should not consume all fuel"
    );
}

#[test]
fn fuel_metrics_recorded() {
    let wasm = echo_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"test").unwrap();
    assert_eq!(result.metrics.signal, Signal::Success);
    assert!(result.metrics.fuel_consumed > 0);
    assert_eq!(result.metrics.fuel_limit, 100_000_000);
}
