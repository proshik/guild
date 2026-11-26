//! `f12` into a Kotlin or Java dependency — M8.11. A language server
//! answers with a `jar:` URI naming a compiled class, and what a person
//! wants to read is the source beside it.
//!
//! Out here because everything about it that can be wrong can be wrong
//! without a window: the URI, where the sources jar sits, and which entry
//! of it a class belongs to.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::jars;

/// A jar with the given entries, written where a test can find it.
fn jar(at: &Path, entries: &[(&str, &str)]) {
    std::fs::create_dir_all(at.parent().expect("a parent")).expect("the directory");
    let file = std::fs::File::create(at).expect("the jar");
    let mut zip = zip::ZipWriter::new(file);
    for (name, text) in entries {
        zip.start_file::<_, ()>(*name, zip::write::SimpleFileOptions::default())
            .expect("an entry");
        zip.write_all(text.as_bytes()).expect("the bytes");
    }
    zip.finish().expect("the archive");
}

fn temp(name: &str) -> Scratch {
    Scratch::new(&format!("guild-jars-{name}-"))
}

/// The URI the server answers with, in both spellings. `jar:file:///…`
/// is the protocol's own; `jar:///…` is what the person's own window
/// showed, and dropping either would be a jump that says nothing.
#[test]
fn a_jar_uri_is_an_archive_and_an_entry() {
    let one = jars::parse("jar:///Users/person/.gradle/telegram-6.1.0.jar!/com/github/Bot.class")
        .expect("the three-slash spelling");
    assert_eq!(
        one.jar,
        Path::new("/Users/person/.gradle/telegram-6.1.0.jar")
    );
    assert_eq!(one.inside, "com/github/Bot.class");

    let two = jars::parse("jar:file:///Users/person/x.jar!/a/B.class").expect("the file: spelling");
    assert_eq!(two.jar, Path::new("/Users/person/x.jar"));
    assert_eq!(two.inside, "a/B.class");

    // A space in a path arrives percent-encoded, as every URI does.
    let three = jars::parse("jar:///Users/person/my%20libs/x.jar!/a/B.class").expect("encoded");
    assert_eq!(three.jar, Path::new("/Users/person/my libs/x.jar"));

    assert_eq!(jars::parse("file:///Users/person/a.kt"), None, "not a jar");
    assert_eq!(jars::parse("jar:///Users/person/a.jar"), None, "no entry");
}

/// **Gradle keeps one directory per file**, so the sources jar is a
/// sibling directory over — measured on the person's own cache, where
/// `…/6.1.0/279eb2db…/telegram-6.1.0.jar` sits beside
/// `…/6.1.0/ec91c5b4…/telegram-6.1.0-sources.jar`. Maven keeps both in
/// one directory.
#[test]
fn the_sources_jar_is_found_in_both_layouts() {
    let dir = temp("layouts");
    let gradle = dir.join("caches/modules-2/files-2.1/g/telegram/6.1.0");
    jar(
        &gradle.join("279eb2db/telegram-6.1.0.jar"),
        &[("a/B.class", "")],
    );
    jar(
        &gradle.join("ec91c5b4/telegram-6.1.0-sources.jar"),
        &[("a/B.kt", "class B")],
    );
    assert_eq!(
        jars::sources_beside(&gradle.join("279eb2db/telegram-6.1.0.jar")),
        Some(gradle.join("ec91c5b4/telegram-6.1.0-sources.jar")),
        "Gradle: one directory over"
    );

    let maven = dir.join("repository/g/artifact/1.2.3");
    jar(&maven.join("artifact-1.2.3.jar"), &[("a/B.class", "")]);
    jar(
        &maven.join("artifact-1.2.3-sources.jar"),
        &[("a/B.java", "")],
    );
    assert_eq!(
        jars::sources_beside(&maven.join("artifact-1.2.3.jar")),
        Some(maven.join("artifact-1.2.3-sources.jar")),
        "Maven: the same directory"
    );

    let alone = dir.join("alone/x-1.0.jar");
    jar(&alone, &[("a/B.class", "")]);
    assert_eq!(
        jars::sources_beside(&alone),
        None,
        "a jar whose sources were never downloaded"
    );
}

/// A class names its source by name. An **inner class lives in its outer
/// class's file**, and Kotlin puts several classes — and `BotKt` — in one
/// `.kt`, so a name that is not there is looked for by its file name.
#[test]
fn a_class_is_opened_as_the_source_it_came_from() {
    let dir = temp("entries");
    let sources = dir.join("telegram-6.1.0-sources.jar");
    jar(
        &sources,
        &[
            ("com/github/Bot.kt", "class Bot\n"),
            ("com/github/util/Helpers.java", "class Helpers {}\n"),
            ("com/other/Renamed.kt", "class Moved\n"),
        ],
    );

    assert_eq!(
        jars::entry_for(&sources, "com/github/Bot.class").as_deref(),
        Some("com/github/Bot.kt")
    );
    assert_eq!(
        jars::entry_for(&sources, "com/github/Bot$Handler.class").as_deref(),
        Some("com/github/Bot.kt"),
        "an inner class is in its outer class's file"
    );
    assert_eq!(
        jars::entry_for(&sources, "com/github/util/Helpers.class").as_deref(),
        Some("com/github/util/Helpers.java")
    );
    // A class whose file is not where the package says: found by its
    // name, which is what Kotlin's `BotKt` and a moved file both need.
    assert_eq!(
        jars::entry_for(&sources, "com/github/Renamed.class").as_deref(),
        Some("com/other/Renamed.kt")
    );
    assert_eq!(jars::entry_for(&sources, "com/github/Absent.class"), None);
}

/// **A server may name the source itself**, not the class: kotlin-lsp
/// answers `references` with `…/feign-core-13.6-sources.jar!/feign/Contract.java`.
/// The entry is the file already, and looking for `Contract.java.java`
/// said *no sources downloaded* of a sources jar. Found by M8.15's window.
#[test]
fn a_source_named_inside_a_sources_jar_is_that_source() {
    let dir = temp("named-source");
    let sources = dir.join("feign-core-13.6-sources.jar");
    jar(
        &sources,
        &[("feign/Contract.java", "public interface Contract {}\n")],
    );
    assert_eq!(
        jars::entry_for(&sources, "feign/Contract.java").as_deref(),
        Some("feign/Contract.java")
    );
    let uri = format!("jar://{}!/feign/Contract.java", sources.display());
    let landed = jars::open_source(&uri, &dir.join("state")).expect("the source");
    assert_eq!(landed.relative, Path::new("feign/Contract.java"));
}

/// The whole jump: a URI in, a file on the disk out, under a root of its
/// own so that External Libraries can hold it and nothing is written into
/// the person's Gradle cache.
#[test]
fn a_uri_becomes_a_file_under_a_root_of_its_own() {
    let dir = temp("open");
    let gradle = dir.join("files-2.1/g/telegram/6.1.0");
    jar(&gradle.join("aaa/telegram-6.1.0.jar"), &[("a/B.class", "")]);
    jar(
        &gradle.join("bbb/telegram-6.1.0-sources.jar"),
        &[("com/github/Bot.kt", "class Bot\n")],
    );
    let uri = format!(
        "jar:{}!/com/github/Bot.class",
        gradle.join("aaa/telegram-6.1.0.jar").display()
    );

    let into = dir.join("state");
    let landed = jars::open_source(&format!("jar:{uri}").replace("jar:jar:", "jar:"), &into)
        .expect("the source");
    assert_eq!(landed.root, into.join("telegram-6.1.0"));
    assert_eq!(landed.relative, Path::new("com/github/Bot.kt"));
    assert_eq!(
        std::fs::read_to_string(landed.root.join(&landed.relative)).expect("the file"),
        "class Bot\n"
    );

    // Asked twice, it does not extract twice — and answers the same.
    let again = jars::open_source(&uri, &into).expect("the source again");
    assert_eq!(again.root, landed.root);
    assert_eq!(again.relative, landed.relative);
}

/// **The person's own jar**, by the URI their window showed. Skipped
/// where that cache is not on the machine: a check that names a real
/// directory has a precondition, and saying so is the rule this
/// repository learned from `target/`.
#[test]
fn the_persons_own_dependency_opens_its_source() {
    let cache = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".gradle/caches/modules-2/files-2.1")
        .join("io.github.kotlin-telegram-bot.kotlin-telegram-bot/telegram/6.1.0");
    if !cache.is_dir() {
        eprintln!("no such dependency in this machine's Gradle cache; nothing claimed");
        return;
    }
    let jar = std::fs::read_dir(&cache)
        .expect("the version's directories")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("telegram-6.1.0.jar"))
        .find(|candidate| candidate.is_file())
        .expect("the jar itself");

    // This optional host-cache case needs both physical inputs. Once sources
    // exist, opening/decoding them and finding Bot remain assertions below.
    if jars::sources_beside(&jar).is_none() {
        eprintln!(
            "PRECONDITION_ABSENT: matching sources archive is not in this machine's Gradle cache; real-cache source opening not exercised"
        );
        return;
    }

    let uri = format!(
        "jar://{}!/com/github/kotlintelegrambot/Bot.class",
        jar.display()
    );
    let into = temp("real");
    let landed = jars::open_source(&uri, &into).expect("Bot.kt out of the sources jar");
    let text = std::fs::read_to_string(landed.root.join(&landed.relative)).expect("the file");
    assert!(
        text.contains("class Bot"),
        "{} does not read like Bot.kt",
        landed.relative.display()
    );
}

/// **A whole sources jar is extracted once, beside what `f12` left, and
/// never outside its root** — M8.14. The directory is the one M8.11
/// opens single files in, so a library unfolded after an `f12` into it is
/// one root, not two; and a name reaching out with `../` is not written.
#[test]
fn a_sources_jar_is_extracted_whole_into_the_root_f12_uses() {
    let dir = temp("extract");
    let sources = dir.join("cache/telegram-6.1.0-sources.jar");
    jar(
        &sources,
        &[
            ("com/github/Bot.kt", "class Bot\n"),
            ("com/github/util/Helpers.kt", "object Helpers\n"),
            ("../escape.txt", "outside"),
        ],
    );
    let into = dir.join("state");
    let classes = dir.join("cache/telegram-6.1.0.jar");
    let root = jars::extracted_dir(&classes, &into).expect("a root");
    assert_eq!(
        root,
        jars::extracted_dir(&sources, &into).unwrap(),
        "the classes jar and its sources name one root"
    );
    assert_eq!(root, into.join("telegram-6.1.0"));

    // `f12` was here first and left one file.
    std::fs::create_dir_all(root.join("com/github")).unwrap();
    std::fs::write(root.join("com/github/Bot.kt"), "class Bot\n").unwrap();

    let extracted = jars::extract_all(&sources, &root).expect("extracted");
    assert_eq!(extracted, root);
    assert_eq!(
        std::fs::read_to_string(root.join("com/github/util/Helpers.kt")).unwrap(),
        "object Helpers\n",
        "what f12 had not reached is there now"
    );
    assert!(!into.join("escape.txt").exists() && !dir.join("escape.txt").exists());
    // The mark that it is whole is not a file of the library: the tree
    // would draw it as one.
    let inside: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(inside, ["com"], "nothing of ours inside the root");

    // Asked again, it is not read again: the archive may be gone.
    std::fs::remove_file(&sources).unwrap();
    assert_eq!(jars::extract_all(&sources, &root), Ok(root.clone()));
}
