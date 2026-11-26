//! How the Log lays itself out. A Git tool window has three columns
//! — the branches, the commits, the commit opened — and they need
//! width: the bottom dock has it, a side dock does not. Decision 6 of
//! M8.3.1; the number is the design system's, section 6.13.

/// The shape the Log draws in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLayout {
    /// Three columns side by side.
    Wide,
    /// The list, the opened commit under it — M8.3's shape.
    Narrow,
}

/// The layout for a panel of this width, in base-scale pixels.
pub fn layout(width: f32) -> LogLayout {
    if width >= crate::theme::space::LOG_WIDE.0 * crate::scale::BASE {
        LogLayout::Wide
    } else {
        LogLayout::Narrow
    }
}

/// The narrowest the commits may be squeezed to.
///
/// The two outer columns exist to explain the list in the middle; when
/// the width runs out it is they that give way, not the list. Section
/// 6.13.
pub const COMMITS_MIN: f32 = 240.0;

/// How wide the Log's outer columns are in a panel of this width.
///
/// Answers the branches' width and the details', in that order. What a
/// person dragged them to, unless the panel is too narrow to hold both
/// and the list. Then the **branches give way first**: it is a column of
/// short names, while the details hold a commit's body and its files and
/// stop being readable at all when squeezed. Only meaningful for
/// [`LogLayout::Wide`]; the narrow shape has one column.
pub fn columns(width: f32, wanted: guild_session::Columns) -> (f32, f32) {
    let (branches_min, _) = guild_session::Columns::BRANCHES_BOUNDS;
    let (details_min, _) = guild_session::Columns::DETAILS_BOUNDS;

    let details = wanted
        .details
        .min(width - COMMITS_MIN - branches_min)
        .max(details_min);
    let branches = wanted
        .branches
        .min(width - COMMITS_MIN - details)
        .max(branches_min);
    (branches, details)
}
