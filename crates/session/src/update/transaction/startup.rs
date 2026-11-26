//! Cache the loaded artifact before opening a window; never infer it at acknowledgement time.
use super::*;
/// One launched instance's startup context. Debug never prints acknowledgement secrets.
pub struct Startup {
    state: PathBuf,
    id: Uuid,
    nonce: Uuid,
    loaded_build: Uuid,
}
impl std::fmt::Debug for Startup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Startup")
            .field("transaction", &self.id)
            .field("loaded_build", &self.loaded_build)
            .finish_non_exhaustive()
    }
}
impl Startup {
    /// Capture before GPUI starts. Ordinary/manual/rollback launches have no context.
    pub fn capture(state: PathBuf) -> Result<Option<Self>, Failure> {
        let transaction = std::env::var_os("GUILD_UPDATE_TRANSACTION");
        let nonce = std::env::var_os("GUILD_UPDATE_NONCE");
        Self::read(
            state,
            transaction.as_deref(),
            nonce.as_deref(),
            &std::env::current_exe().map_err(|_| Failure::InvalidAck)?,
        )
    }
    fn read(
        state: PathBuf,
        transaction: Option<&std::ffi::OsStr>,
        nonce: Option<&std::ffi::OsStr>,
        executable: &Path,
    ) -> Result<Option<Self>, Failure> {
        let transaction = transaction.filter(|value| !value.is_empty());
        let nonce = nonce.filter(|value| !value.is_empty());
        if transaction.is_none() && nonce.is_none() {
            return Ok(None);
        }
        let parse = |value: Option<&std::ffi::OsStr>| -> Result<Uuid, Failure> {
            let id = value
                .and_then(|v| v.to_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .filter(|id| !id.is_nil())
                .ok_or(Failure::InvalidAck)?;
            Ok(id)
        };
        let id = parse(transaction)?;
        let nonce = parse(nonce)?;
        let macos = executable.parent().ok_or(Failure::InvalidAck)?;
        let contents = macos.parent().ok_or(Failure::InvalidAck)?;
        let app = contents.parent().ok_or(Failure::InvalidAck)?;
        if !state.is_absolute()
            || executable.file_name() != Some("guild-desktop".as_ref())
            || macos.file_name() != Some("MacOS".as_ref())
            || contents.file_name() != Some("Contents".as_ref())
            || app.extension() != Some("app".as_ref())
        {
            return Err(Failure::InvalidAck);
        }
        let loaded_build = bundle::info(app).map_err(|_| Failure::InvalidAck)?.build_id;
        Ok(Some(Self {
            state,
            id,
            nonce,
            loaded_build,
        }))
    }
    /// Execute only off-thread after actual first main-window rendering.
    pub fn acknowledge(self, cancel: &AtomicBool) -> Result<(), Failure> {
        Store::new(self.state).acknowledge(self.id, self.nonce, self.loaded_build, cancel)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn metadata(app: &Path, info: &crate::update::BuildInfo) {
        let resources = app.join("Contents/Resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(
            resources.join("build-info.json"),
            serde_json::to_vec(info).unwrap(),
        )
        .unwrap();
        std::fs::write(resources.join("commit"), format!("{}\n", info.commit)).unwrap();
        std::fs::write(resources.join("built"), info.built.to_string()).unwrap();
    }
    #[test]
    fn cached_startup_identity_cannot_turn_into_a_later_on_disk_replacement() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("Guild.app");
        let executable = app.join("Contents/MacOS/guild-desktop");
        let first = crate::update::BuildInfo::local("1".repeat(40));
        metadata(&app, &first);
        let id = Uuid::new_v4().to_string();
        let nonce = Uuid::new_v4().to_string();
        let startup = Startup::read(
            root.path().join("state"),
            Some(id.as_ref()),
            Some(nonce.as_ref()),
            &executable,
        )
        .unwrap()
        .unwrap();
        let next = crate::update::BuildInfo::local("2".repeat(40));
        metadata(&app, &next);
        assert_eq!(startup.loaded_build, first.build_id);
        assert_ne!(startup.loaded_build, bundle::info(&app).unwrap().build_id);
        assert!(
            Startup::read(root.path().join("state"), None, None, &executable)
                .unwrap()
                .is_none()
        );
        assert!(
            Startup::read(
                root.path().join("state"),
                Some(id.as_ref()),
                None,
                &executable
            )
            .is_err()
        );
        assert!(
            Startup::read(
                root.path().join("state"),
                Some("bad".as_ref()),
                Some(nonce.as_ref()),
                &executable
            )
            .is_err()
        );
        assert!(
            Startup::read(
                root.path().join("state"),
                Some("".as_ref()),
                Some("".as_ref()),
                &executable
            )
            .unwrap()
            .is_none()
        );
    }
}
