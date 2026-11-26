//! Workflow identities remain distinct from session, pane and ephemeral tab IDs.
use serde::{Deserialize, Serialize};
macro_rules! identity {
    ($name:ident,$description:literal) => {
        #[doc=$description]
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name {
            /// Reserve a new durable identity before any external effect.
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}
identity!(
    RunId,
    "Stable process identity, never a fabricated SessionId."
);
identity!(
    AttemptId,
    "Stable step/effect attempt identity retained across callbacks and recovery."
);
