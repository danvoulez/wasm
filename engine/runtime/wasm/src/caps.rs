//! Capability model — deny-by-default, explicit grants only.
//!
//! Each capability is a typed token listed in the manifest.
//! The sandbox host checks capabilities before allowing any operation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A single capability granted to a workload.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Capability {
    /// Read files under a specific glob path inside the VFS.
    #[serde(rename = "fs.read")]
    FsRead(String),

    /// Write to the scratch/tmp directory (with quota from limits).
    #[serde(rename = "fs.tmp")]
    FsTmp(String),

    /// Read from stdin.
    #[serde(rename = "stdio.stdin")]
    Stdin,

    /// Write to stdout (subject to byte cap in limits).
    #[serde(rename = "stdio.stdout")]
    Stdout,

    /// Write to stderr (subject to byte cap in limits).
    #[serde(rename = "stdio.stderr")]
    Stderr,

    /// Read from an immutable KV store addressed by CID.
    #[serde(rename = "kv.read")]
    KvRead(String),
}

/// The set of capabilities granted to a sandbox execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySet {
    caps: BTreeSet<Capability>,
}

impl CapabilitySet {
    /// Create an empty (deny-all) capability set.
    pub fn empty() -> Self {
        Self {
            caps: BTreeSet::new(),
        }
    }

    /// Standard set for WASI/command workloads: stdin + stdout + stderr + fs.read:/inputs/**
    pub fn standard_wasi() -> Self {
        let mut caps = BTreeSet::new();
        caps.insert(Capability::Stdin);
        caps.insert(Capability::Stdout);
        caps.insert(Capability::Stderr);
        caps.insert(Capability::FsRead("/inputs/**".into()));
        caps.insert(Capability::FsTmp("/scratch".into()));
        Self { caps }
    }

    /// Insert a capability.
    pub fn grant(&mut self, cap: Capability) {
        self.caps.insert(cap);
    }

    /// Check if a capability is granted.
    pub fn has(&self, cap: &Capability) -> bool {
        self.caps.contains(cap)
    }

    /// Check if any fs.read capability is granted.
    pub fn can_read_fs(&self) -> bool {
        self.caps.iter().any(|c| matches!(c, Capability::FsRead(_)))
    }

    /// Check if stdin is granted.
    pub fn can_stdin(&self) -> bool {
        self.caps.contains(&Capability::Stdin)
    }

    /// Check if stdout is granted.
    pub fn can_stdout(&self) -> bool {
        self.caps.contains(&Capability::Stdout)
    }

    /// Check if stderr is granted.
    pub fn can_stderr(&self) -> bool {
        self.caps.contains(&Capability::Stderr)
    }

    /// Check if scratch/tmp writing is granted.
    pub fn can_tmp(&self) -> bool {
        self.caps.iter().any(|c| matches!(c, Capability::FsTmp(_)))
    }

    /// Get the canonical sorted list (for hashing into CID).
    pub fn sorted_caps(&self) -> Vec<&Capability> {
        self.caps.iter().collect()
    }

    /// Number of granted capabilities.
    pub fn len(&self) -> usize {
        self.caps.len()
    }

    /// Whether no capabilities are granted (deny-all).
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }
}

/// Errors related to capability checks.
#[derive(Debug, thiserror::Error)]
pub enum CapError {
    #[error("capability denied: {0}")]
    Denied(String),
}
