//! A split that drags — M8.3.6, decision 8.

use guild_session::split::Split;

#[test]
fn a_split_keeps_both_sides_at_least_their_minimum() {
    let split = Split::new(0.5);
    assert_eq!(split.sizes(1000.0, 160.0), (500.0, 500.0));

    let narrow = Split::new(0.05);
    assert_eq!(narrow.sizes(1000.0, 160.0), (160.0, 840.0));
    let wide = Split::new(0.99);
    assert_eq!(wide.sizes(1000.0, 160.0), (840.0, 160.0));

    // Too small for both minimums: shared in the fraction's proportion.
    assert_eq!(Split::new(0.5).sizes(200.0, 160.0), (100.0, 100.0));
}

#[test]
fn a_split_dragged_past_an_end_stops_there() {
    let mut split = Split::new(0.5);
    split.drag(100.0, 1000.0, 160.0);
    assert_eq!(split.fraction(), 0.6);

    split.drag(10_000.0, 1000.0, 160.0);
    assert_eq!(split.fraction(), 0.84, "stopped at the right minimum");

    split.drag(-10_000.0, 1000.0, 160.0);
    assert_eq!(split.fraction(), 0.16, "stopped at the left minimum");
}

/// A split is written as its fraction — M12.8, decision 12.
#[test]
fn a_split_is_written_as_its_fraction() {
    assert_eq!(serde_json::to_string(&Split::new(0.25)).unwrap(), "0.25");
}

/// A fraction read from a file goes through `Split::new`: a hand-edited
/// `7.5` is the whole length, not seven and a half of it.
#[test]
fn a_split_read_from_a_file_is_held_between_nothing_and_everything() {
    let wide: Split = serde_json::from_str("7.5").unwrap();
    assert_eq!(wide.fraction(), 1.0);
    let negative: Split = serde_json::from_str("-2").unwrap();
    assert_eq!(negative.fraction(), 0.0);
    assert_eq!(
        Split::from(f32::NAN).fraction(),
        0.5,
        "not a number is a half"
    );
}

#[test]
fn a_split_by_default_is_halfway() {
    assert_eq!(Split::default().fraction(), 0.5);
}
