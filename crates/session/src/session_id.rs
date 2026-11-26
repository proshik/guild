//! What a session is made of: an identifier and a way of launching it.
//!
//! The list of sessions itself lives in [`crate::desk`]: that module
//! is about projects, while these two types are needed without it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persisted launch mode. Earlier stream records reopen in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// The harness CLI in a pseudo-terminal, including permissions and slash commands.
    #[serde(alias = "stream")]
    Terminal,
}

/// A session identifier.
///
/// It is also the `claude` conversation id: Guild assigns it with the
/// `--session-id` flag rather than learning it after the first event. That
/// way it is known before the first turn — and, more importantly, known
/// **exactly**: every session in one directory writes at once, and there
/// is no telling yours from the transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    /// A fresh identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Read back what [`Display`](std::fmt::Display) wrote.
///
/// M5.10: a hook's payload names its session as a string, and that string
/// is the id `--session-id` was given.
impl std::str::FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(text).map(Self)
    }
}
