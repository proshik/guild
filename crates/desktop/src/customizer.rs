//! The customizer as a model — `two-themes-customize`, D7. Pure: what is
//! being edited, the unsaved choices of both palettes, and what Save,
//! Reset and *Copy from* do with them. The window draws it and previews
//! [`Draft::preview`]; closing is dropping the draft.

use guild_session::settings::{Customization, ThemeCustom, Tint};

use crate::theme::{Kind, PRESETS, tint};

/// The three tabs, in the order the card shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Every surface, and text a little.
    Background,
    /// `accent.patina` and what is drawn from it.
    Accent,
    /// Selection and match grounds.
    Highlight,
}

impl Tab {
    /// All three, in order.
    pub const ALL: [Tab; 3] = [Tab::Background, Tab::Accent, Tab::Highlight];
}

/// The intensity a tab's first colour starts at.
pub fn default_intensity(tab: Tab) -> u8 {
    match tab {
        Tab::Background => 40,
        Tab::Accent => 100,
        Tab::Highlight => 70,
    }
}

/// A hex colour as a person types it: `#rgb` or `#rrggbb`, with or without
/// the `#`, in any case, with surrounding spaces.
pub fn parse_hex(text: &str) -> Option<u32> {
    Tint::parse_color(text)
}

/// The customizer while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    editing: Kind,
    tab: Tab,
    light: Customization,
    dark: Customization,
    saved: ThemeCustom,
}

impl Draft {
    /// Open on the palette in effect, with the saved choices as the draft.
    pub fn open(saved: ThemeCustom, in_effect: Kind) -> Draft {
        Draft {
            editing: in_effect,
            tab: Tab::Background,
            light: saved.light,
            dark: saved.dark,
            saved,
        }
    }

    /// Which palette is being edited.
    pub fn editing(&self) -> Kind {
        self.editing
    }

    /// Which tab is in front.
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Edit the other palette; both drafts are kept.
    pub fn edit(&mut self, kind: Kind) {
        self.editing = kind;
    }

    /// Bring a tab to the front.
    pub fn select_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    /// A palette's unsaved choices.
    pub fn customization(&self, kind: Kind) -> &Customization {
        match kind {
            Kind::Light => &self.light,
            Kind::Dark => &self.dark,
        }
    }

    fn edited(&mut self) -> &mut Customization {
        match self.editing {
            Kind::Light => &mut self.light,
            Kind::Dark => &mut self.dark,
        }
    }

    fn slot(&mut self) -> &mut Option<Tint> {
        let tab = self.tab;
        let custom = self.edited();
        match tab {
            Tab::Background => &mut custom.background,
            Tab::Accent => &mut custom.accent,
            Tab::Highlight => &mut custom.highlight,
        }
    }

    /// The edited tab's choice, if it has one.
    pub fn current(&self) -> Option<Tint> {
        let custom = self.customization(self.editing);
        match self.tab {
            Tab::Background => custom.background,
            Tab::Accent => custom.accent,
            Tab::Highlight => custom.highlight,
        }
    }

    /// Choose a colour for the edited tab: a first one at the tab's default
    /// intensity, a later one keeping the intensity there is.
    pub fn set_color(&mut self, color: u32) {
        let intensity = self
            .current()
            .map_or(default_intensity(self.tab), |tint| tint.intensity);
        *self.slot() = Some(Tint { color, intensity });
    }

    /// The edited tab's intensity — nothing while the tab has no colour.
    pub fn set_intensity(&mut self, intensity: u8) {
        let intensity = intensity.min(guild_session::settings::MAX_INTENSITY);
        if let Some(tint) = self.slot() {
            tint.intensity = intensity;
        }
    }

    /// The *Default* swatch: the edited tab back to the built-in look.
    pub fn clear_tab(&mut self) {
        *self.slot() = None;
    }

    /// The edited palette, all three tabs, back to the built-in palette.
    pub fn reset(&mut self) {
        *self.edited() = Customization::default();
    }

    /// *Copy from Light* / *Copy from Dark*: the other palette's **saved**
    /// choices into this one's draft, to be computed over this palette.
    pub fn copy_from_other(&mut self) {
        let other = *self.saved.of(self.editing.other());
        *self.edited() = other;
    }

    /// Whether a palette's draft differs from what is saved.
    pub fn dirty(&self, kind: Kind) -> bool {
        self.customization(kind) != self.saved.of(kind)
    }

    /// *Save Light Theme* / *Save Dark Theme*: the edited palette's draft
    /// into `settings`, and the draft clean for that palette. The other
    /// palette's draft stays unsaved.
    pub fn save(&mut self, settings: &mut ThemeCustom) {
        let kind = self.editing;
        let draft = *self.customization(kind);
        *settings.of_mut(kind) = draft;
        *self.saved.of_mut(kind) = draft;
    }

    /// What the window draws while the customizer is open.
    pub fn preview(&self) -> (Kind, Customization) {
        (self.editing, *self.customization(self.editing))
    }

    /// What closing returns to: the saved choices, which nothing here
    /// changed except Save.
    pub fn saved(&self) -> ThemeCustom {
        self.saved
    }

    /// The edited tab's colour as a hue in degrees and a saturation from
    /// 0 to 100 — its chroma against the most sRGB has at its lightness
    /// and hue.
    pub fn hue_saturation(&self) -> Option<(f32, f32)> {
        let lab = tint::to_lab(self.current()?.color);
        let hue = lab.hue().to_degrees().rem_euclid(360.0);
        let most = tint::max_chroma(lab.l, lab.hue());
        let saturation = if most <= 1e-6 {
            0.0
        } else {
            (lab.chroma() / most * 100.0).clamp(0.0, 100.0)
        };
        Some((hue, saturation))
    }

    /// The colour the hue and saturation rows say, at the current colour's
    /// lightness — or the first preset's, when the tab has none yet.
    fn rebuild(&mut self, hue: f32, saturation: f32) {
        let seed = self.current().map_or(PRESETS[0].1, |tint| tint.color);
        let l = tint::to_lab(seed).l;
        let angle = hue.to_radians();
        let chroma = tint::max_chroma(l, angle) * (saturation / 100.0).clamp(0.0, 1.0);
        let color = tint::to_rgb(tint::Lab {
            l,
            a: chroma * angle.cos(),
            b: chroma * angle.sin(),
        });
        self.set_color(color);
    }

    /// Turn the edited tab's colour to `hue` degrees.
    pub fn set_hue(&mut self, hue: f32) {
        let saturation = self.hue_saturation().map_or(100.0, |(_, s)| s);
        self.rebuild(hue, saturation);
    }

    /// Set the edited tab's saturation, 0 to 100.
    pub fn set_saturation(&mut self, saturation: f32) {
        let hue = self
            .hue_saturation()
            .map_or_else(|| tint::to_lab(PRESETS[0].1).hue().to_degrees(), |(h, _)| h);
        self.rebuild(hue, saturation);
    }
}
