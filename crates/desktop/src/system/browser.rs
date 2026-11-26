//! Open a local page in the default web browser, independently of file associations.
use super::*;

/// Open a file URL in the application registered for HTTPS. Looking up the
/// file URL itself would choose an HTML editor on some systems.
pub fn open_local_page(url: &str) -> Result<(), String> {
    #[cfg(test)]
    if tests::capture(url) {
        return Ok(());
    }
    objc::rc::autoreleasepool(|| unsafe {
        // SAFETY: the documented shared workspace and autoreleased Foundation
        // objects stay alive for this call. The asynchronous open API retains
        // its arguments; no borrowed Rust state crosses that boundary.
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        open_with_workspace(workspace, url)
    })
}

unsafe fn native_url(url: &str) -> Result<*mut Object, String> {
    let text = CString::new(url).map_err(|_| "Page URL contains a NUL byte".to_owned())?;
    unsafe {
        let string: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
        if string.is_null() {
            return Err("Could not create the page URL string".into());
        }
        let url: *mut Object = msg_send![class!(NSURL), URLWithString: string];
        if url.is_null() {
            return Err("Could not create the page URL".into());
        }
        Ok(url)
    }
}

unsafe fn open_with_workspace(workspace: *mut Object, url: &str) -> Result<(), String> {
    if workspace.is_null() {
        return Err("Cannot access macOS application services; try restarting Guild".into());
    }
    unsafe {
        let page = native_url(url)?;
        let is_file: BOOL = msg_send![page, isFileURL];
        if is_file != YES {
            return Err("The local page opener requires a file URL".into());
        }
        let https = native_url("https://example.com")?;
        let browser: *mut Object = msg_send![workspace, URLForApplicationToOpenURL: https];
        if browser.is_null() {
            return Err("No default web browser is available; choose a default web browser in System Settings and try again".into());
        }
        let urls: *mut Object = msg_send![class!(NSArray), arrayWithObject: page];
        let config: *mut Object = msg_send![class!(NSWorkspaceOpenConfiguration), configuration];
        if urls.is_null() || config.is_null() {
            return Err("Could not prepare the browser launch; try restarting Guild".into());
        }
        let _: () = msg_send![workspace, openURLs: urls withApplicationAtURL: browser configuration: config completionHandler: std::ptr::null_mut::<Object>()];
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static CAPTURE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    }

    /// Scoped per-thread launch interception; never open applications in GPUI tests.
    pub(crate) struct Capture;
    impl Capture {
        pub(crate) fn new() -> Self {
            CAPTURE.with(|capture| assert!(capture.replace(Some(Vec::new())).is_none()));
            Self
        }
        pub(crate) fn urls(&self) -> Vec<String> {
            CAPTURE.with(|capture| capture.borrow().as_ref().unwrap().clone())
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            CAPTURE.with(|capture| capture.replace(None));
        }
    }
    pub(super) fn capture(url: &str) -> bool {
        CAPTURE.with(|capture| {
            if let Some(urls) = capture.borrow_mut().as_mut() {
                urls.push(url.into());
                true
            } else {
                false
            }
        })
    }

    #[derive(Default)]
    struct Observed {
        lookups: Vec<String>,
        launches: Vec<(String, String, bool)>,
        no_browser: bool,
    }
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
            observed.lookups.push(url.clone());
            if observed.no_browser {
                std::ptr::null_mut()
            } else {
                // Model a machine whose HTML handler is an editor.
                unsafe {
                    native_url(if url.starts_with("https:") {
                        "file:///Applications/TestBrowser.app"
                    } else {
                        "file:///Applications/HtmlEditor.app"
                    })
                    .unwrap()
                }
            }
        })
    }
    extern "C" fn open(
        _: &Object,
        _: Sel,
        urls: *mut Object,
        app: *mut Object,
        config: *mut Object,
        _: *mut Object,
    ) {
        unsafe {
            let page: *mut Object = msg_send![urls, objectAtIndex: 0usize];
            OBSERVED.with(|observed| {
                observed
                    .borrow_mut()
                    .launches
                    .push((string(page), string(app), !config.is_null()))
            });
        }
    }

    #[test]
    fn native_browser_dispatch_uses_https_application_and_refuses_a_missing_browser() {
        let mut class =
            objc::declare::ClassDecl::new("GuildBrowserWorkspaceTest", class!(NSObject)).unwrap();
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
                let url = "file:///tmp/%D0%BE%D1%82%D1%87%D1%91%D1%82%20%231%3F.html";
                OBSERVED.with(|observed| observed.replace(Observed::default()));
                assert_eq!(open_with_workspace(workspace, url), Ok(()));
                OBSERVED.with(|observed| {
                    let observed = observed.borrow();
                    assert_eq!(observed.lookups, ["https://example.com"]);
                    assert_eq!(
                        observed.launches,
                        [(
                            url.into(),
                            "file:///Applications/TestBrowser.app".into(),
                            true
                        )]
                    );
                });
                OBSERVED.with(|observed| {
                    observed.replace(Observed {
                        no_browser: true,
                        ..Default::default()
                    })
                });
                assert!(
                    open_with_workspace(workspace, url)
                        .unwrap_err()
                        .contains("choose a default web browser in System Settings")
                );
                OBSERVED.with(|observed| {
                    assert!(
                        observed.borrow().launches.is_empty(),
                        "no file-association fallback"
                    )
                });
                assert!(open_with_workspace(workspace, "https://example.org").is_err());
                let _: () = msg_send![workspace, release];
            });
        }
    }
}
