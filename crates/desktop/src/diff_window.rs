//! A commit's file as a diff in its own window, stepping through the
//! commit — M8.3.6, decision 7. Pure: where a step lands.

/// The file a step from `index` lands on among `total`, or `None` at an
/// end: the step does not wrap, and the arrows grey there.
pub fn stepped(index: usize, total: usize, forward: bool) -> Option<usize> {
    if forward {
        (index + 1 < total).then_some(index + 1)
    } else {
        index.checked_sub(1).filter(|_| total > 0)
    }
}
