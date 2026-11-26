//! Staging one shown change: which index lines a write replaces, and with
//! what — `guild_desktop::hunk`, pure. Decision 55: the unit is the block on
//! screen, and a write never changes an index line outside it.

use guild_desktop::diff::document::{Block, Document};
use guild_desktop::hunk::{Action, Comparisons, Offer, View};

fn lines(range: std::ops::Range<usize>, tag: &str) -> String {
    range.map(|i| format!("{tag} {i}\n")).collect()
}

fn shown(old: &str, new: &str) -> Vec<Block> {
    Document::new(old, new).blocks
}

#[test]
fn the_unstaged_diff_stages_one_block_and_keeps_what_is_already_staged() {
    let head = "a\nb\nc\nd\ne\nf\ng\n";
    // `a` is already staged as `A`; the working copy also changes c and f.
    let index = "A\nb\nc\nd\ne\nf\ng\n";
    let working = "A\nb\nC\nd\ne\nF\ng\n";
    let blocks = shown(index, working);
    assert_eq!(blocks.len(), 2);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Unstaged, &blocks[0]), Offer::Stage);
    let (action, post) = compared.postimage(View::Unstaged, &blocks[1]).unwrap();
    assert_eq!(action, Action::Stage);
    assert_eq!(post, "A\nb\nc\nd\ne\nF\ng\n");
}

#[test]
fn the_staged_diff_unstages_one_block_and_keeps_the_other_staged() {
    let head = "a\nb\nc\nd\ne\nf\ng\n";
    let index = "A\nb\nc\nd\ne\nF\ng\n";
    let working = index;
    let blocks = shown(head, index);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Staged, &blocks[1]), Offer::Unstage);
    let (action, post) = compared.postimage(View::Staged, &blocks[0]).unwrap();
    assert_eq!(action, Action::Unstage);
    assert_eq!(post, "a\nb\nc\nd\ne\nF\ng\n");
}

/// Review finding `lenses-3`: two staged replacements one line apart are
/// one `-U3` hunk to git; the HEAD diff shows only the first once the
/// working copy restores the second. Unstaging the shown one must keep the
/// second, which now exists only in the index.
#[test]
fn a_head_diff_unstage_keeps_a_staged_change_the_screen_does_not_show() {
    let head = lines(1..13, "l");
    let index = head.replace("l 5\n", "L5\n").replace("l 7\n", "L7\n");
    let working = head.replace("l 5\n", "L5\n");
    let blocks = shown(&head, &working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(&head, &index, &working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Unstage);
    let (_, post) = compared.postimage(View::Head, &blocks[0]).unwrap();
    assert_eq!(post, head.replace("l 7\n", "L7\n"));
}

#[test]
fn a_staged_change_wider_than_the_shown_block_disables_the_action() {
    let head = lines(1..13, "l");
    let index = head
        .replace("l 5\n", "L5\n")
        .replace("l 6\n", "L6\n")
        .replace("l 7\n", "L7\n");
    // The working copy keeps L5 and takes 6 and 7 back.
    let working = head.replace("l 5\n", "L5\n");
    let blocks = shown(&head, &working);
    let compared = Comparisons::new(&head, &index, &working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Wider);
    assert_eq!(compared.postimage(View::Head, &blocks[0]), None);
}

#[test]
fn an_unstaged_change_wider_than_the_shown_block_disables_stage() {
    let head = lines(1..13, "l");
    // The index already has 5..7 replaced; the working copy goes back to
    // HEAD for 6 and 7 and changes 5 again: the staged change straddles the
    // shown block's edge.
    let index = head
        .replace("l 5\n", "L5\n")
        .replace("l 6\n", "L6\n")
        .replace("l 7\n", "L7\n");
    let working = head.replace("l 5\n", "M5\n");
    let blocks = shown(&head, &working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(&head, &index, &working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Wider);
}

/// A block partly in the index is offered nothing: staging "the rest" can
/// take out an index line the screen does not show. Decision 55, review r1.
#[test]
fn a_head_diff_block_partly_in_the_index_is_mixed() {
    let head = "x\ny\nz\n";
    let index = "X\ny\nz\n";
    let working = "X\nY\nz\n";
    let blocks = shown(head, working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Mixed);
    assert_eq!(compared.postimage(View::Head, &blocks[0]), None);
}

/// Review r1 `bugs+impl-2`: an index-only line at the block's edge is part
/// of no shown change, and a Stage must not take it out.
#[test]
fn an_index_only_line_after_the_shown_change_is_not_erased() {
    let head = "a\nb\nc\n";
    let index = "a\nB\nextra\nc\n";
    let working = "a\nB\nc\n";
    let blocks = shown(head, working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Mixed);
    // A staged insertion right after an unstaged change stays where it is.
    let index = "a\nb\nextra\nc\n";
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Stage);
    let (_, post) = compared.postimage(View::Head, &blocks[0]).unwrap();
    assert_eq!(post, "a\nB\nextra\nc\n");
}

/// Review r1 `arch+quality-1`: a staged insertion right before the shown
/// change is not consumed by its replacement.
#[test]
fn a_staged_insertion_before_the_shown_change_survives_its_stage() {
    let head = "a\nb\nc\n";
    let index = "a\nextra\nb\nc\n";
    let working = "a\nB\nc\n";
    let blocks = shown(head, working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Stage);
    let (_, post) = compared.postimage(View::Head, &blocks[0]).unwrap();
    assert_eq!(post, "a\nextra\nB\nc\n");
}

/// Review r1 `bugs+impl-8`: the shown change is staged as `A\n` and the disk
/// says `A\r\n`; the bytes are not wholly staged, so no Unstage is offered.
#[test]
fn a_line_ending_difference_is_not_wholly_staged() {
    let head = "a\nb\n";
    let index = "A\nb\n";
    let working = "A\r\nb\r\n";
    let blocks = shown(head, working);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::LineEndings);
}

/// Review r1 `adversarial-10`: offering every block of a large diff is not
/// quadratic in the blocks of `HEAD` ↔ index. The shown blocks are built by
/// hand, so a diff deadline cannot coarsen them.
#[test]
fn offering_many_blocks_is_not_quadratic() {
    let count = 10_000;
    let head: String = (0..count).map(|i| format!("same {i}\nold {i}\n")).collect();
    let index: String = (0..count).map(|i| format!("same {i}\nnew {i}\n")).collect();
    let working = index.clone();
    let blocks: Vec<Block> = (0..count)
        .map(|i| Block {
            old: 2 * i + 1..2 * i + 2,
            new: 2 * i + 1..2 * i + 2,
        })
        .collect();
    let compared = Comparisons::new(&head, &index, &working).unwrap();
    let started = std::time::Instant::now();
    for block in &blocks {
        assert_eq!(compared.offer(View::Head, block), Offer::Unstage);
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "{:?} for {count} blocks",
        started.elapsed()
    );
}

#[test]
fn a_head_diff_offers_stage_for_unstaged_and_unstage_for_staged_blocks() {
    let head = "a\nb\nc\nd\ne\n";
    let index = "A\nb\nc\nd\ne\n";
    let working = "A\nb\nc\nd\nE\n";
    let blocks = shown(head, working);
    let compared = Comparisons::new(head, index, working).unwrap();
    let offers: Vec<_> = blocks
        .iter()
        .map(|b| compared.offer(View::Head, b))
        .collect();
    assert_eq!(offers, [Offer::Unstage, Offer::Stage]);
}

/// Review finding `lenses-4`: an identical block repeats, lines are inserted
/// above, and the change to the first copy must land on the first copy.
#[test]
fn a_change_lands_on_its_own_block_when_identical_blocks_repeat() {
    let block = "  x = 1;\n  y = 2;\n  z = 3;\n  w = 4;\n  v = 5;\n";
    let index = format!(
        "{}{block}{}{block}{}",
        lines(0..39, "line"),
        lines(0..13, "mid"),
        lines(0..13, "tail")
    );
    let changed = block.replace("  z = 3;\n", "  z = 30;\n");
    let working = format!(
        "{}{}{changed}{}{block}{}",
        lines(0..20, "new"),
        lines(0..39, "line"),
        lines(0..13, "mid"),
        lines(0..13, "tail")
    );
    let blocks = shown(&index, &working);
    assert_eq!(blocks.len(), 2, "the insertion and the change");
    let compared = Comparisons::new(&index, &index, &working).unwrap();
    let (_, post) = compared.postimage(View::Unstaged, &blocks[1]).unwrap();
    let expected = format!(
        "{}{changed}{}{block}{}",
        lines(0..39, "line"),
        lines(0..13, "mid"),
        lines(0..13, "tail")
    );
    assert_eq!(post, expected);
    // And the other way: the first copy's change taken back out of an index
    // that has the insertion staged above it.
    let staged = working.clone();
    let compared = Comparisons::new(&index, &staged, &staged).unwrap();
    let blocks = shown(&index, &staged);
    let (_, post) = compared.postimage(View::Staged, &blocks[1]).unwrap();
    assert_eq!(
        post,
        format!(
            "{}{}{block}{}{block}{}",
            lines(0..20, "new"),
            lines(0..39, "line"),
            lines(0..13, "mid"),
            lines(0..13, "tail")
        )
    );
}

#[test]
fn line_endings_and_a_missing_final_newline_are_the_files_own() {
    let head = "a\r\nb\r\nc\r\nd";
    let index = head;
    let working = "a\r\nB\r\nc\r\nd\r\ne";
    let blocks = shown(index, working);
    assert_eq!(blocks.len(), 2);
    let compared = Comparisons::new(head, index, working).unwrap();
    let (_, post) = compared.postimage(View::Unstaged, &blocks[0]).unwrap();
    assert_eq!(post, "a\r\nB\r\nc\r\nd");
    let (_, post) = compared.postimage(View::Unstaged, &blocks[1]).unwrap();
    assert_eq!(post, "a\r\nb\r\nc\r\nd\r\ne");
}

#[test]
fn a_lone_carriage_return_is_not_offered_hunks() {
    assert!(Comparisons::new("a\rb\n", "a\rb\n", "a\rB\n").is_none());
}

/// Review r2 `bugs+impl-3`: a write never joins its bytes to an index line
/// it keeps — an unterminated replacement before a kept line, or an append
/// after an unterminated index line.
#[test]
fn a_write_never_joins_a_kept_index_line() {
    let head = "a\nb\n";
    let index = "a\nb\nextra\n";
    let working = "a\nB";
    let blocks = shown(head, working);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Mixed);
    assert_eq!(compared.postimage(View::Head, &blocks[0]), None);
    let index = "a\nb";
    let working = "a\nb\nc\n";
    let blocks = shown(head, working);
    let compared = Comparisons::new(head, index, working).unwrap();
    for block in &blocks {
        if let Some((_, post)) = compared.postimage(View::Head, block) {
            assert!(!post.contains("bc"), "joined: {post:?}");
        }
    }
    // The Unstaged diff's own blocks are kept to the same rule.
    let blocks = shown(index, working);
    for block in &blocks {
        if let Some((_, post)) = compared.postimage(View::Unstaged, block) {
            assert!(!post.contains("bc"), "joined: {post:?}");
        }
    }
}

/// Review r2 `bugs+impl-4`: an insertion wholly in the index is unstaged
/// from the HEAD view.
#[test]
fn a_fully_staged_insertion_is_unstaged_from_the_head_view() {
    let head = "a\nb\n";
    let index = "a\nnew\nb\n";
    let working = index;
    let blocks = shown(head, working);
    assert_eq!(blocks.len(), 1);
    let compared = Comparisons::new(head, index, working).unwrap();
    assert_eq!(compared.offer(View::Head, &blocks[0]), Offer::Unstage);
    let (_, post) = compared.postimage(View::Head, &blocks[0]).unwrap();
    assert_eq!(post, head);
}
