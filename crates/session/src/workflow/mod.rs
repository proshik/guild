//! Portable development workflow, independent of execution hosts and GPUI.

mod definition;
mod json;
pub use definition::*;

/// Validate a complete version-one workflow definition, without executing it.
pub fn validate_definition(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let definition = Definition::parse(bytes)?;
    serde_json::to_value(definition).map_err(|error| error.to_string())
}

mod contract;
pub use contract::*;

mod skills;
pub use skills::*;
mod evidence;
pub use evidence::*;
mod ids;
pub use ids::*;
mod journal;
pub use journal::*;
mod protocol;
pub use protocol::*;
mod graph;
pub use graph::*;

mod reviews;
pub use reviews::*;

mod revust;
pub use revust::*;
