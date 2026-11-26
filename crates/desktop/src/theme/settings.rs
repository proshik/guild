//! Settings layout measurements, matching the approved artboard.
use super::from_px;
use gpui::Rems;
pub const NAV: Rems = from_px(180.0);
pub const CONTENT: Rems = from_px(820.0);
pub const PADDING: Rems = from_px(24.0);
pub const GROUP_GAP: Rems = from_px(28.0);
pub const CONTROL: Rems = from_px(32.0);
pub const SWITCH: Rems = from_px(38.0);
pub const SWATCH: Rems = from_px(48.0);
pub const LABEL_MIN: Rems = from_px(170.0);
pub const VALUE_WIDTH: Rems = from_px(36.0);
/// The theme customizer's hex field — `#rrggbb` in the code face, with room.
pub const HEX_FIELD: Rems = from_px(120.0);
/// The customizer's slider labels: Hue, Saturation, Intensity.
pub const CUSTOMIZER_LABEL: Rems = from_px(96.0);
/// A preset swatch's diameter.
pub const PRESET: Rems = from_px(20.0);

/// Settings boundaries remain legible against both light and dark cards.
pub fn outline(theme: super::Theme) -> gpui::Rgba {
    gpui::rgb(theme.surface.divider).blend(gpui::rgb(theme.text.faint).opacity(0.5))
}

/// Navigation shares the content background; its outline defines the boundary.
pub fn navigation(theme: super::Theme) -> gpui::Rgba {
    gpui::rgb(theme.surface.sunken)
}

/// Quiet grouped surfaces keep outlines and controls ahead of the backdrop.
pub fn card(theme: super::Theme) -> gpui::Rgba {
    gpui::rgb(theme.surface.sunken).blend(gpui::rgb(theme.surface.panel).opacity(0.35))
}

/// Internal separators are quieter than the enclosing group outline.
pub fn rule(theme: super::Theme) -> gpui::Rgba {
    gpui::rgb(theme.surface.divider).blend(gpui::rgb(theme.text.faint).opacity(0.2))
}

/// Address field width from the approved Network Settings target.
pub const NETWORK_FIELD: Rems = from_px(320.0);
