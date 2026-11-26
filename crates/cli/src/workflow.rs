//! Workflow is a door to the owning socket, never a local scheduler or fallback.
use super::*;
use guild_session::workflow::WorkflowCommand;

pub(super) fn run(words: &[&str], file: Option<&str>, json: bool) -> ExitCode {
    if words == ["instructions"] {
        println!(
            "{}",
            include_str!("../../../docs/workflows/native-entry.md")
        );
        return ExitCode::SUCCESS;
    }
    if words == ["help"] || words.is_empty() {
        println!(
            "guild workflow instructions | capabilities | start --from-file PATH|- | status|show RUN | checkpoint|review|retry|recover|reconcile|cancel RUN --from-file PATH|-\nMutations require version:1, request_id and expected_revision. Lost replies must reuse the same key and complete payload. No alternate socket is selected."
        );
        return ExitCode::SUCCESS;
    }
    let mutation = matches!(
        words.first(),
        Some(
            &"start" | &"checkpoint" | &"review" | &"retry" | &"recover" | &"reconcile" | &"cancel"
        )
    );
    if mutation != file.is_some() {
        return usage(
            "workflow input: mutations require --from-file PATH|-; read operations do not accept a file",
        );
    }
    let payload = if mutation {
        let input = match text(None, file) {
            Ok(Some(input)) => input,
            Ok(None) => return usage("workflow input: missing JSON"),
            Err(code) => return code,
        };
        if input.len() > 4 * 1024 * 1024 {
            return usage("workflow input: exceeds 4 MiB");
        }
        match guild_session::workflow::parse_workflow_input(input.as_bytes()) {
            Ok(value) => value,
            Err(_) => return usage("workflow input: invalid JSON"),
        }
    } else {
        serde_json::Value::Null
    };
    let decode = || -> Result<WorkflowCommand, String> {
        Ok(match words {
            ["capabilities"] => WorkflowCommand::Capabilities { version: 1 },
            ["start"] => WorkflowCommand::Start {
                spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
            },
            [operation, raw] => {
                let run = raw.parse().map_err(|_| "run must be a UUID")?;
                match *operation {
                    "status" => WorkflowCommand::Status { version: 1, run },
                    "show" => WorkflowCommand::Show { version: 1, run },
                    "checkpoint" => WorkflowCommand::Checkpoint {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    "review" => WorkflowCommand::Review {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    "retry" => WorkflowCommand::Retry {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    "reconcile" => WorkflowCommand::Reconcile {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    "recover" => WorkflowCommand::Recover {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    "cancel" => WorkflowCommand::Cancel {
                        run,
                        spec: serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    },
                    _ => return Err("unknown operation".into()),
                }
            }
            _ => return Err("unknown operation or arguments".into()),
        })
    };
    let request = match decode().and_then(|request| {
        request.validate()?;
        Ok(request)
    }) {
        Ok(request) => request,
        Err(error) => return usage(&format!("workflow input: {error}")),
    };
    let socket = match std::env::var(SOCKET) {
        Ok(path) => PathBuf::from(path),
        Err(_) => return usage("GUILD_SOCKET is not set"),
    };
    let command = Command::Workflow {
        request,
        caller: std::env::var(guild_session::SESSION_VAR).ok(),
    };
    match ask(&socket, &command) {
        Ok(answer) => say(&answer, json),
        Err(error) => refuse_locally(&error),
    }
}
