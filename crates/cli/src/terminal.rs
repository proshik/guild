//! Typed terminal transport; waiting never holds a desktop request open.
use super::*;
use guild_session::terminal_job::{LaunchSpec, TerminalCommand, TerminalReply};
use std::time::{Duration, Instant};

pub(super) fn run(
    words: &[&str],
    file: Option<&str>,
    timeout: Option<&str>,
    json: bool,
) -> ExitCode {
    let mut wait = false;
    let request = match words {
        ["capabilities"] => TerminalCommand::Capabilities,
        ["start"] => {
            let input = match text(None, file) {
                Ok(Some(input)) => input,
                Ok(None) => return usage("terminal start needs --from-file PATH|-"),
                Err(code) => return code,
            };
            let spec: LaunchSpec = match serde_json::from_str(&input) {
                Ok(spec) => spec,
                Err(_) => return usage("terminal start needs a valid version-1 descriptor"),
            };
            if let Err(error) = spec.validate() {
                return refuse_locally(&error);
            }
            TerminalCommand::Start { spec }
        }
        [operation, raw] => {
            let id = match raw.parse() {
                Ok(id) => id,
                Err(_) => return usage("terminal job ID must be a UUID"),
            };
            match *operation {
                "status" => TerminalCommand::Status { id },
                "show" => TerminalCommand::Show { id },
                "hide" => TerminalCommand::Hide { id },
                "close" => TerminalCommand::Close { id },
                "cancel" => TerminalCommand::Cancel { id },
                "wait" => {
                    wait = true;
                    TerminalCommand::Status { id }
                }
                _ => return usage("unknown terminal operation"),
            }
        }
        _ => return usage("unknown terminal command"),
    };
    let seconds = match timeout.unwrap_or("30").parse::<u64>() {
        Ok(seconds) if (1..=60).contains(&seconds) => seconds,
        _ => return usage("terminal wait --timeout must be 1..60 seconds"),
    };
    let socket = match std::env::var(SOCKET) {
        Ok(path) => PathBuf::from(path),
        Err(_) => return usage("GUILD_SOCKET is not set"),
    };
    let command = Command::Terminal {
        request,
        caller: std::env::var(guild_session::SESSION_VAR).ok(),
    };
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut pending_answer = None;
    loop {
        if wait
            && Instant::now() >= until
            && let Some(answer) = &pending_answer
        {
            return say(answer, json);
        }
        let patience = if wait {
            until
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1))
        } else {
            Duration::from_secs(10)
        };
        let answer = match ask_with_timeout(&socket, &command, Some(patience)) {
            Ok(answer) => answer,
            // A bounded wait can end while the next status reply is in flight.
            // Return the last observed pending receipt; no completion or
            // cancellation is inferred. A fresh wait will ask the window again.
            Err(_) if wait && Instant::now() >= until && pending_answer.is_some() => {
                return say(
                    pending_answer.as_ref().expect("observed pending receipt"),
                    json,
                );
            }
            Err(error) => return refuse_locally(&error),
        };
        let pending = matches!(&answer, Answer::Terminal { reply: TerminalReply::Job { receipt } } if !receipt.state.is_terminal());
        if !wait || !pending {
            return say(&answer, json);
        }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining <= Duration::from_millis(250) {
            std::thread::sleep(remaining);
            return say(&answer, json);
        }
        pending_answer = Some(answer);
        std::thread::sleep(Duration::from_millis(250));
    }
}
