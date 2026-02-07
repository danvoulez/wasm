//! Tests for the capability model: deny-by-default enforcement.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::Signal;

fn echo_wasm() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/echo.wasm")).unwrap()
}

#[test]
fn empty_caps_denies_stdin() {
    let wasm = echo_wasm();
    // No capabilities at all — stdin not granted
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"should not appear").unwrap();
    // The workload runs but stdin is not piped, so it reads empty
    // stdout should be empty since there's no stdin data and no /inputs access
    assert_eq!(result.metrics.signal, Signal::Success);
    assert!(
        result.stdout.is_empty(),
        "with no stdin cap, output should be empty, got: {:?}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[test]
fn with_stdin_cap_workload_gets_input() {
    let wasm = echo_wasm();
    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let input = b"hello sandbox";
    let result = SandboxDriver::execute(&wasm, &manifest, input).unwrap();
    assert_eq!(result.metrics.signal, Signal::Success);
    assert_eq!(
        result.stdout, input,
        "with stdin cap, echo should return the input"
    );
}

#[test]
fn fs_read_cap_allows_reading_inputs() {
    let wasm = echo_wasm();
    let inputs_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/inputs");

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: Some(inputs_dir.into()),
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    assert_eq!(result.metrics.signal, Signal::Success);
    // Should contain the contents of hello.txt
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Hello from the sandbox!"),
        "should read from /inputs, got: {output}"
    );
}

#[test]
fn no_fs_cap_blocks_inputs() {
    let wasm = echo_wasm();
    let inputs_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/inputs");

    // Only stdin+stdout, no fs.read
    let mut caps = CapabilitySet::empty();
    caps.grant(wasm_sandbox::caps::Capability::Stdin);
    caps.grant(wasm_sandbox::caps::Capability::Stdout);
    caps.grant(wasm_sandbox::caps::Capability::Stderr);

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: caps,
        inputs_dir: Some(inputs_dir.into()),
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    // Without fs.read cap, the /inputs directory won't be mounted
    // so the workload either fails or gets empty results
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        !output.contains("Hello from the sandbox!"),
        "without fs.read cap, should NOT read /inputs"
    );
}

#[test]
fn capability_set_operations() {
    let empty = CapabilitySet::empty();
    assert!(empty.is_empty());
    assert!(!empty.can_stdin());
    assert!(!empty.can_stdout());
    assert!(!empty.can_read_fs());
    assert!(!empty.can_tmp());

    let standard = CapabilitySet::standard_wasi();
    assert!(!standard.is_empty());
    assert!(standard.can_stdin());
    assert!(standard.can_stdout());
    assert!(standard.can_stderr());
    assert!(standard.can_read_fs());
    assert!(standard.can_tmp());
}
