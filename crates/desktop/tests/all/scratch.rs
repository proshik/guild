//! A test's own folder under `$TMPDIR`, gone when the test ends.
//!
//! **Not a topic, the one module here that is not.** Until 2026-09-24
//! every test named its folder `guild-<topic>-<name>-<pid>` by hand, many
//! never removed it, and the cleanup line at the end of the rest does
//! not run when an assertion above it fails: one green run left 94
//! folders, and the person's `$TMPDIR` held 28,562 entries. tempfile's
//! `TempDir` removes its folder when it is dropped, panics included, and
//! names it uniquely.
//!
//! **A helper hands out a `Scratch`, never a path.** A path outlives the
//! folder it names: a helper returning `folder.path().to_path_buf()`
//! drops the `TempDir` on its way out and the test starts on a folder
//! that is already gone. Holding a `Scratch` is holding the folder, and
//! it reads as a `Path` wherever one is asked for.

use std::ops::Deref;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub struct Scratch {
    path: PathBuf,
    _folder: TempDir,
}

impl Scratch {
    /// A fresh, empty folder named `<prefix><six random characters>`. The
    /// prefix is for a person reading `$TMPDIR` after a run was killed —
    /// the one way a folder is still left behind.
    pub fn new(prefix: &str) -> Self {
        let folder = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("a scratch folder");
        Self {
            path: folder.path().to_path_buf(),
            _folder: folder,
        }
    }

    /// A path nothing is at yet, inside a fresh folder: for the tests
    /// whose point is that the code under test makes it.
    pub fn absent(prefix: &str) -> Self {
        Self::new(prefix).within("absent")
    }

    /// The same folder, held, with the path one name further in — what
    /// `scratch(..).join(name)` looks like it does and does not: that
    /// drops the folder at the end of the statement and keeps a path
    /// into nothing, which the next `create_dir_all` quietly makes again
    /// where tempfile will never look for it.
    pub fn within(mut self, name: &str) -> Self {
        self.path.push(name);
        self
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
