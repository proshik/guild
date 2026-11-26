//! Open a file in the application macOS opens it with — what Finder's
//! double-click and `open <path>` do — for a file Guild cannot show.
use super::*;
use std::ffi::c_void;
use std::path::Path;

/// What a launch AppKit accepted came to, later and off the window's
/// thread: `Err` with the system's words when the application could not
/// open the file after all. Dropped unanswered if AppKit never calls back.
pub type Launched = tokio::sync::oneshot::Receiver<Result<(), String>>;

/// Hand a local file to its default application. Answers at once why it
/// could not be handed over, in words for a notice; the launch itself is
/// asynchronous, and its outcome comes through [`Launched`].
pub fn open_file(path: &Path) -> Result<Launched, String> {
    #[cfg(test)]
    if let Some(answer) = tests::capture(path) {
        return answer;
    }
    objc::rc::autoreleasepool(|| unsafe {
        // SAFETY: the documented shared workspace and autoreleased Foundation
        // objects stay alive for this call. The asynchronous open API retains
        // its arguments and copies the handler block; the block owns all it
        // captures, and no borrowed Rust state crosses that boundary.
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        open_with_workspace(workspace, path)
    })
}

pub(super) unsafe fn file_url(path: &Path) -> Result<*mut Object, String> {
    use std::os::unix::ffi::OsStrExt as _;
    let text = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "The path contains a NUL byte".to_owned())?;
    unsafe {
        let string: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
        if string.is_null() {
            return Err("The path is not valid UTF-8".into());
        }
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: string];
        if url.is_null() {
            return Err("Could not create the file URL".into());
        }
        Ok(url)
    }
}

unsafe fn open_with_workspace(workspace: *mut Object, path: &Path) -> Result<Launched, String> {
    if workspace.is_null() {
        return Err("Cannot access macOS application services; try restarting Guild".into());
    }
    let (sender, launched) = tokio::sync::oneshot::channel();
    // Called once, on a queue of AppKit's, with the running application
    // or the error; the sender is taken so a second call says nothing.
    let sender = std::sync::Mutex::new(Some(sender));
    let handler = block2::RcBlock::new(move |_: *mut c_void, error: *mut c_void| {
        let outcome = if error.is_null() {
            Ok(())
        } else {
            // SAFETY: a non-null second argument is the NSError AppKit
            // hands the handler, alive for the call.
            Err(objc::rc::autoreleasepool(|| unsafe {
                description(error.cast())
            }))
        };
        if let Some(sender) = sender.lock().ok().and_then(|mut sender| sender.take()) {
            let _ = sender.send(outcome);
        }
    });
    unsafe {
        let file = file_url(path)?;
        let application: *mut Object = msg_send![workspace, URLForApplicationToOpenURL: file];
        if application.is_null() {
            return Err("No application is set to open this kind of file".into());
        }
        let urls: *mut Object = msg_send![class!(NSArray), arrayWithObject: file];
        let config: *mut Object = msg_send![class!(NSWorkspaceOpenConfiguration), configuration];
        if urls.is_null() || config.is_null() {
            return Err("Could not prepare the launch; try restarting Guild".into());
        }
        let handler = block2::RcBlock::as_ptr(&handler).cast::<Object>();
        let _: () = msg_send![workspace, openURLs: urls withApplicationAtURL: application configuration: config completionHandler: handler];
        Ok(launched)
    }
}

/// An NSError's own words, or a stand-in when it has none.
///
/// # Safety
/// `error` is a live NSError.
unsafe fn description(error: *mut Object) -> String {
    unsafe {
        let text: *mut Object = msg_send![error, localizedDescription];
        if text.is_null() {
            return "The application did not open it".into();
        }
        let bytes: *const c_char = msg_send![text, UTF8String];
        if bytes.is_null() {
            return "The application did not open it".into();
        }
        CStr::from_ptr(bytes).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct Captured {
        paths: Vec<PathBuf>,
        failure: Option<String>,
        /// Launches are accepted and left unanswered until `complete`.
        hold: bool,
        pending: Vec<tokio::sync::oneshot::Sender<Result<(), String>>>,
    }

    thread_local! {
        static CAPTURE: RefCell<Option<Captured>> = const { RefCell::new(None) };
    }

    /// Scoped per-thread launch interception; never open applications in GPUI tests.
    pub(crate) struct Capture;
    impl Capture {
        pub(crate) fn new() -> Self {
            CAPTURE.with(|capture| {
                assert!(
                    capture
                        .replace(Some(Captured {
                            paths: Vec::new(),
                            failure: None,
                            hold: false,
                            pending: Vec::new(),
                        }))
                        .is_none()
                )
            });
            Self
        }
        /// Every launch from now on fails with these words.
        pub(crate) fn fail_with(&self, words: &str) {
            CAPTURE.with(|capture| {
                capture.borrow_mut().as_mut().unwrap().failure = Some(words.into())
            });
        }
        /// Every launch from now on is accepted and left unanswered, as
        /// AppKit leaves it until the application has opened the file.
        pub(crate) fn hold(&self) {
            CAPTURE.with(|capture| capture.borrow_mut().as_mut().unwrap().hold = true);
        }
        /// Answer every launch left unanswered, as AppKit's completion
        /// handler does: `Err` with the system's words.
        pub(crate) fn complete(&self, outcome: Result<(), String>) {
            let pending = CAPTURE.with(|capture| {
                std::mem::take(&mut capture.borrow_mut().as_mut().unwrap().pending)
            });
            for sender in pending {
                let _ = sender.send(outcome.clone());
            }
        }
        pub(crate) fn paths(&self) -> Vec<PathBuf> {
            CAPTURE.with(|capture| capture.borrow().as_ref().unwrap().paths.clone())
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            CAPTURE.with(|capture| capture.replace(None));
        }
    }
    pub(super) fn capture(path: &Path) -> Option<Result<Launched, String>> {
        CAPTURE.with(|capture| {
            let mut capture = capture.borrow_mut();
            let capture = capture.as_mut()?;
            capture.paths.push(path.to_path_buf());
            if let Some(words) = capture.failure.clone() {
                return Some(Err(words));
            }
            let (sender, launched) = tokio::sync::oneshot::channel();
            if capture.hold {
                capture.pending.push(sender);
            } else {
                let _ = sender.send(Ok(()));
            }
            Some(Ok(launched))
        })
    }

    #[derive(Default)]
    struct Observed {
        lookups: Vec<String>,
        launches: Vec<(String, String, bool)>,
        no_application: bool,
        /// What the application says when it is launched: `None` opens.
        launch_fails: Option<&'static str>,
        /// The completion AppKit has not called yet: a signal that lets it,
        /// and the thread it is called from.
        completion: Option<(std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>)>,
    }

    /// The handler block, copied as AppKit copies it, carried to the thread
    /// it is called from.
    struct Copied(block2::RcBlock<dyn Fn(*mut c_void, *mut c_void)>);
    // SAFETY: a heap block's reference count is atomic, and the handler it
    // wraps is called from AppKit's queues; it captures only `Send` state.
    unsafe impl Send for Copied {}
    thread_local! {
        static OBSERVED: RefCell<Observed> = RefCell::new(Observed::default());
    }
    unsafe fn string(url: *mut Object) -> String {
        unsafe {
            let text: *mut Object = msg_send![url, absoluteString];
            let bytes: *const c_char = msg_send![text, UTF8String];
            CStr::from_ptr(bytes).to_string_lossy().into_owned()
        }
    }
    extern "C" fn application(_: &Object, _: Sel, url: *mut Object) -> *mut Object {
        let url = unsafe { string(url) };
        OBSERVED.with(|observed| {
            let mut observed = observed.borrow_mut();
            observed.lookups.push(url);
            if observed.no_application {
                std::ptr::null_mut()
            } else {
                unsafe { file_url(Path::new("/Applications/Preview.app")).unwrap() }
            }
        })
    }
    extern "C" fn open(
        _: &Object,
        _: Sel,
        urls: *mut Object,
        app: *mut Object,
        config: *mut Object,
        handler: *mut Object,
    ) {
        unsafe {
            let file: *mut Object = msg_send![urls, objectAtIndex: 0usize];
            let fails = OBSERVED.with(|observed| {
                let mut observed = observed.borrow_mut();
                observed
                    .launches
                    .push((string(file), string(app), !config.is_null()));
                observed.launch_fails
            });
            assert!(
                !handler.is_null(),
                "a launch is answered through its handler"
            );
            // As AppKit does: the block is copied, the call returns, and
            // the handler runs later on another thread once the launch is
            // over, with the error or nil.
            let copied =
                Copied((*handler.cast::<block2::Block<dyn Fn(*mut c_void, *mut c_void)>>()).copy());
            let (go, wait) = std::sync::mpsc::channel::<()>();
            let thread = std::thread::spawn(move || {
                let copied = copied;
                let _ = wait.recv();
                objc::rc::autoreleasepool(|| {
                    let error: *mut Object = match fails {
                        Some(words) => {
                            let words = CString::new(words).unwrap();
                            let words: *mut Object =
                                msg_send![class!(NSString), stringWithUTF8String: words.as_ptr()];
                            let key = CString::new("NSLocalizedDescription").unwrap();
                            let key: *mut Object =
                                msg_send![class!(NSString), stringWithUTF8String: key.as_ptr()];
                            let info: *mut Object = msg_send![class!(NSDictionary), dictionaryWithObject: words forKey: key];
                            msg_send![class!(NSError), errorWithDomain: key code: 1isize userInfo: info]
                        }
                        None => std::ptr::null_mut(),
                    };
                    copied.0.call((std::ptr::null_mut(), error.cast()));
                });
            });
            OBSERVED.with(|observed| observed.borrow_mut().completion = Some((go, thread)));
        }
    }

    /// Let AppKit's deferred completion run, and wait until it has.
    fn complete_launch() {
        let (go, thread) = OBSERVED
            .with(|observed| observed.borrow_mut().completion.take())
            .expect("a launch waiting for its completion");
        go.send(()).unwrap();
        thread.join().unwrap();
    }

    /// The file's own URL is asked for its application — not the browser's,
    /// as `browser::open_local_page` asks — and a missing application is
    /// words for a notice, not a launch.
    #[test]
    fn a_file_opens_in_the_application_registered_for_it() {
        let mut class =
            objc::declare::ClassDecl::new("GuildDefaultAppWorkspaceTest", class!(NSObject))
                .unwrap();
        unsafe {
            class.add_method(
                sel!(URLForApplicationToOpenURL:),
                application as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
            );
            class.add_method(
                sel!(openURLs:withApplicationAtURL:configuration:completionHandler:),
                open as extern "C" fn(
                    &Object,
                    Sel,
                    *mut Object,
                    *mut Object,
                    *mut Object,
                    *mut Object,
                ),
            );
            let class = class.register();
            objc::rc::autoreleasepool(|| {
                let workspace: *mut Object = msg_send![class, new];
                // Foundation decomposes a path's characters, as the disk
                // does, so a precomposed `ё` would come back as two.
                let path = Path::new("/tmp/отчет #1?.pdf");
                let url = "file:///tmp/%D0%BE%D1%82%D1%87%D0%B5%D1%82%20%231%3F.pdf";
                OBSERVED.with(|observed| observed.replace(Observed::default()));
                let mut launched = open_with_workspace(workspace, path).unwrap();
                assert!(
                    launched.try_recv().is_err(),
                    "nothing said before AppKit calls back"
                );
                complete_launch();
                assert_eq!(launched.try_recv(), Ok(Ok(())), "the application opened it");
                OBSERVED.with(|observed| {
                    let observed = observed.borrow();
                    assert_eq!(observed.lookups, [url]);
                    assert_eq!(
                        observed.launches,
                        [(url.into(), "file:///Applications/Preview.app".into(), true)]
                    );
                });
                // The application is found, and then cannot open the file:
                // AppKit says so only in the handler.
                OBSERVED.with(|observed| {
                    observed.replace(Observed {
                        launch_fails: Some("The application is damaged"),
                        ..Default::default()
                    })
                });
                let mut launched = open_with_workspace(workspace, path).unwrap();
                assert!(launched.try_recv().is_err());
                complete_launch();
                assert_eq!(
                    launched.try_recv(),
                    Ok(Err("The application is damaged".into()))
                );
                OBSERVED.with(|observed| {
                    observed.replace(Observed {
                        no_application: true,
                        ..Default::default()
                    })
                });
                assert!(
                    open_with_workspace(workspace, path)
                        .unwrap_err()
                        .contains("No application")
                );
                OBSERVED.with(|observed| assert!(observed.borrow().launches.is_empty()));
                assert!(open_with_workspace(std::ptr::null_mut(), path).is_err());
                let _: () = msg_send![workspace, release];
            });
        }
    }
}
