//! Keyboard access for native close sheets, independent of Full Keyboard Access.
use super::*;

use std::{marker::PhantomData, rc::Rc, sync::OnceLock};

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSWindowWillBeginSheetNotification: *mut Object;
}
#[link(name = "objc")]
unsafe extern "C" {
    fn object_setClass(object: *mut Object, class: *const Class) -> *const Class;
}

extern "C" fn can_become_key_view(_: &Object, _: Sel) -> BOOL {
    YES
}

unsafe fn configure_buttons(sheet: *mut Object, buttons: &[*mut Object], initial: usize) {
    if sheet.is_null() || buttons.is_empty() || initial >= buttons.len() {
        return;
    }
    // Preserve AppKit's concrete button implementation. The subclass adds no
    // storage and changes only eligibility for the keyboard view loop, locally
    // to these buttons; Return/Escape, Space and drawing remain AppKit's.
    unsafe {
        for (ix, button) in buttons.iter().copied().enumerate() {
            let base = (*button).class();
            let name = format!("GuildCloseKeyboard_{}", base.name());
            let class = Class::get(&name).unwrap_or_else(|| {
                let mut class = objc::declare::ClassDecl::new(&name, base).unwrap();
                class.add_method(
                    sel!(canBecomeKeyView),
                    can_become_key_view as extern "C" fn(&Object, Sel) -> BOOL,
                );
                class.register()
            });
            object_setClass(button, class);
            let next = buttons[(ix + 1) % buttons.len()];
            let _: () = msg_send![button, setNextKeyView: next];
        }
        let _: () = msg_send![sheet, setAutorecalculatesKeyViewLoop: NO];
        let _: () = msg_send![sheet, setInitialFirstResponder: buttons[initial]];
        let _: BOOL = msg_send![sheet, makeFirstResponder: buttons[initial]];
    }
}

unsafe fn collect_buttons(view: *mut Object, buttons: &mut Vec<*mut Object>) {
    if view.is_null() {
        return;
    }
    unsafe {
        let button: BOOL = msg_send![view, isKindOfClass: class!(NSButton)];
        if button == YES {
            buttons.push(view);
            return;
        }
        let children: *mut Object = msg_send![view, subviews];
        let count: usize = msg_send![children, count];
        for ix in 0..count {
            let child: *mut Object = msg_send![children, objectAtIndex: ix];
            collect_buttons(child, buttons);
        }
    }
}

extern "C" fn began(observer: &Object, _: Sel, _: *mut Object) {
    // WillBeginSheet precedes attachedSheet being set. Defer once until AppKit
    // has attached and laid out the sheet; no polling or global preference change.
    unsafe {
        let _: () = msg_send![observer, performSelector: sel!(configure:) withObject: std::ptr::null_mut::<Object>() afterDelay: 0.0f64];
    }
}

extern "C" fn configure(observer: &Object, _: Sel, _: *mut Object) {
    unsafe {
        let parent = *observer.get_ivar::<usize>("parent") as *mut Object;
        let sheet: *mut Object = msg_send![parent, attachedSheet];
        if sheet.is_null() {
            return;
        }
        let content: *mut Object = msg_send![sheet, contentView];
        let mut buttons = Vec::new();
        collect_buttons(content, &mut buttons);
        buttons.sort_by_key(|button| {
            let tag: isize = msg_send![*button, tag];
            tag
        });
        let count = *observer.get_ivar::<usize>("count");
        // GPUI tags each answer with its result index. Refuse an unexpected
        // native hierarchy rather than changing unrelated sheet controls.
        if buttons.len() != count
            || buttons.iter().enumerate().any(|(ix, button)| {
                let tag: isize = msg_send![*button, tag];
                tag != ix as isize
            })
        {
            return;
        }
        configure_buttons(sheet, &buttons, *observer.get_ivar::<usize>("initial"));
    }
}

struct Observer {
    object: *mut Object,
    parent: *mut Object,
    _main_thread: PhantomData<Rc<()>>,
}
impl Observer {
    fn new(window: &gpui::Window, count: usize, initial: usize) -> Option<Self> {
        let parent = ns_window(window)?;
        Some(unsafe { Self::for_parent(parent, count, initial) })
    }

    unsafe fn for_parent(parent: *mut Object, count: usize, initial: usize) -> Self {
        static CLASS: OnceLock<&'static Class> = OnceLock::new();
        let class = CLASS.get_or_init(|| {
            let mut class =
                objc::declare::ClassDecl::new("GuildCloseSheetObserver", class!(NSObject)).unwrap();
            class.add_ivar::<usize>("parent");
            class.add_ivar::<usize>("count");
            class.add_ivar::<usize>("initial");
            unsafe {
                class.add_method(
                    sel!(began:),
                    began as extern "C" fn(&Object, Sel, *mut Object),
                );
                class.add_method(
                    sel!(configure:),
                    configure as extern "C" fn(&Object, Sel, *mut Object),
                );
            }
            class.register()
        });
        unsafe {
            let _: *mut Object = msg_send![parent, retain];
            let object: *mut Object = msg_send![*class, new];
            (*object).set_ivar("parent", parent as usize);
            (*object).set_ivar("count", count);
            (*object).set_ivar("initial", initial);
            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let _: () = msg_send![center, addObserver: object selector: sel!(began:) name: NSWindowWillBeginSheetNotification object: parent];
            Self {
                object,
                parent,
                _main_thread: PhantomData,
            }
        }
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            let _: () =
                msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget: self.object];
            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let _: () = msg_send![center, removeObserver: self.object];
            let _: () = msg_send![self.object, release];
            let _: () = msg_send![self.parent, release];
        }
    }
}

/// Keep the platform's prompt and result indices, adding keyboard access only
/// for the lifetime of this close question. Tests still use GPUI's prompt platform.
pub(crate) fn prompt(
    window: &mut gpui::Window,
    message: &str,
    detail: Option<&str>,
    answers: &[gpui::PromptButton],
    cx: &mut gpui::App,
) -> gpui::Task<Result<usize, ()>> {
    let cancel = answers.iter().position(gpui::PromptButton::is_cancel);
    let observer = Observer::new(window, answers.len(), cancel.unwrap_or(0));
    let previous = window.focused(cx);
    let asked = window.prompt(gpui::PromptLevel::Warning, message, detail, answers, cx);
    window.spawn(cx, async move |cx| {
        let answer = asked.await.map_err(|_| ());
        drop(observer);
        if answer.is_err() || answer.ok() == cancel {
            let _ = cx.update(|window, cx| {
                if let Some(previous) = previous {
                    window.focus(&previous, cx);
                }
            });
        }
        answer
    })
}

/// Native regression check: GPUI's test platform does not create AppKit sheets.
#[cfg(feature = "native-preview")]
pub fn verify_navigation() {
    for labels in [
        vec!["Close", "Cancel"],
        vec!["Save", "Don't Save", "Cancel"],
        vec!["Закрыть", "Отмена"],
    ] {
        unsafe { verify_buttons(&labels) };
    }
}

#[cfg(feature = "native-preview")]
unsafe fn verify_buttons(labels: &[&str]) {
    unsafe {
        let _: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let mut buttons = Vec::new();
        for label in labels {
            let label = CString::new(*label).unwrap();
            let title: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: label.as_ptr()];
            let button: *mut Object = msg_send![alert, addButtonWithTitle: title];
            let _: () = msg_send![button, setTag: buttons.len() as isize];
            buttons.push(button);
        }
        let escape = CString::new("\x1b").unwrap();
        let escape: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: escape.as_ptr()];
        let _: () = msg_send![buttons[buttons.len() - 1], setKeyEquivalent: escape];
        let _: () = msg_send![alert, layout];
        let sheet: *mut Object = msg_send![alert, window];
        let parent: *mut Object = msg_send![class!(NSWindow), new];
        let _: () = msg_send![parent, setReleasedWhenClosed: NO];
        let _: () = msg_send![parent, makeKeyAndOrderFront: std::ptr::null_mut::<Object>()];
        let cancel = buttons.len() - 1;
        let observer = Observer::for_parent(parent, buttons.len(), cancel);
        let _: () = msg_send![alert, beginSheetModalForWindow: parent completionHandler: std::ptr::null_mut::<Object>()];
        let run_loop: *mut Object = msg_send![class!(NSRunLoop), currentRunLoop];
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < until {
            let attached: *mut Object = msg_send![parent, attachedSheet];
            let first: *mut Object = msg_send![sheet, firstResponder];
            if attached == sheet && first == buttons[cancel] {
                break;
            }
            let date: *mut Object =
                msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.01f64];
            let _: () = msg_send![run_loop, runUntilDate: date];
        }
        let attached: *mut Object = msg_send![parent, attachedSheet];
        assert_eq!(attached, sheet, "the native sheet is actually attached");
        let first: *mut Object = msg_send![sheet, firstResponder];
        assert_eq!(
            first, buttons[cancel],
            "Cancel receives initial keyboard focus"
        );
        for expected in buttons.iter().cycle().take(buttons.len() * 2) {
            let _: () = msg_send![sheet, selectNextKeyView: std::ptr::null_mut::<Object>()];
            let first: *mut Object = msg_send![sheet, firstResponder];
            assert_eq!(first, *expected, "Tab cycles through every answer");
        }
        for expected in buttons.iter().rev().cycle().skip(1).take(buttons.len() * 2) {
            let _: () = msg_send![sheet, selectPreviousKeyView: std::ptr::null_mut::<Object>()];
            let first: *mut Object = msg_send![sheet, firstResponder];
            assert_eq!(first, *expected, "Shift+Tab cycles through every answer");
        }
        let _: () = msg_send![parent, endSheet: sheet];
        drop(observer);
        let _: () = msg_send![parent, orderOut: std::ptr::null_mut::<Object>()];
        let _: () = msg_send![parent, release];
        let _: () = msg_send![alert, release];
    }
}
