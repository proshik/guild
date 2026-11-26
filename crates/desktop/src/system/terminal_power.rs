//! Event-only power observation for the opt-in terminal scheduler.
//! The single process-lifetime Objective-C receiver has no entity/window pointers.
//! Power notifications may run off main; they only update atomics and wake weak
//! subscribers. Dropping a subscription removes it, including closed terminals.
use super::*;
use std::sync::{
    Arc, Mutex, OnceLock, Weak,
    atomic::{AtomicU8, AtomicU64},
};
use tokio::sync::Notify;
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSProcessInfoPowerStateDidChangeNotification: *mut Object;
}
static POWER: AtomicU8 = AtomicU8::new(0);
static NEXT: AtomicU64 = AtomicU64::new(0);
static LISTENERS: Mutex<Vec<(u64, Weak<Notify>)>> = Mutex::new(Vec::new());
fn read_power() -> u8 {
    unsafe {
        let info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
        let supported: BOOL = msg_send![info, respondsToSelector: sel!(isLowPowerModeEnabled)];
        if supported != YES {
            return 0;
        }
        let low: BOOL = msg_send![info, isLowPowerModeEnabled];
        if low == YES { 2 } else { 1 }
    }
}
extern "C" fn changed(_: &Object, _: Sel, _: *mut Object) {
    if let Ok(mut listeners) = LISTENERS.lock() {
        if listeners.is_empty() {
            return;
        }
        POWER.store(read_power(), Ordering::Release);
        listeners.retain(|(_, weak)| {
            if let Some(signal) = weak.upgrade() {
                signal.notify_one();
                true
            } else {
                false
            }
        });
    }
}
fn receiver() -> *mut Object {
    static RECEIVER: OnceLock<usize> = OnceLock::new();
    *RECEIVER.get_or_init(|| unsafe {
        let mut class =
            objc::declare::ClassDecl::new("GuildTerminalPowerObserver", class!(NSObject)).unwrap();
        class.add_method(
            sel!(changed:),
            changed as extern "C" fn(&Object, Sel, *mut Object),
        );
        let receiver: *mut Object = msg_send![class.register(), new];
        // Process-owned receiver has no Rust/window pointer; an in-flight native
        // callback remains safe after the last subscription unregisters.
        receiver as usize
    }) as *mut Object
}
pub(crate) struct Watch(u64);
impl Watch {
    pub(crate) fn new(signal: &Arc<Notify>) -> Self {
        let receiver = receiver();
        let mut listeners = LISTENERS.lock().unwrap();
        if listeners.is_empty() {
            unsafe {
                let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
                let info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
                let _: () = msg_send![center, addObserver: receiver selector: sel!(changed:) name: NSProcessInfoPowerStateDidChangeNotification object: info];
            }
            POWER.store(read_power(), Ordering::Release);
        }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        listeners.push((id, Arc::downgrade(signal)));
        Self(id)
    }
    pub(crate) fn normal(&self) -> Option<bool> {
        match POWER.load(Ordering::Acquire) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        let mut listeners = LISTENERS.lock().unwrap();
        listeners.retain(|(id, _)| *id != self.0);
        if listeners.is_empty() {
            unsafe {
                let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () = msg_send![center, removeObserver: receiver()];
            }
            POWER.store(0, Ordering::Release);
        }
    }
}
/// Current native state, not a promise of lossless visibility history.
/// Called on main at delivery/paint, never on the PTY reader.
pub(crate) fn state(window: &gpui::Window) -> Option<(bool, bool)> {
    let window = ns_window(window)?;
    unsafe {
        let visible: BOOL = msg_send![window, isVisible];
        let minimized: BOOL = msg_send![window, isMiniaturized];
        let state: u64 = msg_send![window, occlusionState];
        let key: BOOL = msg_send![window, isKeyWindow];
        Some((
            visible == YES && minimized != YES && state & 2 != 0,
            key == YES,
        ))
    }
}

/// Screen capability, not instantaneous VRR cadence. Main thread only.
/// Unknown screens/older systems retain Current scheduling.
pub(crate) fn maximum_frames_per_second(window: &gpui::Window) -> Option<u32> {
    let window = ns_window(window)?;
    unsafe {
        let screen: *mut Object = msg_send![window, screen];
        if screen.is_null() {
            return None;
        }
        let supported: BOOL = msg_send![screen, respondsToSelector: sel!(maximumFramesPerSecond)];
        if supported != YES {
            return None;
        }
        let fps: isize = msg_send![screen, maximumFramesPerSecond];
        u32::try_from(fps).ok().filter(|fps| *fps > 0)
    }
}
