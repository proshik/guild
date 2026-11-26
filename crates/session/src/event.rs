//! Stream events and how they are parsed.

use serde_json::Value;

/// What went wrong while parsing.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The line is not JSON.
    ///
    /// That means the output is not an event stream: either `claude` was
    /// started without `--output-format stream-json`, or stray output got
    /// in. Pretending everything is fine is not an option.
    #[error("could not parse the line as JSON: {line}")]
    NotJson {
        /// What arrived. Truncated: lines can be long.
        line: String,
    },
}

/// Information from the session's first event.
#[derive(Debug, Clone, PartialEq)]
pub struct InitInfo {
    /// The session id. `--resume` uses it.
    pub session_id: String,
    /// The session's working directory.
    pub cwd: String,
    /// The model.
    pub model: String,
    /// Tools available to the session.
    pub tools: Vec<String>,
    /// Connected MCP servers.
    pub mcp_servers: Vec<String>,
}

/// Information from the final event.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultInfo {
    /// What the session cost.
    pub cost_usd: f64,
    /// How long it took.
    pub duration_ms: u64,
    /// How many turns it made.
    pub turns: u32,
    /// Whether it ended in an error.
    pub is_error: bool,
}

/// A single stream event.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// The session started.
    Init(InitInfo),

    /// A model reply.
    Assistant {
        /// The reply text, joined from the text blocks.
        text: String,
        /// The tool inside which a subtask runs.
        ///
        /// The subtask tree is built from it — without parsing text.
        parent_tool_use_id: Option<String>,
    },

    /// A hook fired.
    Hook {
        /// The hook's name.
        name: String,
        /// How it ended.
        outcome: String,
    },

    /// The session asked for a permission and did not get it.
    ///
    /// Structured provider output reports refused requests separately from
    /// successful results, so consumers can explain why a tool did not run.
    PermissionDenied {
        /// Which tool asked.
        tool_name: String,
        /// What exactly was refused, in `claude`'s own words.
        message: String,
    },

    /// The session finished.
    Result(ResultInfo),

    /// A type this crate does not know about.
    ///
    /// `claude` is updated more often than Guild. A new event type must
    /// not kill the session: it lands here and is shown raw. Otherwise a
    /// CLI update breaks the desktop, and the desktop is the one at
    /// fault.
    Unknown {
        /// The whole event, as it arrived.
        raw: Value,
    },
}

/// Parse a single stream line.
///
/// Empty lines and separators are the caller's concern: it knows the
/// stream is line-delimited. Here an empty line is an error, because it is
/// not an event.
pub fn parse_line(line: &str) -> Result<SessionEvent, SessionError> {
    let raw: Value = serde_json::from_str(line).map_err(|_| SessionError::NotJson {
        line: line.chars().take(200).collect(),
    })?;

    let kind = raw.get("type").and_then(Value::as_str).unwrap_or_default();
    let subtype = raw
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match (kind, subtype) {
        ("system", "permission_denied") => Ok(SessionEvent::PermissionDenied {
            tool_name: string_at(&raw, "tool_name"),
            message: string_at(&raw, "message"),
        }),
        ("system", "init") => Ok(SessionEvent::Init(InitInfo {
            session_id: string_at(&raw, "session_id"),
            cwd: string_at(&raw, "cwd"),
            model: string_at(&raw, "model"),
            tools: string_list(&raw, "tools"),
            mcp_servers: mcp_names(&raw),
        })),

        ("system", "hook_response") => Ok(SessionEvent::Hook {
            name: string_at(&raw, "hook_name"),
            outcome: string_at(&raw, "outcome"),
        }),

        ("assistant", _) => Ok(SessionEvent::Assistant {
            text: assistant_text(&raw),
            parent_tool_use_id: raw
                .get("parent_tool_use_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }),

        ("result", _) => Ok(SessionEvent::Result(ResultInfo {
            cost_usd: raw
                .get("total_cost_usd")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
            duration_ms: raw
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            turns: raw
                .get("num_turns")
                .and_then(Value::as_u64)
                .unwrap_or_default() as u32,
            is_error: raw
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
        })),

        _ => Ok(SessionEvent::Unknown { raw }),
    }
}

fn string_at(raw: &Value, key: &str) -> String {
    raw.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn string_list(raw: &Value, key: &str) -> Vec<String> {
    raw.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// MCP server names from `system/init`.
///
/// They arrive as objects with a `name` field rather than as strings —
/// hence the separate helper.
fn mcp_names(raw: &Value) -> Vec<String> {
    raw.get("mcp_servers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The reply text is joined from the message's text blocks.
///
/// Blocks are not only text — tool calls, for instance — and those are
/// skipped here: `parent_tool_use_id` covers the call tree.
fn assistant_text(raw: &Value) -> String {
    raw.get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}
