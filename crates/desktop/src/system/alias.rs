//! Whether a file is a Finder alias, in Foundation's own word for it.
//!
//! macOS opens an alias by opening its original, an application included,
//! so a file handed to its default application must not be one. A bookmark
//! file's header is checked by the caller from the bytes it already read;
//! this asks Foundation, which also knows an alias by the Finder flag an
//! old one carries on a data fork that may be empty.
use super::*;
use std::path::Path;

/// Whether Foundation calls the file at `path` an alias. Reads the disk, so
/// it belongs on a background thread; a path it cannot ask about is not one.
///
/// Foundation calls a symbolic link an alias too: the paths a terminal link
/// resolves to are canonical, so a link never reaches it.
pub fn is_alias(path: &Path) -> bool {
    objc::rc::autoreleasepool(|| unsafe {
        // SAFETY: Foundation objects created here are autoreleased into this
        // pool; every pointer is checked before it is messaged.
        let Ok(url) = default_app::file_url(path) else {
            return false;
        };
        let key = CString::new("NSURLIsAliasFileKey").unwrap();
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: key.as_ptr()];
        if key.is_null() {
            return false;
        }
        let mut value: *mut Object = std::ptr::null_mut();
        let asked: BOOL = msg_send![url, getResourceValue: &mut value forKey: key error: std::ptr::null_mut::<*mut Object>()];
        if asked != YES || value.is_null() {
            return false;
        }
        let alias: BOOL = msg_send![value, boolValue];
        alias == YES
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::links::{Shown, shown_by};

    /// An alias Foundation writes, as Finder's Make Alias does.
    fn make_alias(original: &Path, alias: &Path) {
        objc::rc::autoreleasepool(|| unsafe {
            // NSURLBookmarkCreationSuitableForBookmarkFile
            const SUITABLE_FOR_BOOKMARK_FILE: usize = 1 << 10;
            let original = default_app::file_url(original).unwrap();
            let alias = default_app::file_url(alias).unwrap();
            let nothing = std::ptr::null_mut::<Object>();
            let data: *mut Object = msg_send![original, bookmarkDataWithOptions: SUITABLE_FOR_BOOKMARK_FILE includingResourceValuesForKeys: nothing relativeToURL: nothing error: std::ptr::null_mut::<*mut Object>()];
            assert!(!data.is_null(), "a bookmark of the original");
            let written: BOOL = msg_send![class!(NSURL), writeBookmarkData: data toURL: alias options: SUITABLE_FOR_BOOKMARK_FILE error: std::ptr::null_mut::<*mut Object>()];
            assert_eq!(written, YES, "the alias file written");
        });
    }

    #[test]
    fn an_alias_is_refused_never_followed() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().canonicalize().unwrap();
        // What an alias could open: an application.
        let app = dir.join("Calculator.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        let original = dir.join("Calculator.app");
        let alias = dir.join("minutes.pdf");
        make_alias(&original, &alias);

        assert!(is_alias(&alias));
        assert!(
            std::fs::read(&alias)
                .unwrap()
                .starts_with(b"book\0\0\0\0mark"),
            "Foundation writes the bookmark header the classifier also reads"
        );
        assert_eq!(shown_by(&alias), Shown::Refused);

        // An old alias: the Finder's alias flag, and an empty data fork,
        // which no header check can see.
        let old = dir.join("old.pdf");
        std::fs::write(&old, b"").unwrap();
        let mut info = [0u8; 32];
        info[..4].copy_from_slice(b"alis");
        info[8] = 0x80; // kIsAlias, big-endian
        let name = CString::new("com.apple.FinderInfo").unwrap();
        let path = CString::new(old.as_os_str().as_encoded_bytes()).unwrap();
        let set = unsafe {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                info.as_ptr().cast(),
                info.len(),
                0,
                0,
            )
        };
        assert_eq!(set, 0, "{}", std::io::Error::last_os_error());
        assert!(is_alias(&old));
        assert_eq!(shown_by(&old), Shown::Refused);

        let plain = dir.join("plain.pdf");
        std::fs::write(&plain, b"%PDF-1.4\n%%EOF\n").unwrap();
        assert!(!is_alias(&plain));
        assert!(!is_alias(&dir.join("gone.pdf")));
        assert_eq!(shown_by(&plain), Shown::BySystem);
    }
}
