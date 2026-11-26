//! `f12` out of a Swift project. Measured in an Xcode project on 2026-09-28,
//! sourcekit-lsp answers in three shapes, and what a person wants in each
//! is the file under a name they recognise:
//!
//! - an SDK module's interface it has just generated under the system's
//!   temporary directory — `…/T/sourcekit-lsp/GeneratedInterfaces/
//!   909807708512964116/Swift.Misc.swiftinterface`;
//! - an Objective-C framework's header where Xcode built it —
//!   `…/Build/Products/Debug/Sparkle.framework/Versions/B/Headers/
//!   SPUUpdaterDelegate.h`;
//! - a Swift package's own source, under the checkout that holds its
//!   `Package.swift`.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::highlight::Language;
use guild_desktop::sourcekit;

/// A generated interface, where sourcekit-lsp leaves one.
fn generated(dir: &Path, name: &str, text: &str) -> PathBuf {
    let at = dir.join("sourcekit-lsp/GeneratedInterfaces/909807708512964116");
    std::fs::create_dir_all(&at).expect("the directory");
    let file = at.join(name);
    std::fs::write(&file, text).expect("the interface");
    file
}

/// **Copied under the module's name**, as a jar's source is copied under
/// the archive's: the directory sourcekit-lsp chose is a hash, and the
/// system empties it when it likes.
#[test]
fn an_interface_lands_under_its_module() {
    let temp = Scratch::new("guild-swift-temp-");
    let into = Scratch::new("guild-swift-sources-");
    let text = "public func print(_ items: Any...)\n";
    let file = generated(&temp, "Swift.Misc.swiftinterface", text);

    let landed = sourcekit::interface(&file, &into).expect("an interface");
    assert_eq!(landed.root, into.join("Swift"));
    assert_eq!(landed.relative, Path::new("Swift.Misc.swiftinterface"));
    assert_eq!(
        std::fs::read_to_string(landed.root.join(&landed.relative)).expect("the copy"),
        text
    );

    // A framework's interface is one file, named by the module alone.
    let file = generated(&temp, "Foundation.swiftinterface", "import Swift\n");
    let landed = sourcekit::interface(&file, &into).expect("an interface");
    assert_eq!(landed.root, into.join("Foundation"));
}

/// **Copied again on every jump.** The position the server answers with
/// is a line of the file it has just written; a copy kept from an older
/// SDK would put the caret on some other declaration.
#[test]
fn an_interface_is_copied_again_every_time() {
    let temp = Scratch::new("guild-swift-temp-");
    let into = Scratch::new("guild-swift-sources-");
    let file = generated(&temp, "Swift.Misc.swiftinterface", "old\n");
    sourcekit::interface(&file, &into).expect("an interface");

    std::fs::write(&file, "new\n").expect("regenerated");
    let landed = sourcekit::interface(&file, &into).expect("an interface");
    assert_eq!(
        std::fs::read_to_string(landed.root.join(&landed.relative)).expect("the copy"),
        "new\n"
    );
}

/// Only an interface: any other file outside the project goes the way
/// it went before, and a file that is not there lands nowhere.
#[test]
fn only_an_interface_is_taken() {
    let temp = Scratch::new("guild-swift-temp-");
    let into = Scratch::new("guild-swift-sources-");
    let other = generated(&temp, "main.swift", "print(1)\n");
    assert_eq!(sourcekit::interface(&other, &into), None);
    assert_eq!(
        sourcekit::interface(&temp.join("Gone.swiftinterface"), &into),
        None
    );
}

/// The copy is read as Swift: an interface is Swift's own syntax, and
/// the tab would be grey otherwise.
#[test]
fn an_interface_is_swift() {
    let language = Language::of(Path::new("Swift.Misc.swiftinterface")).expect("in the table");
    assert_eq!(language.name, "swift");
}

/// **The copy is a row the Explorer can reveal.** The tab in front is
/// followed into External Libraries since 2026-09-28, and what it lights
/// is the root's own tree: the module's row, then the interface under it.
#[test]
fn an_interface_is_revealed_under_its_module() {
    let temp = Scratch::new("guild-swift-temp-");
    let into = Scratch::new("guild-swift-sources-");
    let file = generated(&temp, "Swift.Misc.swiftinterface", "public func print()\n");
    let landed = sourcekit::interface(&file, &into).expect("an interface");

    let mut tree = guild_desktop::tree::Tree::open(&landed.root);
    assert_eq!(tree.reveal(&landed.relative), Some(1));
    assert_eq!(tree.rows()[0].label(), "Swift");
}

/// **A framework's header lands under the framework**, the directory a
/// person calls *Sparkle.framework*: a C header has no manifest above it,
/// so the rule every other language uses finds nothing.
#[test]
fn a_header_belongs_to_the_framework_around_it() {
    let products = Path::new(
        "/Users/person/Library/Developer/Xcode/DerivedData/Atlas-fmpv/Build/Products/Debug",
    );
    let header = products.join("Sparkle.framework/Versions/B/Headers/SPUUpdaterDelegate.h");
    assert_eq!(
        sourcekit::framework_root(&header),
        Some(products.join("Sparkle.framework"))
    );

    // A header outside any framework is left to the rules it had.
    assert_eq!(
        sourcekit::framework_root(Path::new("/usr/include/stdio.h")),
        None
    );
    // The file itself is not its own root, whatever it is called.
    assert_eq!(
        sourcekit::framework_root(Path::new("/tmp/odd.framework")),
        None
    );
}

/// **A Swift package's source lands under its checkout**, found by the
/// rule every language with a manifest follows — `Package.swift` is
/// Swift's.
#[test]
fn a_package_s_source_belongs_to_its_checkout() {
    let dir = Scratch::new("guild-swiftpm-checkout-");
    let checkout = dir.join("SourcePackages/checkouts/swift-argument-parser");
    let file = checkout.join("Sources/ArgumentParser/Parsable.swift");
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
    std::fs::write(checkout.join("Package.swift"), "").expect("a manifest");
    std::fs::write(&file, "").expect("a source");

    let swift = Language::of(Path::new("main.swift")).expect("swift is in the table");
    assert_eq!(
        guild_desktop::highlight::dependency_root(&file, swift),
        Some(checkout)
    );
}

/// **What `Set up for Xcode` runs**: `xcode-build-server config` bound to
/// the project's one workspace, or to its one project where there is no
/// workspace — the tool's own two spellings. A workspace wins because a
/// CocoaPods project builds only through it, and the workspace every
/// `.xcodeproj` carries inside itself is not the project's.
#[test]
fn an_xcode_project_is_bound_by_its_workspace_or_its_project() {
    let project = Scratch::new("guild-xcode-project-");
    std::fs::create_dir_all(project.join("App.xcodeproj/project.xcworkspace")).expect("a project");
    assert_eq!(
        sourcekit::xcode_config(&project),
        Some(vec![
            "config".to_owned(),
            "-project".to_owned(),
            "App.xcodeproj".to_owned()
        ])
    );

    std::fs::create_dir_all(project.join("App.xcworkspace")).expect("a workspace");
    assert_eq!(
        sourcekit::xcode_config(&project),
        Some(vec![
            "config".to_owned(),
            "-workspace".to_owned(),
            "App.xcworkspace".to_owned()
        ])
    );

    // Two of a kind is a choice the window cannot make for the person,
    // and neither is a project with none.
    std::fs::create_dir_all(project.join("Other.xcworkspace")).expect("a second one");
    assert_eq!(sourcekit::xcode_config(&project), None);
    let package = Scratch::new("guild-xcode-none-");
    std::fs::write(package.join("Package.swift"), "").expect("a manifest");
    assert_eq!(sourcekit::xcode_config(&package), None);
}

/// **`DEVELOPER_DIR` only where `xcode-select` names the command line
/// tools.** `xcodebuild`, which the bridge runs, refuses them — measured
/// on 2026-09-28 — and Xcode.app is beside them; a person who selected an
/// Xcode, a beta among them, is left with it.
#[test]
fn xcode_is_named_only_when_the_command_line_tools_are_selected() {
    let apps = Scratch::new("guild-xcode-app-");
    let xcode = apps.join("Xcode.app/Contents/Developer");
    let tools = Path::new("/Library/Developer/CommandLineTools");

    assert_eq!(
        sourcekit::developer_dir(tools, &xcode),
        None,
        "no Xcode.app"
    );
    std::fs::create_dir_all(&xcode).expect("an Xcode");
    assert_eq!(sourcekit::developer_dir(tools, &xcode), Some(xcode.clone()));
    assert_eq!(
        sourcekit::developer_dir(
            Path::new("/Applications/Xcode-beta.app/Contents/Developer"),
            &xcode
        ),
        None
    );
}
