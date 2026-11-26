//! `f12` out of a Swift project: where what sourcekit-lsp answers lands.
//! 2026-09-28.
//!
//! Measured in an Xcode project, it answers in three shapes, and
//! the rule every other language follows — the nearest ancestor holding
//! the language's manifest, `highlight::dependency_root` — reaches only
//! the third:
//!
//! - **an SDK module's interface**, printed as Swift into the system's
//!   temporary directory under a hash, `…/GeneratedInterfaces/
//!   909807708512964116/Swift.Misc.swiftinterface`. A root there would be
//!   a node in External Libraries called by a number, gone whenever the
//!   system empties its temporary files — so the file is copied the way a
//!   jar's source is, M8.11: under Guild's own state, one root per module,
//!   `sources/Swift/`;
//! - **an Objective-C framework's header**, where Xcode built it,
//!   `…/Build/Products/Debug/Sparkle.framework/Versions/B/Headers/
//!   SPUUpdaterDelegate.h`. A C header has no manifest above it; the
//!   framework is the directory a person calls by name, and the root;
//! - **a Swift package's source** under its checkout, which holds
//!   `Package.swift` — the language table's manifest for Swift.
//!
//! All three are read only, as every dependency's file is, which is what
//! AppCode showed for the same jumps.

use std::path::{Path, PathBuf};

use crate::jars::Landed;

/// The suffix of a module interface, generated or shipped.
const SUFFIX: &str = "swiftinterface";

/// A module interface in, a copy under the module's root out.
///
/// **Copied on every jump**, not once as a jar's entry is: the position
/// the server answers with is a line of the file it has just written, and
/// a copy kept from an older SDK would put the caret elsewhere.
///
/// `None` for anything that is not an interface, or not there.
pub fn interface(file: &Path, into: &Path) -> Option<Landed> {
    if file.extension()?.to_str()? != SUFFIX {
        return None;
    }
    let name = file.file_name()?;
    // `Swift.Misc` is a group of the module `Swift`; `Foundation` is the
    // module whole.
    let module = name.to_str()?.split('.').next()?;
    let root = into.join(module);
    std::fs::create_dir_all(&root).ok()?;
    std::fs::copy(file, root.join(name)).ok()?;
    Some(Landed {
        root,
        relative: name.into(),
    })
}

/// The framework a file is inside — the nearest ancestor named
/// `*.framework` — or `None`.
///
/// Read off the path, not the disk: the server has just named the file,
/// and a bundle is a directory by its name. The file itself is skipped —
/// it is not its own root, whatever it is called.
pub fn framework_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.extension().is_some_and(|suffix| suffix == "framework"))
        .map(Path::to_path_buf)
}

/// What `Set up for Xcode` passes to `xcode-build-server`: `config` bound
/// to the project's one workspace, or to its one project where there is
/// no workspace — the tool's own two spellings. It writes
/// `buildServer.json` into the directory it is run in, the project's root,
/// which is what the language table's row needs to start sourcekit-lsp.
///
/// **A workspace wins**: a CocoaPods project builds only through it. The
/// workspace every `.xcodeproj` carries inside itself is not looked at —
/// only the root's own entries are. `None` where there is none, and where
/// there are two of the kind that would be chosen: that choice is the
/// person's, and the tool would refuse it too.
pub fn xcode_config(root: &Path) -> Option<Vec<String>> {
    let mut workspaces = Vec::new();
    let mut projects = Vec::new();
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        match Path::new(&name)
            .extension()
            .and_then(|suffix| suffix.to_str())
        {
            Some("xcworkspace") => workspaces.push(name),
            Some("xcodeproj") => projects.push(name),
            _ => {}
        }
    }
    let (flag, found) = match (workspaces.as_slice(), projects.as_slice()) {
        ([], []) => return None,
        ([], projects) => ("-project", projects),
        (workspaces, _) => ("-workspace", workspaces),
    };
    match found {
        [one] => Some(vec!["config".to_owned(), flag.to_owned(), one.clone()]),
        _ => None,
    }
}

/// Where Xcode is, when the bridge has to be told.
///
/// `xcodebuild`, which `xcode-build-server config` runs, refuses the
/// command line tools — *tool 'xcodebuild' requires Xcode, but active
/// developer directory … is a command line tools instance*, measured on
/// 2026-09-28 — and `xcode-select` names them on a machine where Xcode.app
/// is installed beside them. `selected` is what `xcode-select -p` said and
/// `xcode` is Xcode.app's developer directory; the answer is `xcode` only
/// when the tools are selected and it is there. A person who selected an
/// Xcode, a beta among them, is left with it.
pub fn developer_dir(selected: &Path, xcode: &Path) -> Option<PathBuf> {
    let tools = selected
        .components()
        .any(|part| part.as_os_str() == "CommandLineTools");
    (tools && xcode.is_dir()).then(|| xcode.to_path_buf())
}
