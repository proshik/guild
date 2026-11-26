//! Push Commits uses the same palette as the main window.

use super::Theme;

#[derive(Debug, Clone, Copy)]
pub struct Colours {
    pub theme: Theme,
    pub link: u32,
    pub border: u32,
    pub on_accent: u32,
}

pub fn colours(theme: Theme) -> Colours {
    Colours {
        link: theme.accent.patina,
        border: theme.surface.divider,
        on_accent: theme.surface.app,
        theme,
    }
}

/// The default left share; existing saved splits win.
pub const LEFT_SHARE: f32 = 0.55;
