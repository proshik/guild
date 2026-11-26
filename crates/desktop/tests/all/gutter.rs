//! Where the editor's line numbers are, worked out as the component
//! works it out — 2026-09-23, so that the right button there can offer
//! *Annotate with Git Blame*.

use guild_desktop::gutter::inside;

#[test]
fn a_press_is_in_the_gutter_between_the_padding_and_its_width() {
    assert!(inside(20.0, 12.0, 50.0));
    assert!(!inside(70.0, 12.0, 50.0), "the text");
    assert!(
        inside(3.0, 12.0, 50.0),
        "the padding before the numbers is the gutter's too"
    );
    assert!(!inside(-1.0, 12.0, 50.0), "left of the editor");
}

/// Annotation keeps its check beside the file-level run action.
#[test]
fn the_gutter_menu_is_annotate_with_a_check_while_it_is_open() {
    use guild_desktop::i18n::Text;
    use guild_desktop::view::menu::gutter::{GutterDeed, gutter};
    use guild_desktop::view::menu::{Deed, Entry, Label};

    for annotated in [false, true] {
        let entries = gutter(annotated, true);
        let [
            Entry::Choice(row),
            Entry::Item(Text::RunStart, Deed::Gutter(GutterDeed::RunFile)),
        ] = entries.as_slice()
        else {
            panic!("annotation and run: {entries:?}");
        };
        assert_eq!(row.label, Label::Text(Text::MenuAnnotateWithBlame));
        assert_eq!(row.deed, Deed::Gutter(GutterDeed::Annotate));
        assert_eq!(row.check, Some(annotated));
        assert!(row.enabled);
        let unavailable = gutter(annotated, false);
        assert!(
            matches!(unavailable.as_slice(), [Entry::Choice(row)] if row.check == Some(annotated) && row.enabled)
        );
    }
}
