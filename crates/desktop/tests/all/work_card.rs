//! The work is a card on the window's colour — M12.6, section 4.1 of the
//! design system.

use guild_desktop::scale::BASE;
use guild_desktop::theme::{radius, space};

#[test]
fn the_work_card_has_super_s_measures_and_the_document_says_them() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design-system.md");
    let document = std::fs::read_to_string(path).expect("the design system");
    let section = document
        .split("### 4.1 The work is a card")
        .nth(1)
        .expect("section 4.1");
    let px = |rems: gpui::Rems| rems.0 * BASE;

    assert!(section.contains("| the card's radius | 14 |"));
    assert_eq!(px(radius::WORK), 14.0);
    assert!(section.contains("from both sides | 6 |"));
    assert_eq!(px(space::WORK_INSET), 6.0);
    assert!(section.contains("| the gap under a card | 6 |"));
    assert_eq!(px(space::WORK_GAP), 6.0);
    // **Every gap is one width** — the person, 2026-09-18: the grey
    // between the black panels the same across as down, and the thinner.
    // It was 10 between the cards, by the same person's earlier word.
    assert!(section.contains("| the gap between the two cards | 6 |"));
    assert_eq!(px(space::WORK_BETWEEN), 6.0);
    assert_eq!(space::WORK_BETWEEN, space::WORK_GAP);
    assert_eq!(space::WORK_BETWEEN, space::WORK_INSET);
    assert!(section.contains("| the status bar's height | 28 |"));
    assert_eq!(px(space::STATUS_BAR), 28.0);
    assert!(section.contains("| the tab strip's height | 36 |"));
    assert_eq!(px(space::TABS), 36.0);
}
