//! The macOS screen colour sampler — the customizer's eyedropper
//! (`two-themes-customize`, D8).
//!
//! `NSColorSampler` shows the system's own magnifier over every screen and
//! calls back once: with the colour under it, or with nothing when the
//! person pressed Escape. Here, because everything that touches AppKit
//! lives in `desktop::system`.
use super::*;

/// What the sampler answers, later: `Some(0xrrggbb)` for a colour taken,
/// `None` for a cancel or a colour that has no sRGB form. Dropped
/// unanswered if AppKit never calls back.
pub type Sampled = tokio::sync::oneshot::Receiver<Option<u32>>;

/// Show the sampler. Answers at once why it could not be shown, in words
/// for a notice; the colour comes through [`Sampled`].
pub fn sample() -> Result<Sampled, String> {
    let Some(class) = Class::get("NSColorSampler") else {
        return Err("The screen colour sampler needs macOS 10.15 or later".into());
    };
    let (sender, sampled) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    objc::rc::autoreleasepool(|| unsafe {
        // SAFETY: `alloc`/`init` give an owned sampler (+1), checked for
        // null. The selection handler is copied by AppKit and called once
        // on the main thread; it owns everything it captures, releases the
        // sampler it was given after answering, and reads the NSColor only
        // for the duration of the call, through null-checked conversions.
        let sampler: *mut Object = msg_send![class, alloc];
        let sampler: *mut Object = msg_send![sampler, init];
        if sampler.is_null() {
            return Err("The screen colour sampler could not be started".into());
        }
        let owned = sampler as usize;
        let handler = block2::RcBlock::new(move |colour: *mut std::ffi::c_void| {
            let picked = if colour.is_null() {
                None
            } else {
                objc::rc::autoreleasepool(|| srgb(colour.cast()))
            };
            if let Some(sender) = sender.lock().ok().and_then(|mut sender| sender.take()) {
                let _ = sender.send(picked);
            }
            let sampler = owned as *mut Object;
            let _: () = msg_send![sampler, autorelease];
        });
        let handler = block2::RcBlock::as_ptr(&handler).cast::<Object>();
        let _: () = msg_send![sampler, showSamplerWithSelectionHandler: handler];
        Ok(sampled)
    })
}

/// An NSColor's sRGB value, or `None` when it has no sRGB form — a
/// pattern, a catalogue colour that does not resolve.
///
/// # Safety
/// `colour` is a live NSColor.
unsafe fn srgb(colour: *mut Object) -> Option<u32> {
    unsafe {
        let space: *mut Object = msg_send![class!(NSColorSpace), sRGBColorSpace];
        if space.is_null() {
            return None;
        }
        let converted: *mut Object = msg_send![colour, colorUsingColorSpace: space];
        if converted.is_null() {
            return None;
        }
        let (mut r, mut g, mut b, mut a) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        let _: () = msg_send![converted, getRed: &mut r green: &mut g blue: &mut b alpha: &mut a];
        rgb_of(r, g, b)
    }
}

/// Three components from 0 to 1 as `0xrrggbb`, each rounded to its
/// nearest channel and clamped; `None` for a component that is not a
/// number.
pub fn rgb_of(r: f64, g: f64, b: f64) -> Option<u32> {
    let channel = |c: f64| {
        if c.is_finite() {
            Some((c.clamp(0.0, 1.0) * 255.0).round() as u32)
        } else {
            None
        }
    };
    Some((channel(r)? << 16) | (channel(g)? << 8) | channel(b)?)
}

#[cfg(test)]
mod tests {
    use super::rgb_of;

    #[test]
    fn components_round_to_the_nearest_channel_and_clamp() {
        assert_eq!(rgb_of(0.0, 0.0, 0.0), Some(0x000000));
        assert_eq!(rgb_of(1.0, 1.0, 1.0), Some(0xffffff));
        assert_eq!(rgb_of(0.5, 0.25, 0.75), Some(0x8040bf));
        // Extended sRGB reaches past the ends; a channel does not.
        assert_eq!(rgb_of(1.2, -0.1, 0.999), Some(0xff00ff));
        assert_eq!(rgb_of(f64::NAN, 0.0, 0.0), None);
        assert_eq!(rgb_of(0.0, f64::INFINITY, 0.0), None);
    }
}
