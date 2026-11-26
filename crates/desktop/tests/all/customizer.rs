//! The customizer without a window — `two-themes-customize`, D7: what is
//! being edited, what is unsaved, and what Save, Reset, Copy from and
//! closing do with it.

use guild_desktop::customizer::{Draft, Tab, default_intensity, parse_hex};
use guild_desktop::theme::Kind;
use guild_session::settings::{Customization, ThemeCustom, Tint};

fn saved() -> ThemeCustom {
    let mut custom = ThemeCustom::default();
    custom.light.background = Some(Tint {
        color: 0x3b82f6,
        intensity: 40,
    });
    custom
}

#[test]
fn it_opens_on_the_kind_in_effect_with_the_saved_choices() {
    let draft = Draft::open(saved(), Kind::Dark);
    assert_eq!(draft.editing(), Kind::Dark);
    assert_eq!(draft.tab(), Tab::Background);
    assert_eq!(draft.customization(Kind::Light), &saved().light);
    assert!(!draft.dirty(Kind::Light) && !draft.dirty(Kind::Dark));
    assert_eq!(draft.preview(), (Kind::Dark, Customization::default()));
}

#[test]
fn a_choice_changes_only_the_edited_tab_of_the_edited_kind() {
    let mut draft = Draft::open(saved(), Kind::Dark);
    draft.select_tab(Tab::Accent);
    draft.set_color(0xf97316);
    let dark = *draft.customization(Kind::Dark);
    assert_eq!(
        dark.accent,
        Some(Tint {
            color: 0xf97316,
            intensity: default_intensity(Tab::Accent)
        }),
        "a first colour takes the tab's default intensity"
    );
    assert_eq!(dark.background, None);
    assert_eq!(dark.highlight, None);
    assert_eq!(draft.customization(Kind::Light), &saved().light);
    draft.set_intensity(55);
    assert_eq!(
        draft.customization(Kind::Dark).accent.unwrap().intensity,
        55
    );
    draft.set_color(0x22c55e);
    assert_eq!(
        draft.customization(Kind::Dark).accent.unwrap(),
        Tint {
            color: 0x22c55e,
            intensity: 55
        },
        "a new colour keeps the intensity"
    );
    assert!(draft.dirty(Kind::Dark));
    assert!(!draft.dirty(Kind::Light));
}

#[test]
fn intensity_without_a_colour_changes_nothing() {
    let mut draft = Draft::open(ThemeCustom::default(), Kind::Light);
    draft.select_tab(Tab::Highlight);
    draft.set_intensity(80);
    assert!(!draft.dirty(Kind::Light));
    assert_eq!(draft.current(), None);
}

#[test]
fn defaults_per_tab() {
    assert_eq!(default_intensity(Tab::Background), 40);
    assert_eq!(default_intensity(Tab::Accent), 100);
    assert_eq!(default_intensity(Tab::Highlight), 70);
}

#[test]
fn the_default_swatch_clears_the_tab() {
    let mut draft = Draft::open(saved(), Kind::Light);
    draft.clear_tab();
    assert_eq!(draft.customization(Kind::Light).background, None);
    assert!(draft.dirty(Kind::Light));
}

#[test]
fn switching_kinds_keeps_both_drafts_and_previews_the_edited_one() {
    let mut draft = Draft::open(ThemeCustom::default(), Kind::Dark);
    draft.set_color(0xf43f5e);
    draft.edit(Kind::Light);
    draft.set_color(0x14b8a6);
    assert_eq!(draft.editing(), Kind::Light);
    assert_eq!(draft.preview().0, Kind::Light);
    assert_eq!(draft.preview().1.background.unwrap().color, 0x14b8a6);
    draft.edit(Kind::Dark);
    assert_eq!(
        draft.customization(Kind::Dark).background.unwrap().color,
        0xf43f5e
    );
    assert!(draft.dirty(Kind::Light) && draft.dirty(Kind::Dark));
}

#[test]
fn save_writes_the_edited_kind_only_and_it_is_clean_after() {
    let mut draft = Draft::open(ThemeCustom::default(), Kind::Dark);
    draft.set_color(0xf43f5e);
    draft.edit(Kind::Light);
    draft.set_color(0x14b8a6);
    let mut settings = ThemeCustom::default();
    draft.save(&mut settings);
    assert_eq!(settings.light.background.unwrap().color, 0x14b8a6);
    assert_eq!(
        settings.dark,
        Customization::default(),
        "Dark is not saved by Save Light"
    );
    assert!(!draft.dirty(Kind::Light));
    assert!(draft.dirty(Kind::Dark));
}

#[test]
fn reset_returns_the_edited_kind_to_the_built_in_palette() {
    let mut draft = Draft::open(saved(), Kind::Light);
    draft.reset();
    assert!(draft.customization(Kind::Light).is_empty());
    let mut settings = saved();
    draft.save(&mut settings);
    assert!(
        settings.light.is_empty(),
        "Reset + Save leaves no customization"
    );
}

#[test]
fn copy_from_takes_the_other_kind_s_saved_choices() {
    let mut draft = Draft::open(saved(), Kind::Dark);
    // An unsaved change to Light is not what is copied.
    draft.edit(Kind::Light);
    draft.set_color(0x000000);
    draft.edit(Kind::Dark);
    draft.copy_from_other();
    assert_eq!(draft.customization(Kind::Dark), &saved().light);
    assert!(draft.dirty(Kind::Dark));
}

#[test]
fn hex_is_read_in_every_usual_spelling() {
    assert_eq!(parse_hex("#3b82f6"), Some(0x3b82f6));
    assert_eq!(parse_hex("3B82F6"), Some(0x3b82f6));
    assert_eq!(parse_hex(" #abc "), Some(0xaabbcc));
    assert_eq!(parse_hex("abc"), Some(0xaabbcc));
    for invalid in ["", "#", "#12345", "#1234567", "blue", "#ggg", "#12 345"] {
        assert_eq!(parse_hex(invalid), None, "{invalid:?}");
    }
}

#[test]
fn hue_and_saturation_round_trip_and_move_one_thing_each() {
    let mut draft = Draft::open(ThemeCustom::default(), Kind::Dark);
    draft.set_color(0x3b82f6);
    let (hue, saturation) = draft.hue_saturation().unwrap();
    draft.set_hue(hue);
    let again = draft.hue_saturation().unwrap();
    assert!((again.0 - hue).abs() < 2.0 && (again.1 - saturation).abs() < 2.0);
    draft.set_saturation(0.0);
    let grey = draft.current().unwrap().color;
    let (r, g, b) = ((grey >> 16) & 0xff, (grey >> 8) & 0xff, grey & 0xff);
    assert!(
        r.abs_diff(g) <= 2 && g.abs_diff(b) <= 2,
        "no saturation is grey: #{grey:06x}"
    );
    // A hue with no colour yet starts from the first preset.
    let mut empty = Draft::open(ThemeCustom::default(), Kind::Dark);
    empty.select_tab(Tab::Accent);
    empty.set_hue(120.0);
    assert!(empty.current().is_some());
}

#[test]
fn closing_drops_both_drafts() {
    let mut draft = Draft::open(saved(), Kind::Dark);
    draft.set_color(0xf43f5e);
    draft.edit(Kind::Light);
    draft.clear_tab();
    let saved_now = draft.saved();
    assert_eq!(
        saved_now,
        saved(),
        "what closing returns to is what was saved"
    );
}
