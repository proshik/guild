//! Persistable artifact provenance shared by selection, journal and acknowledgement.
use super::{Channel, Source, repository_valid};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
/// Immutable provenance pinned by both candidate selection and confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Origin {
    /// Completion directory, including the explicit default-directory choice.
    Local {
        /// Saved completion directory, including the default-directory choice.
        directory: Option<PathBuf>,
        /// Verified executable/resource seal binding the selected copy.
        seal: String,
    },
    /// Exact provider, channel, release and authenticated archive identity.
    Remote {
        /// Repository configured in the running build.
        repository: String,
        /// Selected release channel.
        channel: Channel,
        /// Provider release identity.
        release_id: u64,
        /// Signed archive checksum.
        sha256: String,
    },
}
/// Honest artifact identity for new and legacy bundles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Artifact {
    /// Embedded identity of a newly built payload.
    Build(Uuid),
    /// Legacy has no embedded UUID; pin the actual verified code seal instead.
    LegacySeal(String),
}
/// A distinct artifact; product version comparison is deliberately separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Identity {
    /// Unique embedded artifact identity.
    pub artifact: Artifact,
    /// Source and payload binding.
    pub origin: Origin,
}

impl Identity {
    /// Reject malformed or ambiguous authority when reading a recovery receipt.
    pub fn validate(&self) -> Result<(), String> {
        let valid = match &self.origin {
            Origin::Local { directory, seal } => {
                directory.as_ref().is_none_or(|p| p.is_absolute())
                    && sha(seal)
                    && match &self.artifact {
                        Artifact::Build(id) => !id.is_nil(),
                        Artifact::LegacySeal(expected) => expected == seal,
                    }
            }
            Origin::Remote {
                repository,
                release_id,
                sha256,
                ..
            } => {
                repository_valid(repository)
                    && *release_id > 0
                    && sha(sha256)
                    && matches!(&self.artifact, Artifact::Build(id) if !id.is_nil())
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid update artifact identity".into())
        }
    }
    /// User-visible source, independent of the current provider implementation.
    pub fn source(&self) -> Source {
        match self.origin {
            Origin::Local { .. } => Source::Local,
            Origin::Remote { .. } => Source::Remote,
        }
    }
}
fn sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
