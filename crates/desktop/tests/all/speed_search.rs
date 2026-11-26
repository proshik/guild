use guild_desktop::speed_search::{Candidate, Search, ranges};

#[test]
fn literal_layout_and_unicode_ranges() {
    assert_eq!(ranges("Cargo.toml CARGO.toml", "cargo"), vec![0..5, 11..16]);
    assert_eq!(ranges("Cargo.toml", "сфкпщ"), vec![0..5]);
    assert_eq!(ranges("Привет.rs", "ghbdtn"), vec![0..12]);
    assert_eq!(ranges("😀ЯяЯ", "я"), vec![4..10]);
    assert_eq!(ranges("x[a].rs", "[a]"), vec![1..4]);
    assert!(ranges("cargo", "cgo").is_empty());
    assert!(ranges("cargo", "").is_empty());
}

#[test]
fn matches_wrap_retain_identity_and_reconcile_current_candidates() {
    let rows = vec![
        Candidate::new(1, "one", 0),
        Candidate::new(2, "two", 2),
        Candidate::new(3, "tone", 5),
    ];
    let mut search = Search::new("o", Some(2));
    assert_eq!(search.reconcile(&rows), Some(&2));
    assert_eq!(search.step(&rows, false), Some(&3));
    assert_eq!(search.step(&rows, false), Some(&1));
    assert_eq!(search.step(&rows, true), Some(&3));
    search.query = "one".into();
    assert_eq!(search.reconcile(&rows), Some(&3));
    assert_eq!(search.reconcile(&rows[..2]), Some(&1));
    search.query = "absent".into();
    assert_eq!(search.reconcile(&rows), None);
    search.query.clear();
    assert_eq!(search.reconcile(&rows), None);
    search.query = "o".into();
    assert_eq!(search.reconcile(&rows[1..]), Some(&2));
}

#[test]
fn refresh_discards_unavailable_identity_even_when_nothing_matches() {
    let mut search = Search::new("gone", Some(1));
    let rows = [Candidate::new(2, "remaining", 0)];
    assert_eq!(search.reconcile(&rows), None);
    assert_eq!(
        search.current, None,
        "a removed key is not a cursor for a later refresh"
    );
}

#[test]
fn layout_mapping_preserves_shift_before_case_folding() {
    assert_eq!(ranges("x{a}.rs", "ХфЪ"), vec![1..4]);
    assert_eq!(ranges("fix: speed search", "ашчЖ"), vec![0..4]);
    assert_eq!(ranges("😀ХФЪ.rs", "{A}"), vec![4..10]);
    assert_eq!(ranges("ПрИвЕт.rs", "GHBDTN"), vec![0..12]);
    assert_eq!(ranges("CARGO.toml", "СФКПЩ"), vec![0..5]);
    assert_eq!(ranges("Ёж", "~:"), vec![0..4]);
    assert_eq!(ranges("x{A}.rs", "{a}"), vec![1..4]);
    assert_eq!(ranges("ХФЪ", "хфъ"), vec![0..6]);
}

#[test]
fn shifted_number_row_and_overlapping_punctuation_keep_every_position() {
    assert_eq!(ranges("#42", "№42"), vec![0..3]);
    assert_eq!(ranges("😀№42", "#42"), vec![4..9]);
    assert_eq!(ranges("hello@", "руддщ\""), vec![0..6]);
    assert_eq!(ranges("руддщ\"", "hello@"), vec![0..11]);
    // Quote also belongs to the letter-key pair: EN quote <-> RU Э.
    assert_eq!(ranges("hello\"", "руддщЭ"), vec![0..6]);
    assert_eq!(ranges("руддщЭ", "hello\""), vec![0..12]);
    for (en, ru) in [("$", ";"), ("^", ":"), ("&", "?"), ("|", "/")] {
        assert_eq!(ranges(en, ru), vec![0..en.len()]);
        assert_eq!(ranges(ru, en), vec![0..ru.len()]);
    }
    for (en, ru) in [(";", "ж"), (":", "Ж"), ("?", ","), ("/", ".")] {
        assert_eq!(ranges(en, ru), vec![0..en.len()]);
        assert_eq!(ranges(ru, en), vec![0..ru.len()]);
    }
    assert_eq!(ranges("😀#№@\";:^&?|", "#№@\";:^&?|"), vec![4..16]);
}

#[test]
fn punctuation_shared_by_layouts_keeps_both_directional_meanings() {
    assert_eq!(ranges("😀@Э\"", "\""), vec![4..8]);
    assert_eq!(ranges("Ж^:", ":"), vec![0..4]);
    assert_eq!(ranges("?&", "&"), vec![0..2]);
}
