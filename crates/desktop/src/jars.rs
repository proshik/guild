//! `f12` into a Kotlin or Java dependency: a `jar:` URI, and the source
//! beside the class it names. M8.11.
//!
//! **Kotlin and Java ship their dependencies compiled**, so a language
//! server answers a definition with
//! `jar:///…/telegram-6.1.0.jar!/com/github/Bot.class` — an entry of an
//! archive, and bytecode at that. M8.5 said so and stopped, because Guild
//! decompiles nothing.
//!
//! **What that missed is that the sources are usually right there.**
//! Gradle keeps one directory per file, so
//! `…/6.1.0/279eb2db…/telegram-6.1.0.jar` has
//! `…/6.1.0/ec91c5b4…/telegram-6.1.0-sources.jar` a sibling directory
//! over; Maven keeps both in one directory. So the jump is not a
//! decompiler, it is a zip — and where there is no sources jar the window
//! says that, which is the true thing to say.
//!
//! Nothing here writes into the person's cache: an entry is copied under
//! Guild's own state directory, one root per archive, and External
//! Libraries holds that root like any other dependency's — M8.8.3.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// A `jar:` URI taken apart: the archive, and the entry inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The archive on the disk.
    pub jar: PathBuf,
    /// The path inside it, as the archive spells it.
    pub inside: String,
}

/// Where an entry landed: a root of its own, and the file within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    /// The directory the archive was opened under — a library root.
    pub root: PathBuf,
    /// The file inside it, as every file tab's path is.
    pub relative: PathBuf,
}

/// A `jar:` URI taken apart, in both spellings a server may use.
///
/// `jar:file:///…!/…` is the protocol's own and `jar:///…!/…` is what the
/// person's window showed; reading one and not the other is a jump that
/// says nothing, in the half of the cases nobody tried.
pub fn parse(uri: &str) -> Option<Entry> {
    let rest = uri.strip_prefix("jar:")?;
    let rest = rest
        .strip_prefix("file://")
        .or_else(|| rest.strip_prefix("file:"))
        .unwrap_or(rest);
    let (archive, inside) = rest.split_once("!/")?;
    if inside.is_empty() {
        return None;
    }
    let archive = crate::lsp::path_of(&format!("file://{archive}"))?;
    Some(Entry {
        jar: archive,
        inside: inside.to_owned(),
    })
}

/// The sources archive of a jar, where one has been downloaded.
///
/// Two layouts, because two build tools: Maven keeps every file of a
/// version in one directory, Gradle keeps one directory per file under
/// the version's — so the sibling directories are looked through as well.
pub fn sources_beside(jar: &Path) -> Option<PathBuf> {
    let name = jar.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jar")?;
    if stem.ends_with("-sources") {
        return Some(jar.to_path_buf());
    }
    let wanted = format!("{stem}-sources.jar");
    let dir = jar.parent()?;

    let beside = dir.join(&wanted);
    if beside.is_file() {
        return Some(beside);
    }
    // Gradle: `…/<version>/<sha1>/<file>`, one directory per file.
    let over = dir.parent()?;
    std::fs::read_dir(over)
        .ok()?
        .filter_map(Result::ok)
        .map(|sibling| sibling.path().join(&wanted))
        .find(|candidate| candidate.is_file())
}

/// Which entry of a sources archive a class came from.
///
/// A class names its file: `com/foo/Bot.class` is `Bot.kt` or `Bot.java`
/// beside it. **An inner class is in its outer class's file** —
/// `Bot$Handler.class` — so the `$` and what follows are dropped. And
/// where the package says one thing and the archive another, which is
/// what Kotlin's `BotKt` and a moved file both look like, the entries are
/// scanned for the file name.
pub fn entry_for(sources: &Path, class: &str) -> Option<String> {
    let names = source_names(class);
    let file = std::fs::File::open(sources).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    for name in &names {
        if archive.by_name(name).is_ok() {
            return Some(name.clone());
        }
    }
    // By file name, wherever it sits.
    let tails: Vec<String> = names
        .iter()
        .filter_map(|name| name.rsplit('/').next().map(|tail| format!("/{tail}")))
        .collect();
    (0..archive.len())
        .filter_map(|at| {
            archive
                .by_index(at)
                .ok()
                .map(|entry| entry.name().to_owned())
        })
        .find(|name| tails.iter().any(|tail| name.ends_with(tail.as_str())))
}

/// The source file names a class may have come from, in the order to try.
///
/// **A name that is a source already is itself**: a server that answers
/// from inside a sources jar names `feign/Contract.java`, and M8.15 found
/// that turned into `Contract.java.java`.
fn source_names(class: &str) -> Vec<String> {
    if [".kt", ".java"]
        .iter()
        .any(|suffix| class.ends_with(suffix))
    {
        return vec![class.to_owned()];
    }
    let path = class.strip_suffix(".class").unwrap_or(class);
    let path = match path.split_once('$') {
        Some((outer, _)) => outer,
        None => path,
    };
    ["kt", "java"]
        .iter()
        .map(|suffix| format!("{path}.{suffix}"))
        .collect()
}

/// The whole jump: a URI in, a file on the disk out.
///
/// `None` where the archive has no sources beside it — the window then
/// says that, rather than opening bytecode nobody can read.
pub fn open_source(uri: &str, into: &Path) -> Option<Landed> {
    let entry = parse(uri)?;
    let sources = sources_beside(&entry.jar)?;
    let inside = entry_for(&sources, &entry.inside)?;

    let stem = sources
        .file_name()?
        .to_str()?
        .strip_suffix("-sources.jar")
        .unwrap_or("sources");
    let root = into.join(stem);
    let landed = Landed {
        root: root.clone(),
        relative: PathBuf::from(&inside),
    };
    let at = root.join(&inside);
    // Extracted once. A second `f12` at the same place is a tab that
    // opens, not an archive that is read again.
    if at.is_file() {
        return Some(landed);
    }

    let file = std::fs::File::open(&sources).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut text = Vec::new();
    archive.by_name(&inside).ok()?.read_to_end(&mut text).ok()?;
    std::fs::create_dir_all(at.parent()?).ok()?;
    std::fs::write(&at, text).ok()?;
    Some(landed)
}

/// Where archives are opened: Guild's own state, never the person's cache.
pub fn sources_dir() -> PathBuf {
    crate::paths::state_dir().join("sources")
}

/// Where a jar's sources are opened — the root `open_source` already uses
/// for it, so a whole extraction and the files `f12` extracted one at a
/// time are one directory. M8.14.
pub fn extracted_dir(jar: &Path, into: &Path) -> Option<PathBuf> {
    let stem = jar.file_stem()?.to_str()?;
    Some(into.join(stem.strip_suffix("-sources").unwrap_or(stem)))
}

/// The mark that an archive was extracted whole. `f12` leaves single files
/// in the same directory, so a directory that exists is not a directory
/// that is complete. **Beside the root, never in it**: inside, the tree
/// drew it as a file of the library — found in the window.
fn complete_mark(root: &Path) -> PathBuf {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    root.with_file_name(format!("{name}.extracted"))
}

/// Extract a whole archive of sources into `root`, once — M8.14, when a
/// library's row is first unfolded. Measured: Guava's sources jar in 60
/// ms, a JDK's `src.zip` in 1.6 s and 218 MB, which is why the caller
/// does this on a thread.
///
/// **A name is taken only if it stays inside `root`** — `enclosed_name`
/// refuses `../` and an absolute path, the zip-slip every extractor has
/// to refuse.
pub fn extract_all(archive: &Path, root: &Path) -> Result<PathBuf, String> {
    let mark = complete_mark(root);
    if mark.is_file() {
        return Ok(root.to_path_buf());
    }
    let file = std::fs::File::open(archive).map_err(|err| err.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|err| err.to_string())?;
    for at in 0..zip.len() {
        let mut entry = zip.by_index(at).map_err(|err| err.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let path = root.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(|err| err.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|err| err.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|err| err.to_string())?;
    }
    std::fs::create_dir_all(root).map_err(|err| err.to_string())?;
    std::fs::write(&mark, "").map_err(|err| err.to_string())?;
    Ok(root.to_path_buf())
}
