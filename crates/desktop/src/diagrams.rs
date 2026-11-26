//! Fixed bundled renderer resources and subprocess transport. No WebKit is linked here.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Engine {
    Mermaid,
    PlantUml,
}
impl Engine {
    pub fn id(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid",
            Self::PlantUml => "plantuml",
        }
    }
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid-11.12.0",
            Self::PlantUml => "plantuml-1.2026.9",
        }
    }
}

/// Exact resources owned by one app, or an explicitly prepared development directory.
#[derive(Clone, Debug)]
pub struct Resources {
    pub helper: PathBuf,
    pub assets: PathBuf,
}
impl Resources {
    /// No I/O or hashing on the UI thread. Verification belongs to the render worker.
    pub fn discover() -> anyhow::Result<Self> {
        Self::for_executable(
            &std::env::current_exe()?,
            std::env::var_os("GUILD_DIAGRAM_RESOURCES")
                .as_deref()
                .map(Path::new),
        )
    }
    pub fn for_executable(executable: &Path, development: Option<&Path>) -> anyhow::Result<Self> {
        if let Some(macos) = executable.parent()
            && macos.file_name().is_some_and(|n| n == "MacOS")
            && let Some(contents) = macos.parent()
            && contents.file_name().is_some_and(|n| n == "Contents")
            && contents
                .parent()
                .is_some_and(|p| p.extension().is_some_and(|e| e == "app"))
        {
            // A production app never falls back to developer or app-support resources.
            return Ok(Self {
                helper: macos.join("guild-diagram-renderer"),
                assets: contents.join("Resources/diagrams"),
            });
        }
        development.map(Self::from_directory).ok_or_else(|| anyhow::anyhow!("Guild diagram resources are unavailable. Development: run just prepare-diagrams and launch with GUILD_DIAGRAM_RESOURCES pointing to that prepared directory."))
    }
    pub fn from_directory(root: impl AsRef<Path>) -> Self {
        Self {
            helper: root.as_ref().join("guild-diagram-renderer"),
            assets: root.as_ref().to_owned(),
        }
    }
    /// Immutable JS pins are compiled into Guild; helper digest is an app-signed
    /// build resource. All reads/hashes happen on the worker before execution.
    pub fn verify(&self, engine: Engine) -> anyhow::Result<()> {
        use anyhow::Context as _;
        self.verify_files(engine).context(
            "Guild diagram bundle is missing or corrupt. Reinstall Guild; source remains editable.",
        )
    }
    fn verify_files(&self, engine: Engine) -> anyhow::Result<()> {
        use anyhow::ensure;
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            cfg!(all(target_os = "macos", target_arch = "aarch64")),
            "Supported platform is macOS arm64"
        );
        ensure!(
            std::fs::symlink_metadata(&self.assets)?
                .file_type()
                .is_dir(),
            "Invalid bundled resource directory"
        );
        ensure!(
            std::fs::symlink_metadata(self.assets.join(engine.directory()))?
                .file_type()
                .is_dir(),
            "Missing bundled engine {}",
            engine.id()
        );
        let prefix = format!("{}/", engine.directory());
        for line in include_str!("../assets/diagrams/files.sha256").lines() {
            let (expected, name) = line.split_once("  ").expect("vendored file pin");
            if name.starts_with(&prefix) {
                let data = read_file(&self.assets.join(name), 5 * 1024 * 1024)?;
                ensure!(
                    format!("{:x}", Sha256::digest(data)) == expected,
                    "Bundled resource integrity mismatch: {name}"
                );
            }
        }
        let expected = String::from_utf8(read_file(&self.assets.join("helper.sha256"), 65)?)?;
        let expected = expected.trim();
        ensure!(
            expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid bundled helper digest"
        );
        let data = read_file(&self.helper, 1024 * 1024)?;
        ensure!(
            std::fs::metadata(&self.helper)?.permissions().mode() & 0o111 != 0,
            "Bundled helper is not executable"
        );
        ensure!(
            format!("{:x}", Sha256::digest(data)) == expected,
            "Bundled helper integrity mismatch"
        );
        Ok(())
    }
}
fn read_file(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    use anyhow::ensure;
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= limit,
        "Expected bounded regular bundled resource: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Bundled file exceeds size limit"
    );
    Ok(bytes)
}

/// Subprocess-only transport; callers run this blocking adapter on their background owner.
pub mod transport {
    use anyhow::{Result, bail, ensure};
    use guild_session::process::ProcessGroup;
    use std::{
        io::{BufRead, Read, Write},
        path::Path,
        process::{Command, Stdio},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    /// One bounded render. The caller must verify the bundled helper and engine files first.
    /// Cancellation and timeout stop/reap the group before joining pipe readers/writers.
    /// No document path, shell command or arbitrary script crosses this protocol.
    pub fn render(
        helper: &Path,
        resources: &Path,
        engine: &str,
        source: &str,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        ensure!(
            matches!(engine, "mermaid" | "plantuml") && source.len() <= 262144,
            "Invalid diagram input"
        );
        if cancel.load(Ordering::Acquire) {
            bail!("Diagram cancelled")
        }
        let input = serde_json::to_vec(&serde_json::json!({"engine":engine,"source":source}))?;
        let mut group = ProcessGroup::spawn(
            Command::new(helper)
                .arg(resources)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        )?;
        let mut stdin = group.take_stdin().expect("piped stdin");
        let stdout = group.take_stdout().expect("piped stdout");
        let writer = std::thread::spawn(move || -> std::io::Result<_> {
            stdin.write_all(&input)?;
            stdin.write_all(b"\n")?;
            stdin.flush()?;
            Ok(stdin)
        });
        let (tx, rx) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut line = Vec::new();
            let result = std::io::BufReader::new(stdout)
                .take(24 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
                .map(|_| line);
            let _ = tx.send(result);
        });
        let deadline = Instant::now() + timeout;
        let result = loop {
            if cancel.load(Ordering::Acquire) {
                break Err(anyhow::anyhow!("Diagram cancelled"));
            }
            if Instant::now() >= deadline {
                break Err(anyhow::anyhow!("Diagram timed out"));
            }
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(line)) => {
                    break (|| {
                        ensure!(
                            line.len() <= 24 * 1024 * 1024 && line.last() == Some(&b'\n'),
                            "Invalid diagram reply length"
                        );
                        Ok(serde_json::from_slice(&line)?)
                    })();
                }
                Ok(Err(error)) => break Err(error.into()),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err(anyhow::anyhow!("Diagram helper disconnected"));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        };
        // Always stop before joining: a stuck JS engine or blocked pipe owns no extra lifetime.
        let stopped = group.stop();
        let written = writer
            .join()
            .map_err(|_| anyhow::anyhow!("Diagram input worker panicked"))?;
        reader
            .join()
            .map_err(|_| anyhow::anyhow!("Diagram output worker panicked"))?;
        stopped?;
        if result.is_ok() {
            written?;
        }
        result
    }
}

pub mod runtime;
