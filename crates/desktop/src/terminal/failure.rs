//! A bounded, transient explanation of a completed harness failure.
use guild_session::HarnessKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub harness: HarnessKind,
    pub code: u32,
    pub network: bool,
}

impl Failure {
    pub fn derive(harness: Option<HarnessKind>, code: Option<u32>, tail: &str) -> Option<Self> {
        let harness = harness?;
        let code = code.filter(|code| *code != 0)?;
        let tail = tail.to_ascii_lowercase();
        let network = [
            "workspace routing discovery failed",
            "dns lookup failed",
            "could not resolve host",
            "failed to resolve host",
            "name or service not known",
            "econnrefused",
            "enotfound",
            "enetunreach",
            "ehostunreach",
            "connection refused",
            "connection timed out",
            "connect timeout",
            "network is unreachable",
        ]
        .iter()
        .any(|evidence| tail.contains(evidence));
        Some(Self {
            harness,
            code,
            network,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finished_harnesses_have_cautious_guidance_and_other_terminals_do_not() {
        for harness in [
            HarnessKind::Claude,
            HarnessKind::Codex,
            HarnessKind::OpenCode,
        ] {
            for (output, network) in [
                (
                    "Error: account/read failed during TUI bootstrap: account/read failed: workspace routing discovery failed (code -32603)",
                    true,
                ),
                ("DNS lookup failed", true),
                ("connect ECONNREFUSED", true),
                ("connection timed out", true),
                ("account/read failed (code -32603)", false),
                ("authentication failed", false),
                ("failed to resolve configuration", false),
                ("unknown error", false),
            ] {
                assert_eq!(
                    Failure::derive(Some(harness), Some(7), output),
                    Some(Failure {
                        harness,
                        code: 7,
                        network
                    })
                );
                assert_eq!(Failure::derive(Some(harness), Some(0), output), None);
                assert_eq!(Failure::derive(Some(harness), None, output), None);
                assert_eq!(Failure::derive(None, Some(7), output), None);
            }
        }
    }
}
