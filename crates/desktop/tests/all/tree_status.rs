use guild_desktop::theme::{self, Mark, Selected, Theme};

#[test]
fn identifier_background_is_visible_without_losing_text_contrast_in_every_palette() {
    for theme in Theme::all() {
        let ground = theme.surface.app;
        let fill = gpui::rgb(ground).blend(theme.identifier_occurrence());
        let color = u32::from(fill) >> 8;
        assert!(
            theme::contrast(ground, color) > 1.1,
            "{:?}: occurrence must be visible",
            theme.kind
        );
        assert!(
            theme::contrast(theme.text.primary, color) >= theme::LEGIBLE,
            "{:?}: keep text legible",
            theme.kind
        );
    }
}

#[test]
fn git_rows_are_subtle_distinct_and_selection_remains_legible_in_every_palette() {
    for theme in Theme::all() {
        for ground in [theme.surface.app, theme.surface.panel] {
            let mut fills = Vec::new();
            for mark in [Mark::Modified, Mark::Added, Mark::Deleted, Mark::Untracked] {
                let fill = theme.file_row(Some(mark), Selected::No, ground);
                assert_ne!(fill, gpui::rgb(ground), "{:?}: {mark:?}", theme.kind);
                assert!(
                    !fills.contains(&fill),
                    "statuses must remain distinguishable"
                );
                fills.push(fill);
                let color = u32::from(fill) >> 8;
                assert!(theme::contrast(theme.text.primary, color) >= theme::LEGIBLE);
                assert_ne!(fill, theme.file_row(Some(mark), Selected::Focused, ground));
                assert_eq!(
                    theme.file_row(Some(mark), Selected::Focused, ground),
                    gpui::rgb(theme.accent.patina_bg)
                );
            }
            assert_eq!(
                theme.file_row(None, Selected::No, ground),
                gpui::rgb(ground)
            );
        }
    }
}

fn hex(colour: gpui::Rgba) -> u32 {
    u32::from(colour) >> 8
}

/// The selection: one solid fill, blue while the list has the keys and a
/// neutral grey while they are elsewhere — never the Git tint under it, and
/// text on either at least as legible as on the selection it replaced.
#[test]
fn a_selected_row_is_blue_with_the_keys_and_grey_without_them_in_every_palette() {
    for theme in Theme::all() {
        let ground = theme.surface.app;
        let inactive = gpui::rgb(ground)
            .blend(gpui::rgb(theme.text.primary).opacity(theme::alpha::SELECTION_INACTIVE));
        for mark in [None, Some(Mark::Modified), Some(Mark::Untracked)] {
            assert_eq!(
                theme.file_row(mark, Selected::Focused, ground),
                gpui::rgb(theme.accent.patina_bg),
                "{:?}: the focused selection replaces the Git tint",
                theme.kind
            );
            assert_eq!(
                theme.file_row(mark, Selected::Unfocused, ground),
                inactive,
                "{:?}: the unfocused selection replaces the Git tint",
                theme.kind
            );
        }
        assert_eq!(Selected::of(true, true), Selected::Focused);
        assert_eq!(Selected::of(true, false), Selected::Unfocused);
        assert_eq!(Selected::of(false, true), Selected::No);
        let inactive = hex(inactive);
        assert!(
            theme::contrast(ground, inactive) > 1.25,
            "{:?}: an unfocused selection is still seen",
            theme.kind
        );
        assert!(
            theme::contrast(theme.text.primary, inactive) >= theme::LEGIBLE,
            "{:?}",
            theme.kind
        );
    }
}

/// The pointer's row takes a soft grey laid over whatever it already
/// shows, so a Git-tinted row keeps its status under the pointer — and the
/// grey stays apart from both the ground and the unfocused selection.
#[test]
fn a_hovered_row_is_a_soft_grey_over_its_git_tint_in_every_palette() {
    for theme in Theme::all() {
        let ground = theme.surface.app;
        let over = |mark| {
            theme
                .file_row(mark, Selected::No, ground)
                .blend(gpui::rgb(theme.text.primary).opacity(theme::alpha::ROW_HOVER))
        };
        for mark in [
            None,
            Some(Mark::Modified),
            Some(Mark::Added),
            Some(Mark::Untracked),
        ] {
            assert_eq!(
                theme.file_row_hover(mark, ground),
                over(mark),
                "{:?}",
                theme.kind
            );
            assert_ne!(
                theme.file_row_hover(mark, ground),
                theme.file_row(mark, Selected::No, ground),
                "{:?}: {mark:?} lights under the pointer",
                theme.kind
            );
        }
        let hover = hex(theme.file_row_hover(None, ground));
        let inactive = hex(theme.file_row(None, Selected::Unfocused, ground));
        assert!(theme::contrast(ground, hover) > 1.15, "{:?}", theme.kind);
        assert!(
            theme::contrast(hover, inactive) > 1.08,
            "{:?}: hover is not the unfocused selection",
            theme.kind
        );
        assert!(theme::contrast(theme.text.primary, hover) >= theme::LEGIBLE);
    }
}
