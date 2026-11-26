//! OKLab, and moving a colour towards another in it — the arithmetic a
//! person's tint is computed with (`two-themes-customize`, D4).
//!
//! **Our own, not a crate's.** Björn Ottosson's published matrices, in
//! f64; a dependency update can never shift the colours a person saved.
//!
//! **Every colour that leaves here is in sRGB by construction.** A value
//! outside it is brought in by shrinking its chroma at the same lightness
//! and hue, never by clipping channels, which would move both: chroma 0 at
//! any lightness in [0, 1] is a grey, and every grey is in sRGB, so the
//! shrinking always ends inside (spec review r1, finding 1).

/// A colour in OKLab: `l` is perceptual lightness from 0 to 1, `a` and `b`
/// the two opponent axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    /// Lightness, 0 to 1.
    pub l: f32,
    /// Green (−) to red (+).
    pub a: f32,
    /// Blue (−) to yellow (+).
    pub b: f32,
}

impl Lab {
    /// How far from grey.
    pub fn chroma(self) -> f32 {
        self.a.hypot(self.b)
    }

    /// The hue angle, in radians.
    pub fn hue(self) -> f32 {
        self.b.atan2(self.a)
    }

    fn scaled(self, k: f64) -> Lab {
        Lab {
            l: self.l,
            a: (f64::from(self.a) * k) as f32,
            b: (f64::from(self.b) * k) as f32,
        }
    }
}

fn decode(channel: u32) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(linear: f64) -> u32 {
    let c = linear.clamp(0.0, 1.0);
    let c = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u32
}

/// `0xrrggbb` in OKLab.
pub fn to_lab(colour: u32) -> Lab {
    let (r, g, b) = (
        decode((colour >> 16) & 0xff),
        decode((colour >> 8) & 0xff),
        decode(colour & 0xff),
    );
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    Lab {
        l: (0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s) as f32,
        a: (1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s) as f32,
        b: (0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s) as f32,
    }
}

/// Linear sRGB, which may be outside [0, 1] when the colour is outside sRGB.
fn linear(lab: Lab) -> [f64; 3] {
    let (l, a, b) = (f64::from(lab.l), f64::from(lab.a), f64::from(lab.b));
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
        -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_695_601 * s_,
    ]
}

/// Inside sRGB, within what rounding to a channel cannot see.
pub fn in_gamut(lab: Lab) -> bool {
    const EPS: f64 = 1e-4;
    linear(lab).iter().all(|c| (-EPS..=1.0 + EPS).contains(c))
}

/// Back to `0xrrggbb`, bringing a colour outside sRGB in by its chroma
/// alone — the same lightness and hue, less colour.
pub fn to_rgb(lab: Lab) -> u32 {
    let lab = Lab {
        l: lab.l.clamp(0.0, 1.0),
        ..lab
    };
    let lab = if in_gamut(lab) {
        lab
    } else {
        // `lo` is inside (chroma 0 is a grey), `hi` outside.
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            if in_gamut(lab.scaled(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lab.scaled(lo)
    };
    let [r, g, b] = linear(lab);
    (encode(r) << 16) | (encode(g) << 8) | encode(b)
}

/// `token` with its lightness kept and its (a, b) moved towards `seed`'s
/// hue, at a chroma of at most `cap`, by `intensity × share`.
///
/// **Intensity 0 is `token` itself**, without a round trip through OKLab.
/// An achromatic seed moves (a, b) towards 0: a neutral tint desaturates.
pub fn tint(token: u32, seed: u32, cap: f32, share: f32, intensity: f32) -> u32 {
    if intensity <= 0.0 {
        return token;
    }
    let from = to_lab(token);
    let seed = to_lab(seed);
    let chroma = seed.chroma();
    let (a_t, b_t) = if chroma < 1e-4 {
        (0.0, 0.0)
    } else {
        let k = chroma.min(cap) / chroma;
        (seed.a * k, seed.b * k)
    };
    let f = (intensity * share).clamp(0.0, 1.0);
    to_rgb(Lab {
        l: from.l,
        a: from.a + (a_t - from.a) * f,
        b: from.b + (b_t - from.b) * f,
    })
}

/// `from` moved towards `to` in all three of L, a and b. The ends are the
/// colours themselves.
pub fn mix(from: u32, to: u32, intensity: f32) -> u32 {
    if intensity <= 0.0 {
        return from;
    }
    if intensity >= 1.0 {
        return to;
    }
    let (x, y) = (to_lab(from), to_lab(to));
    let lerp = |p: f32, q: f32| p + (q - p) * intensity;
    to_rgb(Lab {
        l: lerp(x.l, y.l),
        a: lerp(x.a, y.a),
        b: lerp(x.b, y.b),
    })
}

/// The lowest WCAG contrast of `ink` on any of `grounds`.
pub fn worst_contrast(ink: u32, grounds: &[u32]) -> f32 {
    grounds
        .iter()
        .map(|&ground| super::contrast(ink, ground))
        .fold(f32::INFINITY, f32::min)
}

/// `colour`, or the nearest colour of its hue that reads at `floor` on
/// every one of `grounds`: lightness moved towards white (`lighten`) or
/// black, and chroma reduced only as far as sRGB requires at each
/// lightness. The far end — white or black — passes whenever the built-in
/// accent did, and the search keeps a passing end, so the answer passes.
pub fn legible(colour: u32, grounds: &[u32], floor: f32, lighten: bool) -> u32 {
    let passes = |c: u32| worst_contrast(c, grounds) >= floor;
    if passes(colour) {
        return colour;
    }
    let lab = to_lab(colour);
    let end = if lighten { 1.0 } else { 0.0 };
    let candidate = |t: f32| {
        to_rgb(Lab {
            l: lab.l + (end - lab.l) * t,
            ..lab
        })
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    if !passes(candidate(hi)) {
        return candidate(hi);
    }
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        if passes(candidate(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    candidate(hi)
}

/// The strongest intensity in [0, `intensity`] whose result `passes`,
/// given that 0 passes (it is the built-in palette). Twelve halvings.
pub fn strongest(intensity: f32, passes: impl Fn(f32) -> bool) -> f32 {
    if intensity <= 0.0 || passes(intensity) {
        return intensity;
    }
    let (mut lo, mut hi) = (0.0_f32, intensity);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if passes(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The most chroma sRGB has at lightness `l` and hue `hue` (radians).
pub fn max_chroma(l: f32, hue: f32) -> f32 {
    let (cos, sin) = (hue.cos(), hue.sin());
    let at = |c: f32| Lab {
        l,
        a: c * cos,
        b: c * sin,
    };
    let (mut lo, mut hi) = (0.0_f32, 0.5_f32);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        if in_gamut(at(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}
