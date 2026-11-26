//! Bounded native-window capability observations. Never a visibility acceptance source.
use super::*;
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSWindowDidChangeOcclusionStateNotification: *mut Object;
    static NSWindowDidBecomeKeyNotification: *mut Object;
    static NSWindowDidResignKeyNotification: *mut Object;
    static NSWindowDidMiniaturizeNotification: *mut Object;
    static NSWindowDidDeminiaturizeNotification: *mut Object;
}
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::OnceLock, time::Instant};

#[derive(Clone, Debug)]
pub struct Observation {
    pub at: Instant,
    pub visible: bool,
    pub occluded: bool,
    pub key: bool,
}

struct Records {
    events: Vec<Observation>,
    lost: usize,
}

/// Owns the observer and retains only the explicitly supplied native window.
/// Main-thread only; callers keep it alive during the owned-window trial.
pub struct Observer {
    observer: *mut Object,
    window: *mut Object,
    records: Box<RefCell<Records>>,
    _main_thread: PhantomData<Rc<()>>,
}

unsafe fn sample(window: *mut Object) -> Observation {
    unsafe {
        let visible: BOOL = msg_send![window, isVisible];
        let state: u64 = msg_send![window, occlusionState];
        let key: BOOL = msg_send![window, isKeyWindow];
        Observation {
            at: Instant::now(),
            visible: visible == YES,
            occluded: state & 2 == 0,
            key: key == YES,
        }
    }
}

extern "C" fn changed(observer: &Object, _: Sel, notification: *mut Object) {
    // NSNotificationCenter invokes this synchronously on the posting thread;
    // the registered NSWindow notifications are delivered by AppKit on main.
    unsafe {
        let window: *mut Object = msg_send![notification, object];
        let records = *observer.get_ivar::<usize>("records") as *const RefCell<Records>;
        let mut records = (*records).borrow_mut();
        if records.events.len() < 4096 {
            records.events.push(sample(window));
        } else {
            records.lost += 1;
        }
    }
}

impl Observer {
    pub fn new(window: &gpui::Window) -> Option<Self> {
        let window = ns_window(window)?;
        static CLASS: OnceLock<&'static Class> = OnceLock::new();
        let class = CLASS.get_or_init(|| {
            let mut class =
                objc::declare::ClassDecl::new("GuildOwnedWindowProbe", class!(NSObject)).unwrap();
            class.add_ivar::<usize>("records");
            unsafe {
                class.add_method(
                    sel!(changed:),
                    changed as extern "C" fn(&Object, Sel, *mut Object),
                );
            }
            class.register()
        });
        let records = Box::new(RefCell::new(Records {
            events: Vec::new(),
            lost: 0,
        }));
        unsafe {
            let _: *mut Object = msg_send![window, retain];
            let observer: *mut Object = msg_send![*class, new];
            (*observer).set_ivar("records", (&*records as *const RefCell<Records>) as usize);
            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            for name in [
                NSWindowDidChangeOcclusionStateNotification,
                NSWindowDidBecomeKeyNotification,
                NSWindowDidResignKeyNotification,
                NSWindowDidMiniaturizeNotification,
                NSWindowDidDeminiaturizeNotification,
            ] {
                let _: () = msg_send![center, addObserver: observer selector: sel!(changed:) name: name object: window];
            }
            Some(Self {
                observer,
                window,
                records,
                _main_thread: PhantomData,
            })
        }
    }

    pub fn sample(&self) -> Observation {
        unsafe { sample(self.window) }
    }
    pub fn drain(&self) -> (Vec<Observation>, usize) {
        let mut records = self.records.borrow_mut();
        (
            std::mem::take(&mut records.events),
            std::mem::take(&mut records.lost),
        )
    }

    /// Tests delivery across same-turn order-out/order-front without steering other apps.
    /// These are commanded transitions, not proof of every physical occlusion.
    pub fn hide_reveal(&self) -> (Observation, Observation) {
        unsafe {
            let _: () = msg_send![self.window, orderOut: std::ptr::null_mut::<Object>()];
            let hidden = self.sample();
            let _: () = msg_send![self.window, orderFront: std::ptr::null_mut::<Object>()];
            (hidden, self.sample())
        }
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let _: () = msg_send![center, removeObserver: self.observer];
            let _: () = msg_send![self.observer, release];
            let _: () = msg_send![self.window, release];
        }
    }
}

/// Checks the calling executable without asking for permissions.
pub fn permissions() -> (bool, bool) {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { (AXIsProcessTrusted(), CGPreflightScreenCaptureAccess()) }
}

/// Move only the explicitly supplied diagnostic window, without activation.
/// Public AppKit coordinates avoid assumptions about the arrangement of screens.
/// Returns false for a removed/unknown display. Native behavior needs a trial.
pub fn move_to_display(window: &gpui::Window, display_id: u32) -> bool {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        origin: Point,
        size: Point,
    }
    unsafe impl objc::Encode for Point {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGPoint=dd}") }
        }
    }
    unsafe impl objc::Encode for Rect {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
        }
    }
    struct Move {
        window: *mut Object,
        origin: Point,
    }
    #[link(name = "System")]
    unsafe extern "C" {
        // Public dispatch_get_main_queue() macro backing symbol in queue.h.
        static _dispatch_main_q: u8;
        fn dispatch_async_f(
            queue: *const u8,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
    }
    extern "C" fn move_owned(context: *mut std::ffi::c_void) {
        let request = unsafe { Box::from_raw(context.cast::<Move>()) };
        unsafe {
            // Outside the GPUI App borrow: AppKit synchronously reports move/resize.
            let _: () = msg_send![request.window, setFrameOrigin: request.origin];
            let _: () = msg_send![request.window, release];
        }
    }
    let Some(window) = ns_window(window) else {
        return false;
    };
    unsafe {
        let screens: *mut Object = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        let key: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: c"NSScreenNumber".as_ptr()];
        for index in 0..count {
            let screen: *mut Object = msg_send![screens, objectAtIndex: index];
            let description: *mut Object = msg_send![screen, deviceDescription];
            let number: *mut Object = msg_send![description, objectForKey: key];
            if number.is_null() {
                continue;
            }
            let id: u32 = msg_send![number, unsignedIntValue];
            if id != display_id {
                continue;
            }
            let visible: Rect = msg_send![screen, visibleFrame];
            let frame: Rect = msg_send![window, frame];
            let origin = Point {
                x: visible.origin.x + (visible.size.x - frame.size.x) / 2.,
                y: visible.origin.y + (visible.size.y - frame.size.y) / 2.,
            };
            let _: *mut Object = msg_send![window, retain];
            let request = Box::new(Move { window, origin });
            dispatch_async_f(
                std::ptr::addr_of!(_dispatch_main_q),
                Box::into_raw(request).cast(),
                move_owned,
            );
            return true;
        }
    }
    false
}
