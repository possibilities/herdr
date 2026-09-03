//! Shim for herdr's `detect::manifest_update`. herdr fetches manifest updates
//! from its release catalog and records their status here; this crate never
//! does. `/maintain` on the herdx workshop is the update path. Only the
//! symbols `manifest.rs` reaches for are provided, with `ManifestVersion`
//! reproduced verbatim so remote-versus-bundled comparisons keep herdr's
//! semantics if a caller ever drops a manifest into the remote path.

use std::{cmp::Ordering, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{agent_label, Agent};

/// The manifest engine version this crate implements. Must track herdr's
/// constant of the same name, which gates `min_engine_version` in manifests.
pub(crate) const MANIFEST_ENGINE_VERSION: u32 = 3;

#[derive(Debug, Clone)]
pub(crate) struct ManifestVersion(String);

impl ManifestVersion {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("version must not be empty".to_string());
        }
        for segment in trimmed.split('.') {
            if segment.is_empty() {
                return Err(format!("version {trimmed:?} contains an empty segment"));
            }
            if !segment.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(format!("version {trimmed:?} must be dotted numeric"));
            }
            segment
                .parse::<u64>()
                .map_err(|_| format!("version {trimmed:?} contains an oversized segment"))?;
        }
        Ok(Self(trimmed.to_string()))
    }
}

impl fmt::Display for ManifestVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ManifestVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ManifestVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl Ord for ManifestVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut left = self.0.split('.');
        let mut right = other.0.split('.');

        loop {
            match (left.next(), right.next()) {
                (Some(left), Some(right)) => {
                    let left = left.parse::<u64>().unwrap_or(0);
                    let right = right.parse::<u64>().unwrap_or(0);
                    match left.cmp(&right) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                (Some(left), None) => {
                    let left = left.parse::<u64>().unwrap_or(0);
                    if left == 0 {
                        continue;
                    }
                    return Ordering::Greater;
                }
                (None, Some(right)) => {
                    let right = right.parse::<u64>().unwrap_or(0);
                    if right == 0 {
                        continue;
                    }
                    return Ordering::Less;
                }
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

impl PartialOrd for ManifestVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ManifestVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ManifestVersion {}

/// Remote update status for one agent. Never populated by this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRemoteStatus {
    pub(crate) last_result: String,
    pub(crate) last_error: Option<String>,
}

/// Remote update status. Always empty here.
#[derive(Debug, Clone, Default)]
pub(crate) struct ManifestUpdateStatus;

impl ManifestUpdateStatus {
    pub(crate) fn agent_status(&self, _agent: Agent) -> Option<AgentRemoteStatus> {
        None
    }
}

pub(crate) fn load_status() -> ManifestUpdateStatus {
    ManifestUpdateStatus
}

/// Where herdr would cache a fetched manifest. This crate never writes it,
/// so it exists only to satisfy the engine's "newer cached remote beats
/// bundled" lookup, which finds nothing.
pub(crate) fn remote_manifest_path(agent: Agent) -> PathBuf {
    crate::config::config_dir()
        .join("agent-detection-remote")
        .join(format!("{}.toml", agent_label(agent)))
}
