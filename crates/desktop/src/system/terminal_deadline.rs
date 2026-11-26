//! Cancellable strict one-shot for pending eligible output, never a frame clock.
//! SDK dispatch/source.h: strict leeway is best effort and can cost active power.
#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    ffi::c_void,
    future::Future,
    pin::Pin,
    ptr::NonNull,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::sync::oneshot;
#[cfg(test)]
#[derive(Default)]
struct Metrics {
    fired: AtomicUsize,
    released: AtomicBool,
}

// Public libdispatch C ABI, supplied by libSystem; no new dependency/runtime.
unsafe extern "C" {
    static _dispatch_source_type_timer: u8;
    fn dispatch_get_global_queue(priority: isize, flags: usize) -> *mut c_void;
    fn dispatch_source_create(
        kind: *const c_void,
        handle: usize,
        mask: usize,
        queue: *mut c_void,
    ) -> *mut c_void;
    fn dispatch_set_context(object: *mut c_void, context: *mut c_void);
    fn dispatch_source_set_event_handler_f(
        source: *mut c_void,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_set_cancel_handler_f(
        source: *mut c_void,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_set_timer(source: *mut c_void, start: u64, interval: u64, leeway: u64);
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_resume(object: *mut c_void);
    fn dispatch_source_cancel(source: *mut c_void);
    fn dispatch_retain(object: *mut c_void);
    fn dispatch_release(object: *mut c_void);
}
struct Callback {
    source: *mut c_void,
    sender: Option<oneshot::Sender<()>>,
    #[cfg(test)]
    metrics: Arc<Metrics>,
}
extern "C" fn fire(pointer: *mut c_void) {
    // Only this source's event handler accesses sender. Cancellation waits for
    // the event handler to return before freeing Callback (SDK contract).
    let callback = unsafe { &mut *pointer.cast::<Callback>() };
    #[cfg(test)]
    callback.metrics.fired.fetch_add(1, Ordering::Relaxed);
    if let Some(sender) = callback.sender.take() {
        let _ = sender.send(());
    }
    unsafe {
        dispatch_source_cancel(callback.source);
    }
}
extern "C" fn cancelled(pointer: *mut c_void) {
    // Sole owner of this allocation and the source's original create retain.
    let callback = unsafe { Box::from_raw(pointer.cast::<Callback>()) };
    #[cfg(test)]
    callback.metrics.released.store(true, Ordering::Release);
    unsafe {
        dispatch_release(callback.source);
    }
}
pub(crate) struct Deadline {
    source: NonNull<c_void>,
    receiver: oneshot::Receiver<()>,
    #[cfg(test)]
    metrics: Arc<Metrics>,
}
impl Deadline {
    pub(crate) fn new(delay: Duration) -> Option<Self> {
        // HIGH matches GPUI's own deadline queue. STRICT=1; interval FOREVER
        // makes this one shot, never a repeating 8-ms timer. No App Nap override.
        let source = NonNull::new(unsafe {
            dispatch_source_create(
                std::ptr::addr_of!(_dispatch_source_type_timer).cast(),
                0,
                1,
                dispatch_get_global_queue(2, 0),
            )
        })?;
        let (sender, receiver) = oneshot::channel();
        #[cfg(test)]
        let metrics = Arc::new(Metrics::default());
        let callback = Box::into_raw(Box::new(Callback {
            source: source.as_ptr(),
            sender: Some(sender),
            #[cfg(test)]
            metrics: metrics.clone(),
        }));
        unsafe {
            // Guard and callback own independent retains, so drop can race fire
            // or cancellation without touching an already released source.
            dispatch_retain(source.as_ptr());
            dispatch_set_context(source.as_ptr(), callback.cast());
            dispatch_source_set_event_handler_f(source.as_ptr(), fire);
            dispatch_source_set_cancel_handler_f(source.as_ptr(), cancelled);
            dispatch_source_set_timer(
                source.as_ptr(),
                dispatch_time(0, delay.as_nanos().min(i64::MAX as u128) as i64),
                u64::MAX,
                0,
            );
            dispatch_resume(source.as_ptr());
        }
        Some(Self {
            source,
            receiver,
            #[cfg(test)]
            metrics,
        })
    }
}
impl Future for Deadline {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        Pin::new(&mut self.receiver).poll(cx).map(|_| ())
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        // Cancellation is asynchronous; cancelled frees the callback only after
        // any running event completes. A canceled sender cannot notify a view.
        unsafe {
            dispatch_source_cancel(self.source.as_ptr());
            dispatch_release(self.source.as_ptr());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn released(metrics: &Metrics) {
        let end = std::time::Instant::now() + Duration::from_secs(2);
        while !metrics.released.load(Ordering::Acquire) && std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            metrics.released.load(Ordering::Acquire),
            "cancel handler must release context"
        );
    }
    #[test]
    fn one_shot_fires_once_and_releases_context() {
        let mut timer = Deadline::new(Duration::from_millis(4)).expect("native deadline");
        let metrics = timer.metrics.clone();
        let end = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match timer.receiver.try_recv() {
                Ok(()) => break,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                    if std::time::Instant::now() < end =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                other => panic!("deadline did not deliver: {other:?}"),
            }
        }
        drop(timer);
        released(&metrics);
        assert_eq!(metrics.fired.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn dropping_pending_deadline_cancels_without_delivery() {
        let timer = Deadline::new(Duration::from_secs(60)).expect("native deadline");
        let metrics = timer.metrics.clone();
        drop(timer);
        released(&metrics);
        assert_eq!(metrics.fired.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn firing_and_drop_race_releases_each_native_context() {
        let mut all = Vec::new();
        for _ in 0..64 {
            let timer = Deadline::new(Duration::ZERO).expect("native deadline");
            all.push(timer.metrics.clone());
            drop(timer);
        }
        for metrics in all {
            released(&metrics);
            assert!(metrics.fired.load(Ordering::Relaxed) <= 1);
        }
    }
}
