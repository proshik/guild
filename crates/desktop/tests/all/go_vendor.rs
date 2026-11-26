//! Real offline Go fixtures; each discovery has its own subprocess environment.
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::scratch::Scratch;
use guild_desktop::deps::{self, Source, Stack};

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn go(root: &Path, args: &[&str]) {
    let output = Command::new("go")
        .args(args)
        .current_dir(root)
        .env("GOWORK", "off")
        .env("GOFLAGS", "")
        .env("GOPROXY", "off")
        .env("GOSUMDB", "off")
        .env("GOTOOLCHAIN", "local")
        .output()
        .expect("Go installed");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture() -> Scratch {
    let root = Scratch::new("guild-go-vendor-");
    write(&root, "dep/go.mod", "module example.com/dep\ngo 1.14\n");
    write(&root, "dep/dep.go", "package dep\n");
    write(
        &root,
        "app/go.mod",
        "module example.com/app\ngo 1.14\nrequire example.com/dep v1.0.0\nreplace example.com/dep => ../dep\n",
    );
    write(
        &root,
        "app/main.go",
        "package app\nimport _ \"example.com/dep\"\n",
    );
    go(&root.join("app"), &["mod", "vendor"]);
    root
}

fn discover(root: &Path, flags: &str, work: &str, expected: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "go_vendor::discovery_child", "--nocapture"])
        .env("GUILD_GO_FIXTURE", root)
        .env("GUILD_GO_EXPECT", expected)
        .env("GOWORK", work)
        .env("GOFLAGS", flags)
        .env("GOPROXY", "off")
        .env("GOSUMDB", "off")
        .env("GOTOOLCHAIN", "local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "flags={flags} work={work}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn discovery_child() {
    let Some(root) = std::env::var_os("GUILD_GO_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let scratch = Scratch::new("guild-go-answer-");
    let result = deps::ask::ask(&root, Stack::Go, &scratch);
    let expected = std::env::var("GUILD_GO_EXPECT").unwrap();
    if let Some(error) = expected.strip_prefix("error:") {
        assert!(
            result.as_ref().is_err_and(|why| why.contains(error)),
            "{result:?}"
        );
        return;
    }
    let answer = result.expect("Go discovery");
    if expected == "refresh" {
        verify_refresh(&root, &scratch, &answer);
        return;
    }
    assert_eq!(answer.dependencies.len(), 1);
    assert_eq!(answer.dependencies[0].label, "example.com/dep@v1.0.0");
    let expected = PathBuf::from(std::env::var_os("GUILD_GO_EXPECT").unwrap());
    let actual = answer.dependencies[0]
        .source
        .directory()
        .expect("source directory");
    assert_eq!(
        actual.canonicalize().unwrap(),
        expected.canonicalize().unwrap()
    );
    assert!(
        actual.join("dep.go").is_file(),
        "source navigation reaches the package"
    );
}

#[test]
fn effective_vendor_and_explicit_bypass() {
    let fixture = fixture();
    let app = fixture.join("app");
    let vendor = app.join("vendor/example.com/dep");
    discover(&app, "", "off", vendor.to_str().unwrap());
    discover(&app, "-mod=vendor", "off", vendor.to_str().unwrap());
    for flags in ["-mod=readonly", "-mod=mod"] {
        discover(&app, flags, "off", fixture.join("dep").to_str().unwrap());
    }
}

#[test]
fn go_cache_never_replays_a_previous_source_mode() {
    let scratch = Scratch::new("guild-go-cache-");
    write(
        &scratch,
        "go.answer",
        r#"{"Path":"example.com/dep","Version":"v1.0.0","Dir":"/old/source"}"#,
    );
    assert_eq!(deps::ask::cached(&scratch, Stack::Go, &scratch), None);
}

#[test]
fn ordinary_and_old_go_modules_do_not_select_vendor_by_presence() {
    let fixture = fixture();
    let app = fixture.join("app");
    let module = std::fs::read_to_string(app.join("go.mod")).unwrap();
    write(&app, "go.mod", &module.replace("go 1.14", "go 1.13"));
    discover(&app, "", "off", fixture.join("dep").to_str().unwrap());
    std::fs::remove_dir_all(app.join("vendor")).unwrap();
    discover(&app, "", "off", fixture.join("dep").to_str().unwrap());
}

#[test]
fn invalid_vendor_and_arbitrary_go_errors_are_not_bypassed() {
    let fixture = fixture();
    let app = fixture.join("app");
    let manifest = std::fs::read_to_string(app.join("vendor/modules.txt")).unwrap();
    write(
        &app,
        "vendor/modules.txt",
        &manifest.replace("v1.0.0", "v1.1.0"),
    );
    discover(&app, "", "off", "error:go mod vendor");
    std::fs::remove_file(app.join("vendor/modules.txt")).unwrap();
    discover(&app, "-mod=vendor", "off", "error:go mod vendor");
    write(&app, "go.mod", "not a module file\n");
    discover(&app, "", "off", "error:unknown directive");
    // Empty explicit vendor mode still requires readable metadata, even
    // though Go's consistency check has no requirements to compare.
    write(&app, "go.mod", "module example.com/app\ngo 1.14\n");
    discover(&app, "-mod=vendor", "off", "error:modules.txt");
}

#[test]
fn workspace_vendor_is_selected_over_module_local_vendor() {
    let fixture = fixture();
    let app = fixture.join("app");
    write(&fixture, "go.work", "go 1.22\nuse ./app\n");
    assert!(
        Stack::of(&fixture).contains(&Stack::Go),
        "workspace root is a Go project"
    );
    let work = fixture.join("go.work");
    let output = Command::new("go")
        .args(["work", "vendor"])
        .current_dir(&app)
        .env("GOWORK", &work)
        .env("GOFLAGS", "")
        .env("GOPROXY", "off")
        .env("GOSUMDB", "off")
        .env("GOTOOLCHAIN", "local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    write(&app, "vendor/modules.txt", "broken module-local metadata\n");
    discover(
        &app,
        "",
        work.to_str().unwrap(),
        fixture.join("vendor/example.com/dep").to_str().unwrap(),
    );
    discover(
        &fixture,
        "",
        work.to_str().unwrap(),
        fixture.join("vendor/example.com/dep").to_str().unwrap(),
    );
    discover(
        &app,
        "-mod=readonly",
        work.to_str().unwrap(),
        fixture.join("dep").to_str().unwrap(),
    );
}

#[test]
fn metadata_keeps_identities_replacements_and_package_ownership() {
    let root = Scratch::new("guild-go-metadata-");
    write(&root, "example.com/a/sub/sub.go", "package sub\n");
    let text = "# example.com/a v1.0.0 => ../local\n## explicit; go 1.14\n# example.com/a/sub v2.0.0 => example.com/fork v2.1.0\n## explicit\nexample.com/a/sub\n# example.com/a => ../local\n# example.com/a/sub v2.0.0 => example.com/fork v2.1.0\n# example.com/unused v1.0.0 => ../unused\n";
    let answer = deps::go_vendor::read(&root, text).unwrap();
    assert_eq!(
        answer.dependencies.len(),
        2,
        "replacement trailers are not libraries"
    );
    let parent = answer
        .dependencies
        .iter()
        .find(|d| d.label == "example.com/a@v1.0.0")
        .unwrap();
    assert_eq!(
        parent.source,
        Source::Missing,
        "the child module's directory is not the parent's sources"
    );
    let child = answer
        .dependencies
        .iter()
        .find(|d| d.label == "example.com/a/sub@v2.0.0")
        .unwrap();
    assert_eq!(
        child.source.directory(),
        Some(root.join("example.com/a/sub").as_path())
    );
}

#[test]
fn vendor_paths_reject_traversal_missing_packages_and_symlink_escape() {
    let root = Scratch::new("guild-go-invalid-");
    for text in [
        "garbage",
        "# ../escape v1.0.0\n../escape\n",
        "# /absolute v1.0.0\n",
        "# example.com/a v1.0.0\nexample.com/a/../escape\n",
        "# example.com/a v1.0.0\nexample.com/ab\n",
        "# example.com/a v1.0.0\nexample.com/a\n",
    ] {
        assert!(deps::go_vendor::read(&root, text).is_err(), "{text}");
    }
    let outside = Scratch::new("guild-go-outside-");
    std::fs::create_dir_all(root.join("example.com")).unwrap();
    std::os::unix::fs::symlink(&*outside, root.join("example.com/a")).unwrap();
    assert!(
        deps::go_vendor::read(&root, "# example.com/a v1.0.0\nexample.com/a\n")
            .unwrap_err()
            .contains("outside vendor")
    );
}

#[test]
fn watched_inputs_reach_the_refresh_predicate_for_ignored_and_external_vendor() {
    use guild_desktop::watch::{DependencyInputs, Watcher};
    use std::time::{Duration, Instant};
    let root = Scratch::new("guild-go-watch-");
    let outside = Scratch::new("guild-go-work-watch-");
    write(&root, ".gitignore", "vendor/\n");
    write(&root, "vendor/modules.txt", "old");
    write(&outside, "vendor/modules.txt", "old");
    let watcher = Watcher::start(&root);
    let inputs = DependencyInputs::read(vec![
        root.join("vendor/modules.txt"),
        outside.join("vendor/modules.txt"),
    ]);
    // A write between the worker's snapshot and watcher registration must
    // trigger a refresh as well as subsequent platform events.
    write(&root, "vendor/modules.txt", "new");
    watcher.track_dependencies(inputs);
    let await_change = || {
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            if watcher.try_recv().is_some_and(|batch| batch.dependencies()) {
                return;
            }
            assert!(
                Instant::now() < until,
                "metadata change never reached dependencies_heard's predicate"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    };
    await_change();
    for path in [
        root.join("vendor/modules.txt"),
        outside.join("vendor/modules.txt"),
    ] {
        // Let any platform event from the initial write drain first.
        std::thread::sleep(Duration::from_millis(300));
        while watcher.try_recv().is_some() {}
        std::fs::remove_file(&path).unwrap();
        await_change();
        std::fs::write(&path, "created").unwrap();
        await_change();
        let new = path.with_extension("tmp");
        std::fs::write(&new, "replace").unwrap();
        std::fs::rename(new, path).unwrap();
        await_change();
    }
    let mut batch = guild_desktop::watch::Batch::new(&ignore::gitignore::Gitignore::empty());
    batch.push(Path::new("vendor/modules.txt"));
    assert!(batch.dependencies());
    let mut nested = guild_desktop::watch::Batch::new(&ignore::gitignore::Gitignore::empty());
    nested.push(Path::new("sub/build.gradle.kts"));
    assert!(
        nested.dependencies(),
        "existing nested stack manifests still refresh"
    );
}

#[test]
fn vendor_browser_excludes_nested_modules_and_symlinks() {
    let root = Scratch::new("guild-go-navigation-");
    write(&root, "example.com/a/a.go", "package a\n");
    write(&root, "example.com/a/sub/sub.go", "package sub\n");
    write(&root, "example.com/a/helper/helper.go", "package helper\n");
    std::os::unix::fs::symlink("../../../../outside", root.join("example.com/a/escape")).unwrap();
    let answer = deps::go_vendor::read(&root, "# example.com/a v1.0.0\nexample.com/a\nexample.com/a/helper\n# example.com/a/sub v1.0.0\nexample.com/a/sub\n").unwrap();
    let parent = answer
        .dependencies
        .iter()
        .find(|d| d.label == "example.com/a@v1.0.0")
        .unwrap();
    let mut tree = parent.source.open_tree().unwrap();
    assert!(tree.reveal(Path::new("helper/helper.go")).is_some());
    assert!(tree.reveal(Path::new("sub/sub.go")).is_none());
    assert!(tree.reveal(Path::new("../escape")).is_none());
    let names: Vec<_> = tree.rows().iter().map(|r| r.path().to_path_buf()).collect();
    assert!(
        !names
            .iter()
            .any(|p| p.starts_with("sub") || p.starts_with("escape"))
    );
}

fn verify_refresh(root: &Path, scratch: &Path, initial: &deps::Answer) {
    use guild_desktop::watch::{DependencyInputs, Watcher};
    use std::time::{Duration, Instant};
    let watcher = Watcher::start(root);
    let inputs = deps::go_vendor::watched(root);
    let manifest = inputs
        .iter()
        .find(|p| p.ends_with("vendor/modules.txt") && p.is_file() && !p.starts_with(root))
        .unwrap();
    watcher.track_dependencies(DependencyInputs::read(inputs.clone()));
    let original = std::fs::read_to_string(manifest).unwrap();
    let refresh = || {
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            if watcher.try_recv().is_some_and(|batch| batch.dependencies()) {
                return deps::ask::ask(root, Stack::Go, scratch);
            }
            assert!(Instant::now() < until, "refresh event absent");
            std::thread::sleep(Duration::from_millis(30));
        }
    };
    std::fs::write(manifest, original.replace("v1.0.0", "v1.1.0")).unwrap();
    assert!(
        refresh().is_err(),
        "inconsistent vendor must replace the previous answer with a failure"
    );
    std::fs::write(manifest, original).unwrap();
    assert_eq!(refresh().unwrap(), *initial);
    let vendor = manifest.parent().unwrap();
    let moved = vendor.with_file_name("vendor.saved");
    std::fs::rename(vendor, &moved).unwrap();
    let module = refresh().unwrap();
    assert!(matches!(module.dependencies[0].source, Source::Dir(_)));
    assert_ne!(module, *initial);
    std::fs::rename(&moved, vendor).unwrap();
    assert_eq!(refresh().unwrap(), *initial);
    assert_eq!(deps::ask::cached(root, Stack::Go, scratch), None);
}

#[test]
fn real_workspace_metadata_refreshes_discovery_and_mode_without_cached_paths() {
    let fixture = fixture();
    let app = fixture.join("app");
    write(&fixture, "go.work", "go 1.22\nuse ./app\n");
    let work = fixture.join("go.work");
    let output = Command::new("go")
        .args(["work", "vendor"])
        .current_dir(&app)
        .env("GOWORK", &work)
        .env("GOFLAGS", "")
        .env("GOPROXY", "off")
        .env("GOSUMDB", "off")
        .env("GOTOOLCHAIN", "local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    write(&fixture, ".gitignore", "vendor/\n");
    discover(&app, "", work.to_str().unwrap(), "refresh");
}

#[test]
fn unused_version_specific_replacements_are_trailers_not_conflicting_modules() {
    let fixture = fixture();
    let app = fixture.join("app");
    write(
        &app,
        "go.mod",
        "module example.com/app\ngo 1.14\nrequire example.com/dep v1.0.0\nreplace example.com/dep v1.0.0 => ../dep\nreplace example.com/dep v1.1.0 => ../unused\n",
    );
    go(&app, &["mod", "vendor"]);
    discover(
        &app,
        "",
        "off",
        app.join("vendor/example.com/dep").to_str().unwrap(),
    );
}
