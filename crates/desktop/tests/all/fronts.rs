//! Which tab comes forward when a person comes back to a seat — a project,
//! a worktree, as one workspace holds it. 2026-09-26.

use guild_desktop::fronts::Fronts;

/// **The tab a person left is the tab they come back to.** Four tabs, the
/// second in front, a look at another project and back: it was the
/// fourth, because every way into a project picked the strip's last.
#[test]
fn coming_back_to_a_seat_brings_its_own_tab_forward() {
    let mut fronts = Fronts::default();
    fronts.remember("guild", 2);
    fronts.remember("meadow", 7);

    assert_eq!(fronts.returning("guild", &[1, 2, 3, 4]), Some(2));
    assert_eq!(
        fronts.returning("meadow", &[5, 6, 7]),
        Some(7),
        "each seat keeps its own — two worktrees of one repository are two"
    );
}

/// A seat never looked at — a project just opened, or one restored
/// without a tab in front — falls back to the strip's last, as before.
#[test]
fn a_seat_with_nothing_remembered_brings_its_last_tab_forward() {
    let fronts: Fronts<&str, u32> = Fronts::default();
    assert_eq!(fronts.returning("guild", &[1, 2, 3]), Some(3));
    assert_eq!(fronts.returning("guild", &[]), None, "no tabs, no front");
}

/// A remembered tab that is no longer on the strip is not brought
/// forward: it would be a front that shows nothing.
#[test]
fn a_remembered_tab_that_is_gone_is_not_brought_forward() {
    let mut fronts = Fronts::default();
    fronts.remember("guild", 9);
    assert_eq!(fronts.returning("guild", &[1, 2, 3]), Some(3));
    assert_eq!(fronts.of("guild", &[1, 2, 3]), None);
}

/// **Closed while the person was elsewhere, its neighbour stands in** —
/// the same rule as `cmd-w` on the tab in front: left, else right.
#[test]
fn closing_the_remembered_tab_remembers_its_neighbour() {
    let mut fronts = Fronts::default();
    fronts.remember("guild", 2);
    fronts.close("guild", &[1, 2, 3, 4], 2);
    assert_eq!(fronts.returning("guild", &[1, 3, 4]), Some(1));

    fronts.remember("guild", 1);
    fronts.close("guild", &[1, 3, 4], 1);
    assert_eq!(
        fronts.returning("guild", &[3, 4]),
        Some(3),
        "none on the left"
    );

    fronts.close("guild", &[3, 4], 4);
    assert_eq!(
        fronts.returning("guild", &[3]),
        Some(3),
        "another tab closing leaves the remembered one alone"
    );
}

/// A seat that moves to another workspace takes its front with it; one
/// that is closed forgets it.
#[test]
fn a_moved_seat_keeps_its_front_and_a_forgotten_one_loses_it() {
    let mut fronts = Fronts::default();
    fronts.remember("one", 2);
    fronts.moved("one", "two");
    assert_eq!(fronts.of("two", &[1, 2, 3]), Some(2));
    assert_eq!(fronts.of("one", &[1, 2, 3]), None);

    fronts.forget("two");
    assert_eq!(fronts.returning("two", &[1, 2, 3]), Some(3));
}
