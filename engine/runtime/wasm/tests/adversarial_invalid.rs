//! Adversarial tests: malformed modules, resource abuse, path traversal.

use wasm_sandbox::caps::CapabilitySet;
use wasm_sandbox::driver::{ExecMode, RunManifest, SandboxDriver};
use wasm_sandbox::limits::ResourceLimits;
use wasm_sandbox::observe::Signal;
use wasm_sandbox::vfs::VfsConfig;

fn standard_manifest() -> RunManifest {
    RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits::default(),
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    }
}

#[test]
fn invalid_wasm_bytes_rejected() {
    let garbage = b"this is not a valid wasm module at all";
    let manifest = standard_manifest();

    let result = SandboxDriver::execute(garbage, &manifest, b"");
    assert!(result.is_err(), "invalid WASM should be rejected");
}

#[test]
fn empty_wasm_rejected() {
    let manifest = standard_manifest();
    let result = SandboxDriver::execute(&[], &manifest, b"");
    assert!(result.is_err(), "empty WASM should be rejected");
}

#[test]
fn module_with_disallowed_imports_rejected() {
    // Module that imports from a non-WASI namespace
    let wasm = wat::parse_str(
        r#"
        (module
            (import "env" "some_host_func" (func))
            (memory (export "memory") 1)
            (func (export "_start") (call 0))
        )
    "#,
    )
    .unwrap();

    let manifest = standard_manifest();
    let result = SandboxDriver::execute(&wasm, &manifest, b"");
    assert!(
        result.is_err(),
        "module with non-WASI imports should be rejected"
    );
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("disallowed") || msg.contains("Validation") || msg.contains("validation"),
        "error should mention disallowed import, got: {msg}"
    );
}

#[test]
fn deep_recursion_trapped() {
    // Module with deep recursion — should hit stack or fuel limit
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func $recurse
                (call $recurse)
            )
            (func (export "_start")
                (call $recurse)
            )
        )
    "#,
    )
    .unwrap();

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            fuel: 1_000_000,
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    assert_eq!(
        result.metrics.signal,
        Signal::Failure,
        "deep recursion should be trapped"
    );
}

#[test]
fn path_traversal_blocked() {
    let base = std::path::Path::new("/tmp/sandbox_test_base");
    std::fs::create_dir_all(base).unwrap();
    std::fs::write(base.join("legit.txt"), "ok").unwrap();

    // Try to escape
    let evil = std::path::Path::new("../../etc/passwd");
    let result = VfsConfig::validate_path(base, evil);
    assert!(result.is_err(), "path traversal should be blocked");
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("traversal"),
        "error should mention traversal, got: {msg}"
    );

    // Clean up
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn stdout_cap_enforced() {
    // Module that writes a lot to stdout
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            ;; iov at offset 1024: ptr=0, len=128
            (data (i32.const 1024) "\00\00\00\00\80\00\00\00")
            (func (export "_start")
                (local $i i32)
                (local.set $i (i32.const 0))
                (block $done
                    (loop $loop
                        ;; Write 128 bytes to stdout (fd=1)
                        (drop (call $fd_write
                            (i32.const 1)    ;; fd = stdout
                            (i32.const 1024) ;; iovs pointer
                            (i32.const 1)    ;; iovs_len
                            (i32.const 2048) ;; nwritten pointer
                        ))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $done (i32.ge_u (local.get $i) (i32.const 100)))
                        (br $loop)
                    )
                )
            )
        )
    "#,
    )
    .unwrap();

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            max_stdout_bytes: 1024, // Only 1KB allowed
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::standard_wasi(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    // Output should be capped at max_stdout_bytes
    assert!(
        result.stdout.len() <= 1024,
        "stdout should be capped at 1024 bytes, got {}",
        result.stdout.len()
    );
}

#[test]
fn zero_fuel_rejected_by_validation() {
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
    )
    .unwrap();

    let manifest = RunManifest {
        mode: ExecMode::WasiCommand,
        limits: ResourceLimits {
            fuel: 0,
            ..ResourceLimits::default()
        },
        capabilities: CapabilitySet::empty(),
        inputs_dir: None,
        canonicalize_json_output: false,
    };

    let result = SandboxDriver::execute(&wasm, &manifest, b"");
    assert!(result.is_err(), "zero fuel should be rejected");
}

#[test]
fn runtime_certificate_present() {
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
    )
    .unwrap();
    let manifest = standard_manifest();

    let result = SandboxDriver::execute(&wasm, &manifest, b"").unwrap();
    assert!(
        result.certificate.runtime_hash.starts_with("b3:"),
        "certificate should have b3: hash prefix"
    );
    assert!(!result.certificate.flags.is_empty());
}
