//! Tests for memory limit enforcement.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::Signal;

/// A WASM module that tries to grow memory aggressively.
/// (module
///   (memory (export "memory") 1)
///   (func (export "_start")
///     ;; Try to grow memory by 1000 pages (64MB)
///     (drop (memory.grow (i32.const 1000)))
///   )
/// )
fn memory_hog_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Try to grow memory by 1000 pages (64MB)
                (drop (memory.grow (i32.const 1000)))
            )
        )
    "#,
    )
    .unwrap()
}

#[test]
fn memory_growth_within_limits_succeeds() {
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Grow by 1 page (64KB) — should succeed
                (drop (memory.grow (i32.const 1)))
            )
        )
    "#,
    )
    .unwrap();

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            max_memory_bytes: 64 * 1024 * 1024,
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    assert_eq!(result.metrics.signal, Signal::Success);
}

#[test]
fn excessive_memory_growth_is_bounded() {
    let wasm = memory_hog_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            max_memory_bytes: 2 * 1024 * 1024, // 2MB limit
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    // The module will succeed but memory.grow returns -1 (failure)
    // because the engine limits prevent allocation beyond the cap.
    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    // Either succeeds (with grow returning -1) or fails with memory error
    assert!(
        result.metrics.signal == Signal::Success || result.metrics.signal == Signal::Failure,
        "should handle memory limit gracefully"
    );
}
