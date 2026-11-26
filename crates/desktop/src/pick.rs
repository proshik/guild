//! Moving a selection through a list with the arrows.
//!
//! Pure, and shared on purpose. The Commit panel and the Log both walk a
//! list of rows with `up` and `down`, and a window where one of them
//! stops at the end while the other wraps is a window with a bug in it —
//! the person does not know which list they are in until it surprises
//! them. The arithmetic is small enough to write twice and exactly the
//! kind that is written twice differently.

/// Where the selection goes when an arrow is pressed.
///
/// `None` for an empty list: there is nothing to select and nothing to
/// scroll to. From nothing, either arrow lands on the first row — a list
/// is read from the top and that is where the eye already is, which is
/// what the Commit panel has done since M8.3. Otherwise one step,
/// clamped at both ends: a list that wraps round takes a person
/// somewhere they did not ask to go, and in a log of ten thousand
/// commits that is a long way back.
pub fn step(len: usize, current: Option<usize>, down: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    // An index past the end is a selection the list has outgrown — a
    // page that came back shorter. It is read as the last row rather
    // than followed off the end: whatever this answers is used to index
    // the list, so it may never point outside it.
    let current = current.map(|index| index.min(last));
    Some(match current {
        None => 0,
        Some(index) if down => (index + 1).min(last),
        Some(index) => index.saturating_sub(1),
    })
}

/// The row a list should be lit on, after whatever just changed it.
///
/// **A list with rows always has one lit**, because `enter` takes the
/// lit row and a gesture that answers nothing is worse than one that
/// answers wrongly — a person cannot tell it apart from a broken key.
/// The first row when nothing was chosen, the chosen one while it still
/// exists, and the last row when the list has shrunk under it.
pub fn settle(len: usize, selected: Option<usize>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(selected.unwrap_or(0).min(len - 1))
}
