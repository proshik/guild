//! What the platform does that GPUI does not expose.
//!
//! Native window gestures and application services. Fullscreen title-bar reveal
//! belongs to AppKit: do not hide its traffic-light container during rendering.

// The `objc` 0.2 macros expand a `feature = "cargo-clippy"` check that
// modern rustc no longer knows. It comes from the macro, not from us, and
// there is nothing here to fix.
#![allow(unexpected_cfgs)]

use std::ffi::{CStr, CString, c_char};
use std::sync::atomic::{AtomicBool, Ordering};

use objc::runtime::{BOOL, Class, NO, Object, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The `NSWindow` behind a GPUI window, if it can be reached.
///
/// The raw view handle and its owning window are checked before use.
fn ns_window(window: &gpui::Window) -> Option<*mut Object> {
    // Through the trait explicitly: `Window` has an inherent
    // `window_handle` of its own — GPUI's own handle type — and it wins
    // over the trait method that yields the platform pointer.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };

    let view = appkit.ns_view.as_ptr().cast::<Object>();
    if view.is_null() {
        return None;
    }

    // SAFETY: a non-null `NSView` AppKit handed GPUI, and `window` is its
    // documented accessor.
    let ns_window: *mut Object = unsafe { msg_send![view, window] };
    (!ns_window.is_null()).then_some(ns_window)
}

/// Native CGSize/NSSize on 64-bit macOS, used only through documented NSWindow APIs.
#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
struct NativeSize {
    width: f64,
    height: f64,
}

// SAFETY: repr(C), two CGFloat doubles, matching AppKit's NSSize alias.
unsafe impl objc::Encode for NativeSize {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGSize=dd}") }
    }
}

/// GPUI exposes the initial content minimum, but no setter for later UI-scale changes.
pub fn set_minimum_content_size(window: &gpui::Window, size: gpui::Size<gpui::Pixels>) {
    let Some(native) = ns_window(window) else {
        return;
    };
    let want = NativeSize {
        width: f64::from(f32::from(size.width)),
        height: f64::from(f32::from(size.height)),
    };
    // SAFETY: a checked live NSWindow, called while rendering on the main thread.
    // Read first to avoid layout callbacks when the minimum has not changed.
    unsafe {
        let current: NativeSize = msg_send![native, contentMinSize];
        if current != want {
            let _: () = msg_send![native, setContentMinSize: want];
        }
    }
}

/// What a double-click on a title bar should do.
///
/// The person chooses this in System Settings, under Desktop & Dock. A
/// window with the system's own title bar obeys it for free; Guild draws
/// its own, so nothing obeys it unless we ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleClick {
    /// Fill the screen, keeping the menu bar and the Dock. The default,
    /// and **not** the same as full screen — that is `ctrl-cmd-f`.
    Zoom,
    /// Send the window to the Dock.
    Minimize,
    /// Nothing at all, which some people deliberately choose.
    Nothing,
}

/// Read the choice from the system.
///
/// Anything unexpected means the default. This decides what a gesture
/// does, and guessing wrong in the direction of "do nothing" is better
/// than guessing wrong in the direction of "minimise the window a person
/// was working in".
pub fn double_click_action() -> DoubleClick {
    // SAFETY: `standardUserDefaults` and `stringForKey:` are the
    // documented API, every pointer is checked before use, and the call is
    // made from the main thread while rendering.
    unsafe {
        let defaults: *mut Object = msg_send![class!(NSUserDefaults), standardUserDefaults];
        if defaults.is_null() {
            return DoubleClick::Zoom;
        }

        let Ok(name) = CString::new(SETTING) else {
            return DoubleClick::Zoom;
        };
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
        if key.is_null() {
            return DoubleClick::Zoom;
        }

        let value: *mut Object = msg_send![defaults, stringForKey: key];
        if value.is_null() {
            // Never set. macOS's own default is Maximize.
            return action_of(None);
        }

        let utf8: *const c_char = msg_send![value, UTF8String];
        if utf8.is_null() {
            return DoubleClick::Zoom;
        }

        action_of(CStr::from_ptr(utf8).to_str().ok())
    }
}

/// The key macOS keeps the choice under.
const SETTING: &str = "AppleActionOnDoubleClick";

/// What a value of that setting means.
///
/// Pure, and separate from the Objective-C above, so the mapping can be
/// checked without a window — and it is the half that can be wrong.
fn action_of(value: Option<&str>) -> DoubleClick {
    match value {
        Some("Minimize") => DoubleClick::Minimize,
        Some("None") => DoubleClick::Nothing,
        // "Maximize", and anything unrecognised or unset. macOS ships with
        // this unset and behaving as Maximize.
        _ => DoubleClick::Zoom,
    }
}

/// Whether a precise scrolling delta follows the fingers.
///
/// GPUI forwards AppKit's `scrollingDeltaX` unchanged, so the swipe detector
/// needs the preference in order to give the same physical gesture the same
/// workspace direction in both system modes.
pub fn natural_scrolling() -> bool {
    // SAFETY: `standardUserDefaults`, `objectForKey:` and `boolValue` are
    // documented Foundation APIs. Every Objective-C pointer is checked.
    unsafe {
        let defaults: *mut Object = msg_send![class!(NSUserDefaults), standardUserDefaults];
        if defaults.is_null() {
            return natural_of(None);
        }
        let Ok(name) = CString::new("com.apple.swipescrolldirection") else {
            return natural_of(None);
        };
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
        if key.is_null() {
            return natural_of(None);
        }
        let value: *mut Object = msg_send![defaults, objectForKey: key];
        if value.is_null() {
            return natural_of(None);
        }
        let enabled: BOOL = msg_send![value, boolValue];
        natural_of(Some(i64::from(enabled)))
    }
}

/// Pure mapping of macOS's preference; an absent value means its default.
pub fn natural_of(value: Option<i64>) -> bool {
    value != Some(0)
}

/// Whether the primary mouse button is held down right now — M12.1.
///
/// **A release can go missing**, and a clock that scrolls a selection held
/// past the terminal's edge must not trust one to arrive: let go outside
/// the window and nothing of the window's may hear it. `NSEvent
/// pressedMouseButtons` is the platform's own answer, asked at the moment,
/// and bit zero is the primary button.
pub fn primary_button_down() -> bool {
    // SAFETY: `pressedMouseButtons` is a documented class property of
    // `NSEvent` returning an `NSUInteger`, and it takes no receiver that
    // could be null.
    unsafe {
        let pressed: usize = msg_send![class!(NSEvent), pressedMouseButtons];
        pressed & 1 == 1
    }
}

/// Put a number on the application's Dock icon, or take it off — M5.10.
///
/// The number is how many agents need the person: blocked on them, or
/// finished a turn nobody has read. `NSApp.dockTile.setBadgeLabel:` is
/// the whole of it, and GPUI does not expose it, which is why it lives
/// here with the rest of what touches AppKit.
pub fn set_dock_badge(count: usize) {
    // SAFETY: `sharedApplication`, `dockTile` and `setBadgeLabel:` are the
    // documented API, every pointer is checked before it is used, and the
    // call is made on the main thread — from the watch loop, which runs on
    // GPUI's foreground executor.
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let tile: *mut Object = msg_send![app, dockTile];
        if tile.is_null() {
            return;
        }
        let label: *mut Object = match badge_label(count) {
            Some(text) => {
                let Ok(text) = CString::new(text) else {
                    return;
                };
                let label: *mut Object =
                    msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
                if label.is_null() {
                    return;
                }
                label
            }
            // `nil` takes the badge off.
            None => std::ptr::null_mut(),
        };
        let _: () = msg_send![tile, setBadgeLabel: label];
    }
}

/// Play a named macOS system sound on the main thread. Missing sounds are silent.
pub fn play_sound(name: &str) {
    let Ok(name) = CString::new(name) else {
        return;
    };
    // SAFETY: documented NSSound API, called by the real window's foreground
    // status loop. Every returned object is checked before sending a message.
    unsafe {
        let name: *mut Object = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
        if name.is_null() {
            return;
        }
        let sound: *mut Object = msg_send![class!(NSSound), soundNamed: name];
        if sound.is_null() {
            return;
        }
        restart_sound(sound);
    }
}

/// The caller supplies a live NSSound-compatible object on the main thread.
unsafe fn restart_sound(sound: *mut Object) {
    // NSSound caches named instances and refuses play while already playing.
    // A new status pass must restart Glass, not silently lose its completion.
    // SAFETY: the caller checked the object; stop/play are documented sound API.
    unsafe {
        let _: objc::runtime::BOOL = msg_send![sound, stop];
        let _: objc::runtime::BOOL = msg_send![sound, play];
    }
}

/// What the Dock's badge says for `count`. Pure, for the reason
/// `action_of` is: the half that can be wrong is checked without AppKit.
/// Nothing at zero — a badge saying *0* is a red mark saying all is well.
fn badge_label(count: usize) -> Option<String> {
    (count > 0).then(|| count.to_string())
}

/// Move paths to the Trash.
///
/// **The one gesture in the window that can lose a person's work, and
/// the reason it is not `unlink`.** There is no local history behind
/// Delete here, so a file removed here has to be
/// recoverable by a gesture every macOS user already knows. Decision 13
/// of M8.8.
///
/// A slice rather than one path because the callers differ: the tree
/// deletes the row under the pointer, and the Commit panel of M8.8.4
/// deletes a selection. Every path is attempted — a list that stopped at
/// the first refusal would leave a half-done delete with nothing saying
/// which half — and the answer is the platform's own words for the first
/// that refused, with the file named in them.
///
/// The paths are absolute: `NSURL fileURLWithPath:` resolves a relative
/// one against the process's working directory, which is not the
/// project.
pub fn trash(paths: &[std::path::PathBuf]) -> Result<(), String> {
    let mut refused: Option<String> = None;
    for path in paths {
        if let Err(said) = trash_one(path) {
            refused.get_or_insert(said);
        }
    }
    match refused {
        Some(said) => Err(said),
        None => Ok(()),
    }
}

/// One file or directory to the Trash, or what AppKit said about it.
fn trash_one(path: &std::path::Path) -> Result<(), String> {
    let name = path.display().to_string();
    let Ok(text) = CString::new(name.as_str()) else {
        // A path with a NUL in it cannot reach AppKit at all. It cannot
        // come from the disk either, so this is a guard rather than a
        // case.
        return Err(name);
    };

    // SAFETY: every pointer AppKit returns is checked before it is used
    // again, and the messages are the documented ones for these classes.
    // The call is made from the window's thread, which is where a menu
    // item runs.
    unsafe {
        let url: *mut Object = msg_send![
            class!(NSURL),
            fileURLWithPath: msg_send_string(&text)
        ];
        if url.is_null() {
            return Err(name);
        }
        let manager: *mut Object = msg_send![class!(NSFileManager), defaultManager];
        if manager.is_null() {
            return Err(name);
        }
        let mut error: *mut Object = std::ptr::null_mut();
        let moved: BOOL = msg_send![
            manager,
            trashItemAtURL: url
            resultingItemURL: std::ptr::null_mut::<Object>()
            error: &mut error
        ];
        if moved != NO {
            return Ok(());
        }
        Err(said_by(error).unwrap_or(name))
    }
}

/// An `NSString` from a C string, or null.
///
/// A helper rather than a line inside the `msg_send!` above: the macro
/// expands its arguments more than once, and a `CString` built inside it
/// would be dropped while AppKit still held the pointer.
///
/// # Safety
///
/// The caller keeps `text` alive across the call, which is what makes the
/// pointer valid for as long as `NSString` reads it.
unsafe fn msg_send_string(text: &CString) -> *mut Object {
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

/// What an `NSError` says, in the language the person's system is set to.
///
/// **Its words, not ours.** A file the disk refused to move — a locked
/// volume, a permission, a Trash that is full — is refused for a reason
/// only the platform knows, and a sentence of ours would be a worse
/// version of one it already wrote.
///
/// # Safety
///
/// `error` is either null or an `NSError` AppKit filled in.
unsafe fn said_by(error: *mut Object) -> Option<String> {
    if error.is_null() {
        return None;
    }
    unsafe {
        let described: *mut Object = msg_send![error, localizedDescription];
        if described.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![described, UTF8String];
        if utf8.is_null() {
            return None;
        }
        CStr::from_ptr(utf8).to_str().ok().map(str::to_owned)
    }
}

/// The window's seconds east of UTC, as the system reckons them now.
///
/// The log's date column says *Today 11:41*, and that needs a wall clock
/// rather than an instant. The platform is the only thing that knows
/// which one — the offset moves twice a year and again when a person
/// travels — so it is asked, not computed.
///
/// `NSTimeZone` rather than a crate: `objc` is already here for the title
/// bar, and this is one message send. `secondsFromGMT` is the offset **at
/// this moment**, daylight saving included, which is what a clock on a
/// screen means.
///
/// Zero when AppKit will not answer, which puts the column on UTC — an
/// hour or two wrong somewhere, rather than a window that will not draw.
pub fn utc_offset() -> i64 {
    unsafe {
        let class = objc::runtime::Class::get("NSTimeZone");
        let Some(class) = class else {
            return 0;
        };
        let zone: *mut Object = msg_send![class, localTimeZone];
        if zone.is_null() {
            return 0;
        }
        let seconds: isize = msg_send![zone, secondsFromGMT];
        seconds as i64
    }
}

/// Whether a sheet — a question of the window's — hangs from the window
/// now. `NSWindow attachedSheet` is the platform's answer, and it covers
/// every sheet, the unsaved-file question and the running-sessions one
/// alike. A window that cannot be reached has none.
pub fn has_sheet(window: &gpui::Window) -> bool {
    let Some(ns_window) = ns_window(window) else {
        return false;
    };
    // SAFETY: a live `NSWindow` reached through `ns_window`, and
    // `attachedSheet` is its documented accessor, called on the main thread
    // where GPUI updates a window.
    let sheet: *mut Object = unsafe { msg_send![ns_window, attachedSheet] };
    !sheet.is_null()
}

/// Bring a minimised window back from the Dock. A window that is not
/// minimised, or cannot be reached, is left as it is.
///
/// GPUI's `activate_window` is `makeKeyAndOrderFront:` and nothing else;
/// the quit from outside the window needs its question seen, and a sheet
/// on a window in the Dock is not. M5.11's task 17.
pub fn deminiaturize(window: &gpui::Window) {
    let Some(ns_window) = ns_window(window) else {
        return;
    };
    // SAFETY: a live `NSWindow` reached through `ns_window`;
    // `isMiniaturized` and `deminiaturize:` are its documented messages,
    // sent on the main thread where GPUI updates a window.
    unsafe {
        let minimised: BOOL = msg_send![ns_window, isMiniaturized];
        if minimised == YES {
            let sender: *mut Object = std::ptr::null_mut();
            let _: () = msg_send![ns_window, deminiaturize: sender];
        }
    }
}

// **A quit that comes from outside the window** — M5.11's decision 20.
//
// GPUI 0.2.2 registers `applicationWillTerminate:` on its application
// delegate and no `applicationShouldTerminate:`, so the Dock's Quit, a
// logout and a restart went straight to terminating: the window never
// asked about an unsaved file or a turn in progress, the questions `cmd-q`
// and the close button ask. The method is added to GPUI's delegate class
// at run time. Task 0.4 measured it with a windowless probe: the added
// method is called, the delegate need not be set again, a cancel keeps the
// process alive, and a later `terminate:` answered *now* quits.
//
// The method runs inside AppKit, where the window cannot be reached, so
// it reads what the window wrote down: whether Guild itself has decided to
// quit, and whether anything is at stake. A cancel leaves a request the
// window picks up on its next tick.

/// Guild itself has decided to quit — its questions answered, or none to
/// ask. Set by [`allow_termination`] and never cleared: a quit decided is
/// not undecided.
static QUITTING: AtomicBool = AtomicBool::new(false);
/// Something would be lost by quitting now — written every tick of the
/// watch loop by the window.
static NEEDS_ASKING: AtomicBool = AtomicBool::new(false);
/// AppKit was asked to quit, cancelled, and the window has not asked yet.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// `NSTerminateCancel` and `NSTerminateNow`.
const TERMINATE_CANCEL: usize = 0;
const TERMINATE_NOW: usize = 1;

/// What `applicationShouldTerminate:` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// Terminate now: nothing is at stake, or Guild decided.
    Now,
    /// Cancel, and leave the window a request to ask its questions.
    CancelAndAsk,
}

/// The answer, from Guild having decided and from something being at
/// stake. Pure, for the reason [`action_of`] is: the half that can be
/// wrong is checked without AppKit.
pub fn reply(quitting: bool, needs_asking: bool) -> Reply {
    if quitting || !needs_asking {
        Reply::Now
    } else {
        Reply::CancelAndAsk
    }
}

/// Add `applicationShouldTerminate:` to GPUI's application delegate.
///
/// Called once, inside `Application::run`'s closure — after GPUI has set
/// its delegate and before any quit can arrive. A class that is not there,
/// or a method that is already there, is logged and leaves quitting as it
/// was: GPUI renaming its class would bring back the old behaviour, not a
/// crash.
pub fn install_should_terminate() {
    const CLASS: &str = "GPUIApplicationDelegate";
    let Some(class) = Class::get(CLASS) else {
        tracing::warn!(
            class = CLASS,
            "no application delegate class; a quit from the Dock will not ask"
        );
        return;
    };
    let selector = sel!(applicationShouldTerminate:);
    if class.instance_method(selector).is_some() {
        tracing::warn!(
            class = CLASS,
            "the delegate already answers applicationShouldTerminate:; ours is not added"
        );
        return;
    }
    let Ok(types) = CString::new("Q@:@") else {
        return;
    };
    // SAFETY: `should_terminate` has exactly the signature the type string
    // says — an `NSUInteger` returned, `self`, `_cmd` and one object — which
    // is what `Imp` is transmuted from everywhere the runtime is used this
    // way. The class is a live class object from the runtime.
    let added = unsafe {
        let imp: objc::runtime::Imp = std::mem::transmute(
            should_terminate as extern "C" fn(&Object, Sel, *mut Object) -> usize,
        );
        objc::runtime::class_addMethod(
            class as *const Class as *mut Class,
            selector,
            imp,
            types.as_ptr(),
        )
    };
    if added == NO {
        tracing::warn!(
            class = CLASS,
            "applicationShouldTerminate: was not added; a quit from the Dock will not ask"
        );
    }
}

/// `-[GPUIApplicationDelegate applicationShouldTerminate:]`.
extern "C" fn should_terminate(_this: &Object, _cmd: Sel, _sender: *mut Object) -> usize {
    match reply(
        QUITTING.load(Ordering::SeqCst),
        NEEDS_ASKING.load(Ordering::SeqCst),
    ) {
        Reply::Now => TERMINATE_NOW,
        Reply::CancelAndAsk => {
            QUIT_REQUESTED.store(true, Ordering::SeqCst);
            TERMINATE_CANCEL
        }
    }
}

/// Guild has decided to quit: the next `applicationShouldTerminate:`
/// answers *now*. Called by `guild_desktop::quit`, which every quit of
/// Guild's own goes through.
pub fn allow_termination() {
    QUITTING.store(true, Ordering::SeqCst);
}

/// Whether a quit now would lose something — written by the window every
/// tick of the watch loop.
pub fn set_quit_needs_asking(needs: bool) {
    NEEDS_ASKING.store(needs, Ordering::SeqCst);
}

/// Whether AppKit cancelled a quit the window has not yet asked about —
/// answered once: the request is taken.
pub fn take_quit_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Where the application with a bundle identifier is installed —
/// `NSWorkspace`'s `URLForApplicationWithBundleIdentifier:`, the lookup
/// Launch Services answers — M12.10's Open In. `None` when it is not.
pub fn application(bundle_id: &str) -> Option<std::path::PathBuf> {
    let id = CString::new(bundle_id).ok()?;
    // SAFETY: every receiver is checked for nil before it is messaged;
    // the objects are autoreleased and only read here.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let id: *mut Object = msg_send![class!(NSString), stringWithUTF8String: id.as_ptr()];
        let url: *mut Object = msg_send![workspace, URLForApplicationWithBundleIdentifier: id];
        if url.is_null() {
            return None;
        }
        let path: *mut Object = msg_send![url, path];
        if path.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![path, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(std::path::PathBuf::from(
            CStr::from_ptr(utf8).to_string_lossy().into_owned(),
        ))
    }
}

/// A validated installed application bundle. Paths identify distinct installations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApplication {
    /// Actual bundle directory (canonical where possible).
    pub path: std::path::PathBuf,
    /// Actual CFBundleIdentifier, never inferred from the filename.
    pub bundle_id: String,
    /// Bundle filename without its extension.
    pub name: String,
}

/// Read a local .app bundle without launching or registering it.
pub fn application_bundle(path: &std::path::Path) -> Option<InstalledApplication> {
    if !path.is_dir()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let path = std::fs::canonicalize(path).ok()?;
    let encoded = CString::new(path.to_str()?).ok()?;
    let bundle_id = objc::rc::autoreleasepool(|| unsafe {
        // SAFETY: checked autoreleased Foundation receivers; copy the identifier.
        let string: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: encoded.as_ptr()];
        let bundle: *mut Object = msg_send![class!(NSBundle), bundleWithPath: string];
        if bundle.is_null() {
            return None;
        }
        let id: *mut Object = msg_send![bundle, bundleIdentifier];
        if id.is_null() {
            return None;
        }
        let bytes: *const c_char = msg_send![id, UTF8String];
        if bytes.is_null() {
            return None;
        }
        Some(CStr::from_ptr(bytes).to_string_lossy().into_owned())
    })?;
    if bundle_id.is_empty() {
        return None;
    }
    let name = path.file_stem()?.to_string_lossy().into_owned();
    Some(InstalledApplication {
        path,
        bundle_id,
        name,
    })
}

/// Bounded read-only inventory for the explicit installed-app selector.
/// Stop at bundles; never recurse through their contents or arbitrary home paths.
pub fn installed_applications() -> Vec<InstalledApplication> {
    let mut roots = vec![
        std::path::PathBuf::from("/Applications"),
        "/System/Applications".into(),
        "/System/Library/CoreServices/Applications".into(),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        roots.push(home.join("Applications"));
        roots.push(home.join("Library/Application Support/JetBrains/Toolbox/apps"));
    }
    let mut apps = installed_applications_in(&roots);
    if let Some(finder) = application_bundle(std::path::Path::new(
        "/System/Library/CoreServices/Finder.app",
    )) && !apps.iter().any(|app| app.path == finder.path)
    {
        apps.push(finder);
    }
    apps.sort_by_key(|app| (app.name.to_lowercase(), app.path.clone()));
    apps
}

fn installed_applications_in(roots: &[std::path::PathBuf]) -> Vec<InstalledApplication> {
    let mut pending: Vec<_> = roots.iter().cloned().map(|path| (path, 0)).collect();
    let mut visited = std::collections::HashSet::new();
    let mut found = std::collections::BTreeMap::new();
    let mut examined = 0;
    while let Some((directory, depth)) = pending.pop() {
        if visited.len() >= 2048 || examined >= 10000 {
            break;
        }
        let Ok(canonical) = std::fs::canonicalize(&directory) else {
            continue;
        };
        if !visited.insert(canonical) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            examined += 1;
            if examined > 10000 {
                break;
            }
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
            {
                if let Some(app) = application_bundle(&path) {
                    found.entry(app.path.clone()).or_insert(app);
                }
            } else if depth < 4
                && !entry.file_name().to_string_lossy().starts_with('.')
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
            {
                pending.push((path, depth + 1));
            }
        }
    }
    found.into_values().collect()
}

/// A file's icon as Finder draws it, as PNG bytes near `pixels` square —
/// `iconForFile:`, then the representation AppKit picks for that size
/// (`CGImageForProposedRect:`), then a PNG of it. For an application
/// bundle it is the application's own icon. Not the TIFF of the whole
/// image: its first representation may be the 1024-pixel one.
pub fn icon_png(path: &std::path::Path, pixels: f64) -> Option<Vec<u8>> {
    /// `NSBitmapImageFileTypePNG`.
    const PNG: u64 = 4;
    #[repr(C)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }
    let path = CString::new(path.to_str()?).ok()?;
    // SAFETY: nil checked before every message; the representation we
    // allocate is released on every path out, and the bytes are copied out
    // of the `NSData` before it goes back to the pool.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let path: *mut Object = msg_send![class!(NSString), stringWithUTF8String: path.as_ptr()];
        let image: *mut Object = msg_send![workspace, iconForFile: path];
        if image.is_null() {
            return None;
        }
        let mut rect = Rect {
            x: 0.0,
            y: 0.0,
            width: pixels,
            height: pixels,
        };
        let none: *mut Object = std::ptr::null_mut();
        let cg: *mut std::ffi::c_void =
            msg_send![image, CGImageForProposedRect: &mut rect context: none hints: none];
        if cg.is_null() {
            return None;
        }
        let rep: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut Object = msg_send![rep, initWithCGImage: cg];
        if rep.is_null() {
            return None;
        }
        let png: *mut Object = msg_send![rep, representationUsingType: PNG properties: none];
        let bytes = if png.is_null() {
            None
        } else {
            let length: usize = msg_send![png, length];
            let bytes: *const u8 = msg_send![png, bytes];
            (!bytes.is_null() && length > 0)
                .then(|| std::slice::from_raw_parts(bytes, length).to_vec())
        };
        let _: () = msg_send![rep, release];
        bytes
    }
}

/// Read the macOS accessibility motion preference on the window thread.
pub fn reduce_motion() -> bool {
    // SAFETY: optional class and receiver checked before the documented accessor.
    unsafe {
        let Some(class) = Class::get("NSWorkspace") else {
            return true;
        };
        let workspace: *mut Object = msg_send![class, sharedWorkspace];
        if workspace.is_null() {
            return true;
        }
        let reduced: BOOL = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        reduced != NO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_inventory_is_bounded_and_stops_at_valid_bundles() {
        let root = tempfile::tempdir().unwrap();
        let bundle = |relative: &str, id: &str| {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.join("Contents")).unwrap();
            std::fs::write(path.join("Contents/Info.plist"),format!(r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#)).unwrap();
            path
        };
        bundle("Group/IDE.app", "org.example.ide");
        bundle("Outer.app", "org.example.outer");
        bundle("Outer.app/Contents/Hidden.app", "org.example.plugin");
        bundle("a/b/c/d/e/f/TooDeep.app", "org.example.deep");
        std::fs::create_dir_all(root.path().join("Invalid.app")).unwrap();
        let apps = installed_applications_in(&[root.path().to_path_buf()]);
        let ids: std::collections::BTreeSet<_> =
            apps.iter().map(|app| app.bundle_id.as_str()).collect();
        assert_eq!(ids, ["org.example.ide", "org.example.outer"].into());
    }

    #[test]
    fn a_new_chime_restarts_an_already_playing_sound_without_using_a_speaker() {
        use objc::runtime::{BOOL, NO, Sel, YES};

        // NSSound rejects play while already playing. Exercise our actual ObjC
        // messages against that protocol, without loading or playing audio.
        extern "C" fn play(this: &mut Object, _: Sel) -> BOOL {
            unsafe {
                if *this.get_ivar::<BOOL>("playing") == YES {
                    return NO;
                }
                this.set_ivar("playing", YES);
                let starts = *this.get_ivar::<usize>("starts");
                this.set_ivar("starts", starts + 1);
            }
            YES
        }
        extern "C" fn stop(this: &mut Object, _: Sel) -> BOOL {
            unsafe {
                this.set_ivar("playing", NO);
            }
            YES
        }
        let mut class = objc::declare::ClassDecl::new("GuildChimeProtocolTest", class!(NSObject))
            .expect("unique test class");
        class.add_ivar::<BOOL>("playing");
        class.add_ivar::<usize>("starts");
        unsafe {
            class.add_method(sel!(play), play as extern "C" fn(&mut Object, Sel) -> BOOL);
            class.add_method(sel!(stop), stop as extern "C" fn(&mut Object, Sel) -> BOOL);
            let class = class.register();
            let sound: *mut Object = msg_send![class, new];
            assert!(!sound.is_null());
            restart_sound(sound);
            restart_sound(sound);
            let starts = *(*sound).get_ivar::<usize>("starts");
            let _: () = msg_send![sound, release];
            assert_eq!(starts, 2, "a later pass must start its chime too");
        }
    }

    /// Nothing asked for is nothing done — and it is not a failure. The
    /// tree's Delete asks about a selection, and an empty selection must
    /// not put the platform's words in front of a person who chose
    /// nothing.
    #[test]
    fn trashing_nothing_is_not_a_failure() {
        assert_eq!(trash(&[]), Ok(()));
    }

    /// **A file that is not there is a refusal, not a silence.** This is
    /// the one gesture in the window that can lose work, so the caller
    /// has to be able to tell "it went to the Trash" from "nothing
    /// happened" — a `Result` that is `Ok` either way would put the row
    /// back in the tree with no notice and no reason.
    #[test]
    fn trashing_what_is_not_there_says_so_in_the_platform_words() {
        let folder = tempfile::Builder::new()
            .prefix("guild-trash-")
            .tempdir()
            .expect("a scratch folder");
        let missing = folder.path().join("guild-trash-nothing-here-4f1c9a");
        let refused = trash(std::slice::from_ref(&missing)).expect_err("nothing to trash");
        // **AppKit's sentence, not the path back again.** The fallback
        // in `trash_one` answers the path when it cannot reach the
        // platform at all, so a test that only looked for the name would
        // pass on a call that never happened. What comes back here is
        // *The file “…” doesn't exist.* — a sentence, naming the file.
        assert_ne!(
            refused,
            missing.display().to_string(),
            "the platform was never asked"
        );
        assert!(
            refused.contains("guild-trash-nothing-here-4f1c9a"),
            "the refusal does not name the file: {refused}"
        );
    }

    #[test]
    fn the_system_default_is_to_fill_the_screen() {
        assert_eq!(action_of(None), DoubleClick::Zoom);
        assert_eq!(action_of(Some("Maximize")), DoubleClick::Zoom);
    }

    #[test]
    fn the_other_two_choices_are_honoured() {
        assert_eq!(action_of(Some("Minimize")), DoubleClick::Minimize);
        assert_eq!(action_of(Some("None")), DoubleClick::Nothing);
    }

    /// A value we do not know must not minimise a window a person is
    /// working in. Falling back to the documented default is the safe
    /// direction to be wrong in.
    #[test]
    fn an_unknown_value_falls_back_to_the_default() {
        assert_eq!(action_of(Some("Sideways")), DoubleClick::Zoom);
        assert_eq!(action_of(Some("")), DoubleClick::Zoom);
    }

    #[test]
    fn natural_scrolling_uses_the_system_default_and_honours_both_values() {
        assert!(natural_of(None));
        assert!(natural_of(Some(1)));
        assert!(!natural_of(Some(0)));
    }

    /// **A quit from outside the window asks only when something is at
    /// stake, and never twice.** The Dock's Quit, a logout or a restart
    /// reaches `applicationShouldTerminate:` with nothing of Guild's having
    /// decided anything: with nothing at stake it goes at once, with
    /// something at stake it is cancelled and the window asks. When the
    /// window itself has decided — its questions answered, or none to ask —
    /// the same method must let it go, or the answer would be asked again.
    #[test]
    fn a_quit_from_outside_asks_only_when_something_is_at_stake_and_never_twice() {
        assert_eq!(reply(false, false), Reply::Now);
        assert_eq!(reply(false, true), Reply::CancelAndAsk);
        assert_eq!(reply(true, true), Reply::Now);
        assert_eq!(reply(true, false), Reply::Now);
    }

    /// The Dock counts who is waiting, and says nothing when nobody is.
    #[test]
    fn the_dock_badge_is_the_count_and_nothing_at_zero() {
        assert_eq!(badge_label(0), None);
        assert_eq!(badge_label(1).as_deref(), Some("1"));
        assert_eq!(badge_label(12).as_deref(), Some("12"));
    }
}

#[cfg(feature = "native-policy-probe")]
pub mod window_probe;

#[cfg(feature = "adaptive-terminal-refresh")]
pub(crate) mod terminal_deadline;
#[cfg(feature = "adaptive-terminal-refresh")]
pub(crate) mod terminal_power;

#[cfg(feature = "native-preview")]
pub fn probe_mach_ticks() -> u64 {
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
    }
    unsafe { mach_absolute_time() }
}

#[cfg(feature = "native-preview")]
pub fn probe_low_power() -> Option<bool> {
    unsafe {
        let info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
        let supported: BOOL = msg_send![info,respondsToSelector:sel!(isLowPowerModeEnabled)];
        if supported != YES {
            return None;
        }
        let low: BOOL = msg_send![info, isLowPowerModeEnabled];
        Some(low == YES)
    }
}

/// Own-task public TASK_POWER_INFO_V2 GPU nanoseconds; zero is not a supported-counter proof.
#[cfg(feature = "native-preview")]
pub fn probe_gpu_ns() -> Option<u64> {
    // SDK mach/task_info.h: six CPU u64s, then gpu_energy_data (four u64s),
    // arm64 task_energy, task_ptime and task_pset_switches. All naturally aligned.
    let mut values = [0u64; 13];
    let mut count = if cfg!(target_arch = "aarch64") {
        26
    } else {
        24
    };
    // The existing libc binding is sufficient for this diagnostic query.
    #[allow(deprecated)]
    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            26,
            values.as_mut_ptr().cast(),
            &mut count,
        )
    };
    (result == 0 && count >= 14).then_some(values[6])
}

/// Kernel executable-reference guard for explicit updater recovery.
pub(crate) mod update_processes;

/// UTF-8 byte boundaries of extended grapheme clusters, using Foundation's
/// Unicode implementation without a second segmentation dependency.
pub(crate) fn grapheme_boundaries(text: &str) -> Vec<usize> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NativeRange {
        location: usize,
        length: usize,
    }
    // SAFETY: NSRange consists of two NSUInteger values on supported macOS.
    unsafe impl objc::Encode for NativeRange {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{_NSRange=QQ}") }
        }
    }
    let mut result = vec![0];
    if text.is_empty() {
        return result;
    }
    // SAFETY: NSString copies valid UTF-8 bytes; the owned object is released
    // below. Returned UTF-16 ranges are converted through valid Rust chars.
    unsafe {
        let allocated: *mut Object = msg_send![class!(NSString), alloc];
        let string: *mut Object =
            msg_send![allocated, initWithBytes:text.as_ptr() length:text.len() encoding:4usize];
        if string.is_null() {
            return vec![0, text.len()];
        }
        let mut utf16 = 0;
        let mut byte = 0;
        let mut chars = text.chars();
        let length: usize = msg_send![string, length];
        while utf16 < length {
            let range: NativeRange =
                msg_send![string, rangeOfComposedCharacterSequenceAtIndex:utf16];
            let end = range.location + range.length;
            if end <= utf16 {
                break;
            }
            while utf16 < end {
                let Some(ch) = chars.next() else {
                    break;
                };
                utf16 += ch.len_utf16();
                byte += ch.len_utf8();
            }
            if !(text.as_bytes().get(byte.wrapping_sub(1)) == Some(&b'\r')
                && text.as_bytes().get(byte) == Some(&b'\n'))
            {
                result.push(byte);
            }
        }
        let _: () = msg_send![string, release];
    }
    if result.last().copied() != Some(text.len()) {
        result.push(text.len());
    }
    result
}

pub mod alias;
pub mod browser;
pub mod close_prompt;
pub mod color_sampler;
pub mod default_app;
pub mod open_files;

/// Local OSC7 authority comparison, without spawning a process or trusting env.
pub(crate) fn local_hostname() -> Option<String> {
    let mut bytes = [0u8; 256];
    // gethostname writes at most the supplied length; reject a truncated result.
    if unsafe { libc::gethostname(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return None;
    }
    let end = bytes.iter().position(|b| *b == 0)?;
    std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

/// Abandon the native conversion session after its terminal has committed its
/// own preedit. Call outside an entity borrow: an IME may synchronously callback.
pub(crate) fn discard_terminal_marked_text(window: &gpui::Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let view = appkit.ns_view.as_ptr().cast::<Object>();
    if view.is_null() {
        return;
    }
    // SAFETY: checked NSView from the live window on the main thread; these
    // documented NSTextInputContext APIs neither target nor inspect other apps.
    unsafe {
        let context: *mut Object = msg_send![view, inputContext];
        if !context.is_null() {
            let _: () = msg_send![context, discardMarkedText];
        }
    }
}
