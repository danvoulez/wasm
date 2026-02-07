//! Virtual Filesystem — inputs (read-only) + scratch (quota-limited, ephemeral).
//!
//! The VFS is mounted into the WASI context. No other filesystem access is allowed.

use std::path::{Path, PathBuf};

/// Configuration for the sandbox virtual filesystem.
#[derive(Debug, Clone)]
pub struct VfsConfig {
    /// Host directory to mount as read-only /inputs inside the sandbox.
    pub inputs_host_path: Option<PathBuf>,

    /// Host directory for the ephemeral scratch space (/scratch inside sandbox).
    pub scratch_host_path: PathBuf,

    /// Maximum bytes allowed in scratch.
    pub scratch_quota_bytes: u64,
}

impl VfsConfig {
    /// Create a new VFS config.
    pub fn new(
        inputs_host_path: Option<PathBuf>,
        scratch_host_path: PathBuf,
        scratch_quota_bytes: u64,
    ) -> Self {
        Self {
            inputs_host_path,
            scratch_host_path,
            scratch_quota_bytes,
        }
    }

    /// Prepare the VFS directories on the host side.
    pub fn prepare(&self) -> Result<(), VfsError> {
        // Ensure scratch dir exists
        std::fs::create_dir_all(&self.scratch_host_path)
            .map_err(|e| VfsError::Setup(format!("cannot create scratch dir: {e}")))?;

        // Validate inputs dir if provided
        if let Some(ref inputs) = self.inputs_host_path {
            if !inputs.exists() {
                return Err(VfsError::Setup(format!(
                    "inputs directory does not exist: {}",
                    inputs.display()
                )));
            }
            if !inputs.is_dir() {
                return Err(VfsError::Setup(format!(
                    "inputs path is not a directory: {}",
                    inputs.display()
                )));
            }
        }

        Ok(())
    }

    /// Check that a path doesn't escape via traversal.
    pub fn validate_path(base: &Path, requested: &Path) -> Result<PathBuf, VfsError> {
        let canonical_base = base
            .canonicalize()
            .map_err(|e| VfsError::PathTraversal(format!("cannot canonicalize base: {e}")))?;
        let full = base.join(requested);
        let canonical = full
            .canonicalize()
            .map_err(|e| VfsError::PathTraversal(format!("cannot canonicalize path: {e}")))?;

        if !canonical.starts_with(&canonical_base) {
            return Err(VfsError::PathTraversal(format!(
                "path traversal detected: {} escapes {}",
                requested.display(),
                base.display()
            )));
        }
        Ok(canonical)
    }

    /// Get the current scratch usage in bytes.
    pub fn scratch_usage(&self) -> Result<u64, VfsError> {
        dir_size(&self.scratch_host_path)
    }

    /// Check if scratch quota is exceeded.
    pub fn check_scratch_quota(&self) -> Result<(), VfsError> {
        let usage = self.scratch_usage()?;
        if usage > self.scratch_quota_bytes {
            return Err(VfsError::QuotaExceeded {
                used: usage,
                quota: self.scratch_quota_bytes,
            });
        }
        Ok(())
    }
}

/// Recursively compute directory size.
fn dir_size(path: &Path) -> Result<u64, VfsError> {
    let mut total = 0u64;
    if path.is_dir() {
        let entries = std::fs::read_dir(path)
            .map_err(|e| VfsError::Setup(format!("cannot read dir: {e}")))?;
        for entry in entries {
            let entry = entry.map_err(|e| VfsError::Setup(format!("dir entry error: {e}")))?;
            let ft = entry
                .file_type()
                .map_err(|e| VfsError::Setup(format!("file type error: {e}")))?;
            if ft.is_file() {
                total += entry
                    .metadata()
                    .map_err(|e| VfsError::Setup(format!("metadata error: {e}")))?
                    .len();
            } else if ft.is_dir() {
                total += dir_size(&entry.path())?;
            }
        }
    }
    Ok(total)
}

/// VFS errors.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("VFS setup error: {0}")]
    Setup(String),

    #[error("path traversal: {0}")]
    PathTraversal(String),

    #[error("scratch quota exceeded: used {used} bytes, quota {quota} bytes")]
    QuotaExceeded { used: u64, quota: u64 },
}
