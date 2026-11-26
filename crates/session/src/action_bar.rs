//! Stable identities and compatible global ordering for titlebar controls.

/// A supported Action Bar control. Persist strings so newer IDs survive older builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// Provider usage.
    Usage,
    /// Open the current project in another application.
    OpenIn,
    /// Recover terminal jobs.
    TerminalJobs,
}

impl Item {
    /// Default left-to-right order; future controls append here.
    pub const ALL: [Self; 3] = [Self::Usage, Self::OpenIn, Self::TerminalJobs];

    /// Stable settings identifier, independent of translated labels.
    pub fn id(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::OpenIn => "open_in",
            Self::TerminalJobs => "terminal_jobs",
        }
    }
}

/// Render each supported control once, appending missing controls in catalog order.
pub fn effective(saved: &[String]) -> Vec<Item> {
    let mut order = Vec::new();
    for id in saved
        .iter()
        .map(String::as_str)
        .chain(Item::ALL.map(Item::id))
    {
        if let Some(item) = Item::ALL.into_iter().find(|item| item.id() == id)
            && !order.contains(&item)
        {
            order.push(item);
        }
    }
    order
}

/// Visible composition, shared by the real titlebar and its settings preview.
/// Missing catalog entries append unless explicitly excluded; an empty result is valid.
pub fn visible(saved: &[String], excluded: &[String]) -> Vec<Item> {
    effective(saved)
        .into_iter()
        .filter(|item| !excluded.iter().any(|id| id == item.id()))
        .collect()
}

fn write_order(saved: &mut Vec<String>, visible: Vec<Item>) {
    let rest = effective(saved)
        .into_iter()
        .filter(|item| !visible.contains(item))
        .collect::<Vec<_>>();
    let unknown = saved
        .iter()
        .filter(|id| !Item::ALL.iter().any(|item| item.id() == id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    *saved = visible
        .into_iter()
        .chain(rest)
        .map(|item| item.id().to_owned())
        .chain(unknown)
        .collect();
}

/// Move within the visible subset. Hidden, invalid and boundary moves are no-ops.
/// Unknown IDs and excluded controls survive ordinary edits and unrelated saves.
pub fn move_to(saved: &mut Vec<String>, excluded: &[String], item: Item, target: usize) -> bool {
    let mut order = visible(saved, excluded);
    let Some(source) = order.iter().position(|candidate| *candidate == item) else {
        return false;
    };
    if target >= order.len() || source == target {
        return false;
    }
    order.remove(source);
    order.insert(target, item);
    write_order(saved, order);
    true
}

/// Exclude a supported control without losing its configured order.
pub fn remove(excluded: &mut Vec<String>, item: Item) -> bool {
    if excluded.iter().any(|id| id == item.id()) {
        return false;
    }
    excluded.push(item.id().to_owned());
    true
}

/// Restore an excluded supported control at the visible end, retaining unknown IDs.
pub fn add(saved: &mut Vec<String>, excluded: &mut Vec<String>, item: Item) -> bool {
    if !excluded.iter().any(|id| id == item.id()) {
        return false;
    }
    let mut order = visible(saved, excluded);
    excluded.retain(|id| id != item.id());
    order.push(item);
    write_order(saved, order);
    true
}

/// Restore the default composition and order, including future catalog additions.
pub fn reset(saved: &mut Vec<String>, excluded: &mut Vec<String>) {
    saved.clear();
    excluded.clear();
}
