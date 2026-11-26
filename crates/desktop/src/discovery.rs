//! Application-owned CLI discovery, composed per terminal launch.
//! No project skill or development workflow is selected here.
use guild_session::HarnessKind;
use portable_pty::CommandBuilder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MATCHER: &str = "startup|resume|clear|compact";
const TIMEOUT: u64 = 5;
const KEY: &str = "/<session-flags>/config.toml:session_start:0:0";
const PLUGIN: &str = include_str!("../assets/hooks/cli-discovery.js");
/// Private files survive the child, including its closing reaper.
/// Contents are written once before launch and verified before any trust grant.
pub struct Material {
    directory: tempfile::TempDir,
    files: Vec<(PathBuf, Vec<u8>)>,
    command: String,
}
impl Material {
    pub fn prepare(parent: &Path) -> io::Result<Self> {
        if parent.to_str().is_none() {
            return Err(io::Error::other(
                "CLI discovery unavailable: instruction path is not UTF-8",
            ));
        }
        std::fs::create_dir_all(parent)?;
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .prefix("cli-")
            .tempdir_in(parent)?;
        let payload = serde_json::to_vec(&json!({"hookSpecificOutput": {
            "hookEventName": "SessionStart", "additionalContext": guild_session::command::SESSION_PROMPT
        }}))?;
        let command = format!(
            "/bin/cat {}",
            shell_quote(&directory.path().join("context.json"))
        );
        let settings = serde_json::to_vec(&json!({"hooks":{"SessionStart":[{
            "matcher": MATCHER, "hooks":[{"type":"command", "command":command, "timeout":TIMEOUT}]
        }]}}))?;
        let plugin = PLUGIN.replace(
            "__GUILD_DISCOVERY_JSON__",
            &serde_json::to_string(guild_session::command::SESSION_PROMPT)?,
        );
        let files = vec![
            (directory.path().join("context.json"), payload),
            (directory.path().join("claude.json"), settings),
            (directory.path().join("package.json"), br#"{"name":"guild-cli-discovery","version":"1.0.0","type":"module","exports":{".":"./server"}}"#.to_vec()),
            // V2 Host.resolve uses an explicit server entrypoint in a local package.
            (directory.path().join("server"), plugin.into_bytes()),
        ];
        for (path, bytes) in &files {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o400)
                .open(path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Ok(Self {
            directory,
            files,
            command,
        })
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
    #[cfg(test)]
    pub(crate) fn keep_fixture(self) {
        let Self { directory, .. } = self;
        let _ = directory.keep();
    }
    fn verify(&self) -> io::Result<()> {
        for (path, bytes) in &self.files {
            let metadata = std::fs::symlink_metadata(path)?;
            if !metadata.is_file() || std::fs::read(path)? != *bytes {
                return Err(io::Error::other(
                    "Owned CLI discovery material changed; no hook trust granted",
                ));
            }
        }
        Ok(())
    }
    /// Compose only the known host source; never replace model/developer text,
    /// approval policy or existing user hook state. Managed restrictions remain native.
    pub fn apply(
        &mut self,
        command: &mut CommandBuilder,
        harness: HarnessKind,
        inline: Option<&str>,
    ) -> io::Result<()> {
        self.verify()?;
        match harness {
            HarnessKind::Claude => {
                let positions: Vec<_> = command
                    .get_argv()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, arg)| (arg == "--settings").then_some(i))
                    .collect();
                if positions.len() > 1 {
                    return Err(io::Error::other(
                        "Claude CLI discovery unavailable: multiple existing settings inputs; preserved unchanged",
                    ));
                }
                if let Some(&at) = positions.first() {
                    let source = command
                        .get_argv()
                        .get(at + 1)
                        .ok_or_else(|| io::Error::other("Claude settings path missing"))?;
                    let mut settings: Value = serde_json::from_slice(&std::fs::read(source)?)?;
                    let object = settings
                        .as_object_mut()
                        .ok_or_else(|| io::Error::other("Claude settings must be an object"))?;
                    let hooks = object
                        .entry("hooks")
                        .or_insert_with(|| json!({}))
                        .as_object_mut()
                        .ok_or_else(|| io::Error::other("Claude hooks must be an object"))?;
                    let groups = hooks
                        .entry("SessionStart")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .ok_or_else(|| io::Error::other("Claude SessionStart must be an array"))?;
                    groups.push(json!({"matcher": MATCHER, "hooks":[{"type":"command", "command":self.command,"timeout":TIMEOUT}]}));
                    let bytes = serde_json::to_vec(&settings)?;
                    let path = self
                        .path()
                        .join(format!("claude-composed-{}.json", self.files.len()));
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o400)
                        .open(&path)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    self.files.push((path.clone(), bytes));
                    // Claude selects the last --settings, rather than merging flags.
                    // Replace the sole app-owned input, retaining all its fields/hooks.
                    command.get_argv_mut()[at + 1] = path.into_os_string();
                } else {
                    command.arg("--settings");
                    command.arg(self.path().join("claude.json"));
                }
            }
            HarnessKind::Codex => {
                let declaration = format!(
                    "hooks.SessionStart=[{{matcher={},hooks=[{{type=\"command\",command={},timeout={TIMEOUT}}}]}}]",
                    toml_string(MATCHER),
                    toml_string(&self.command)
                );
                // Codex 0.162.1 fingerprints sorted JSON converted from normalized TOML.
                // Explicit timeout avoids dependence on the upstream default. None fields
                // are absent in TOML; async=false is present after normalization.
                let mut identity = json!({"event_name":"session_start","matcher":MATCHER,
                    "hooks":[{"type":"command","command":self.command,"timeout":TIMEOUT,"async":false}]});
                identity.sort_all_objects();
                let hash = format!(
                    "sha256:{:x}",
                    Sha256::digest(serde_json::to_vec(&identity)?)
                );
                let state = format!(
                    "hooks.state={{{}={{trusted_hash={}}}}}",
                    toml_string(KEY),
                    toml_string(&hash)
                );
                for value in [declaration, state] {
                    command.arg("-c");
                    command.arg(value);
                }
            }
            HarnessKind::OpenCode => {
                let content = opencode_content(inline, self.path())?;
                command.env("OPENCODE_CONFIG_CONTENT", content);
            }
        }
        Ok(())
    }
}

// JSON string escapes are TOML basic-string escapes for these UTF-8 values.
fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string is serializable")
}
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Existing inline fields/directives are retained. JSONC is deliberately refused
/// rather than partially parsed or replaced; the unchanged launch can still run.
fn opencode_content(inline: Option<&str>, plugin: &Path) -> io::Result<String> {
    let mut config: Value = match inline {
        Some(text) => serde_json::from_str(text).map_err(|_| io::Error::other(
            "OpenCode CLI discovery unavailable: inherited OPENCODE_CONFIG_CONTENT is not strict JSON (JSONC/comments/trailing commas are preserved unchanged)"))?,
        None => json!({}),
    };
    let object = config.as_object_mut().ok_or_else(|| io::Error::other(
        "OpenCode CLI discovery unavailable: inherited inline config is not an object; preserved unchanged"))?;
    let plugins = object.entry("plugins").or_insert_with(|| json!([])).as_array_mut()
        .ok_or_else(|| io::Error::other("OpenCode CLI discovery unavailable: inline plugins is not an array; preserved unchanged"))?;
    if plugins
        .iter()
        .any(|value| matches!(value.as_str(), Some("-*") | Some("-guild-cli-discovery")))
    {
        return Err(io::Error::other(
            "OpenCode CLI discovery unavailable: inherited config explicitly disables all plugins or guild-cli-discovery; preserved unchanged",
        ));
    }
    let path = json!(plugin);
    if !plugins.contains(&path) {
        plugins.push(path);
    }
    serde_json::to_string(&config).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_opencode_user_fields_and_plugin_directives_survive() {
        let prior = r#"{"model":"explicit","permissions":["keep"],"plugins":["user","-disabled"],"custom":{"value":"quotes \\\" and Кириллица"}}"#;
        let old: Value = serde_json::from_str(prior).unwrap();
        let new: Value = serde_json::from_str(
            &opencode_content(Some(prior), Path::new("/host/'space plugin")).unwrap(),
        )
        .unwrap();
        for key in ["model", "permissions", "custom"] {
            assert_eq!(new[key], old[key]);
        }
        assert_eq!(
            &new["plugins"].as_array().unwrap()[..2],
            old["plugins"].as_array().unwrap()
        );
        assert_eq!(new["plugins"][2], "/host/'space plugin");
        for bad in [
            "{/* user comment */}",
            "{\"plugins\":[],}",
            "null",
            "{\"plugins\":false}",
            "{\"plugins\":[\"-guild-cli-discovery\"]}",
        ] {
            assert!(opencode_content(Some(bad), Path::new("/host")).is_err());
        }
    }
    #[test]
    fn round04_real_opencode_adapter_preserves_plugin_exclusions() {
        let root = tempfile::tempdir().unwrap();
        let mut material = Material::prepare(root.path()).unwrap();
        for plugins in [
            json!(["-*"]),
            json!(["user", "-*", "another-user"]),
            json!(["-guild-cli-discovery"]),
        ] {
            let inline = serde_json::to_string(
                &json!({"plugins":plugins,"model":"chosen","permissions":["keep"]}),
            )
            .unwrap();
            let mut command = CommandBuilder::new("opencode");
            command.env("OPENCODE_CONFIG_CONTENT", &inline);
            let error = material
                .apply(&mut command, HarnessKind::OpenCode, Some(&inline))
                .expect_err("explicit exclusion must not be undone by host append");
            assert!(error.to_string().contains("disables"));
            assert_eq!(
                command.get_env("OPENCODE_CONFIG_CONTENT").unwrap(),
                inline.as_str()
            );
        }
        // Upstream recognizes only *, namespace.* and exact names. These
        // selectors do not match the non-namespaced host id.
        for plugins in [json!(["-guild.*"]), json!(["-guild*"]), json!(["-other"])] {
            let inline = serde_json::to_string(&json!({"plugins":plugins})).unwrap();
            let mut command = CommandBuilder::new("opencode");
            material
                .apply(&mut command, HarnessKind::OpenCode, Some(&inline))
                .unwrap();
        }
    }

    #[test]
    fn helper_is_private_verified_and_owned() {
        use std::os::unix::fs::PermissionsExt;
        let parent = tempfile::tempdir().unwrap();
        let mut material = Material::prepare(parent.path()).unwrap();
        let path = material.path().to_owned();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let context: Value =
            serde_json::from_slice(&std::fs::read(path.join("context.json")).unwrap()).unwrap();
        assert_eq!(
            context["hookSpecificOutput"]["additionalContext"],
            guild_session::command::SESSION_PROMPT
        );
        let mut command = CommandBuilder::new("codex");
        material
            .apply(&mut command, HarnessKind::Codex, None)
            .unwrap();
        let args: Vec<_> = command
            .get_argv()
            .iter()
            .map(|a| a.to_string_lossy())
            .collect();
        assert!(args.iter().any(|a| a.contains("trusted_hash")));
        assert!(!args.iter().any(|a| a.contains("enabled")
            || a.contains("bypass")
            || a.contains("developer_instructions")));
        std::fs::set_permissions(
            path.join("context.json"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::write(path.join("context.json"), b"changed").unwrap();
        assert!(
            material
                .apply(&mut CommandBuilder::new("codex"), HarnessKind::Codex, None)
                .is_err()
        );
        drop(material);
        assert!(!path.exists());
    }
    #[test]
    fn every_terminal_harness_gets_the_same_additive_source() {
        let parent = tempfile::tempdir().unwrap();
        let mut material = Material::prepare(parent.path()).unwrap();
        for harness in HarnessKind::ALL {
            for resumed in [false, true] {
                let mut command = CommandBuilder::new(harness.program());
                if resumed {
                    command.arg("synthetic-resume");
                }
                command.arg("explicit-user-choice");
                let initial = command.get_argv().to_vec();
                material
                    .apply(
                        &mut command,
                        harness,
                        Some(r#"{"model":"user","plugins":["keep"]}"#),
                    )
                    .unwrap();
                assert_eq!(&command.get_argv()[..initial.len()], initial);
                let args: Vec<_> = command
                    .get_argv()
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect();
                assert!(!args.iter().any(|a| a.contains("system-prompt")
                    || a.contains("developer_instructions")
                    || a.contains("approval_policy")
                    || a.contains("sandbox")));
                match harness {
                    HarnessKind::Claude => {
                        let config: Value = serde_json::from_slice(
                            &std::fs::read(material.path().join("claude.json")).unwrap(),
                        )
                        .unwrap();
                        let hook = &config["hooks"]["SessionStart"][0]["hooks"][0];
                        let output = std::process::Command::new("/bin/sh")
                            .args(["-c", hook["command"].as_str().unwrap()])
                            .output()
                            .unwrap();
                        assert!(output.status.success());
                        let context: Value = serde_json::from_slice(&output.stdout).unwrap();
                        assert_eq!(
                            context["hookSpecificOutput"]["additionalContext"],
                            guild_session::command::SESSION_PROMPT
                        );
                    }
                    HarnessKind::Codex => {
                        assert!(args.iter().any(|a| a.starts_with("hooks.SessionStart=")));
                        assert!(args.iter().any(|a| a.starts_with("hooks.state=")));
                    }
                    HarnessKind::OpenCode => {
                        let config: Value = serde_json::from_str(
                            command
                                .get_env("OPENCODE_CONFIG_CONTENT")
                                .unwrap()
                                .to_str()
                                .unwrap(),
                        )
                        .unwrap();
                        assert_eq!(config["model"], "user");
                        assert_eq!(config["plugins"][0], "keep");
                        let plugin =
                            std::fs::read_to_string(material.path().join("server")).unwrap();
                        assert!(plugin.contains(
                            &serde_json::to_string(guild_session::command::SESSION_PROMPT).unwrap()
                        ));
                    }
                }
            }
        }
        export_owned_adapter_fixture_when_requested();
    }

    /// A deterministic handoff to the installed, provider-free qualification
    /// script. Normal cargo test uses a disposable directory and exports nothing.
    fn export_owned_adapter_fixture_when_requested() {
        let Some(root) = std::env::var_os("GUILD_DISCOVERY_FIXTURE_DIRECTORY") else {
            return;
        };
        let root = PathBuf::from(root);
        let mut material = Material::prepare(&root).unwrap();
        let mut commands = json!({});
        for harness in HarnessKind::ALL {
            let mut command = CommandBuilder::new(harness.program());
            material.apply(&mut command, harness, None).unwrap();
            commands[harness.program()] = json!({"argv":command.get_argv().iter().map(|a|a.to_string_lossy()).collect::<Vec<_>>(),
                "inline":command.get_env("OPENCODE_CONFIG_CONTENT").and_then(|a|a.to_str())});
        }
        std::fs::write(
            root.join("adapter-commands.json"),
            serde_json::to_vec(&commands).unwrap(),
        )
        .unwrap();
        // The caller owns this expressly requested ignored fixture directory.
        material.keep_fixture();
    }
}
