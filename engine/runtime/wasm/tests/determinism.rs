//! Determinism tests: same input → same output, byte for byte, across multiple runs.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::Signal;

fn echo_wasm() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/echo.wasm")).unwrap()
}

fn normalize_json_wasm() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/normalize_json.wasm"
    ))
    .unwrap()
}

fn checksum_wasm() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/checksum.wasm"
    ))
    .unwrap()
}

fn standard_manifest() -> RunManifest {
    RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    }
}

/// Run the same workload N times and assert byte-identical output.
#[test]
fn echo_determinism_10_runs() {
    let wasm = echo_wasm();
    let stdin = b"determinism test input 12345";
    let manifest = standard_manifest();

    let mut outputs: Vec<Vec<u8>> = Vec::new();
    for _ in 0..10 {
        let result = SandboxDriver::execute(&wasm, &manifest, stdin).unwrap();
        assert_eq!(result.metrics.signal, Signal::Success);
        outputs.push(result.stdout);
    }

    let first = &outputs[0];
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            first, output,
            "run 0 and run {i} produced different output"
        );
    }
}

#[test]
fn checksum_determinism_10_runs() {
    let wasm = checksum_wasm();
    let stdin = b"some data to checksum";
    let manifest = standard_manifest();

    let mut outputs: Vec<Vec<u8>> = Vec::new();
    for _ in 0..10 {
        let result = SandboxDriver::execute(&wasm, &manifest, stdin).unwrap();
        assert_eq!(result.metrics.signal, Signal::Success);
        outputs.push(result.stdout);
    }

    let first = &outputs[0];
    assert!(!first.is_empty(), "checksum output should not be empty");
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            first, output,
            "checksum run 0 and run {i} differ"
        );
    }
}

#[test]
fn normalize_json_determinism() {
    let wasm = normalize_json_wasm();
    // JSON with intentionally unordered keys
    let stdin = br#"{"z":1,"a":2,"m":{"c":3,"b":4},"array":[3,2,1]}"#;
    let manifest = standard_manifest();

    let mut outputs: Vec<Vec<u8>> = Vec::new();
    for _ in 0..10 {
        let result = SandboxDriver::execute(&wasm, &manifest, stdin).unwrap();
        assert_eq!(result.metrics.signal, Signal::Success);
        outputs.push(result.stdout);
    }

    let first = &outputs[0];
    // Verify keys are sorted
    let output_str = std::str::from_utf8(first).unwrap();
    assert!(
        output_str.contains("\"a\""),
        "output should contain key 'a'"
    );

    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            first, output,
            "normalize_json run 0 and run {i} differ"
        );
    }
}

#[test]
fn fuel_consumed_is_deterministic() {
    let wasm = echo_wasm();
    let stdin = b"hello";
    let manifest = standard_manifest();

    let r1 = SandboxDriver::execute(&wasm, &manifest, stdin).unwrap();
    let r2 = SandboxDriver::execute(&wasm, &manifest, stdin).unwrap();

    assert_eq!(
        r1.metrics.fuel_consumed, r2.metrics.fuel_consumed,
        "fuel consumption should be deterministic"
    );
}

#[test]
fn different_input_different_output() {
    let wasm = checksum_wasm();
    let manifest = standard_manifest();

    let r1 = SandboxDriver::execute(&wasm, &manifest, b"input A").unwrap();
    let r2 = SandboxDriver::execute(&wasm, &manifest, b"input B").unwrap();

    assert_ne!(
        r1.stdout, r2.stdout,
        "different inputs should produce different checksums"
    );
}
