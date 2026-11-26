//! `CHANGELOG.md`, in the part of keepachangelog 1.1.0 we write — M5.16,
//! decision 45 of the spec.
//!
//! **Read by the window and written by `guild-release`, through this one
//! module**, so a changelog the release accepts is one the window can
//! show — by construction rather than by two readers agreeing. Here
//! rather than in the desktop because the desktop links gpui and the
//! release tool must not.
//!
//! The form is fixed because nobody else writes the file: a release is
//! `## [<version>] - <YYYY-MM-DD>` or `## [Unreleased]`, newest first; a
//! group is `### ` and one of keepachangelog's six names; an entry is a
//! line starting `- ` under a group. **Everything else is ignored, not
//! refused** — the window reads a changelog in a bundle it did not build,
//! and a line it does not understand must cost that line, not the card.
//! Inline markdown is kept as the characters it is.

/// One of keepachangelog's six kinds of change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// New features.
    Added,
    /// Changes in existing behaviour.
    Changed,
    /// Soon-to-be removed features.
    Deprecated,
    /// Removed features.
    Removed,
    /// Bug fixes.
    Fixed,
    /// Vulnerabilities.
    Security,
}

impl Group {
    /// keepachangelog's order, which is the order they are shown in
    /// whatever order a file has them.
    pub const ALL: [Group; 6] = [
        Group::Added,
        Group::Changed,
        Group::Deprecated,
        Group::Removed,
        Group::Fixed,
        Group::Security,
    ];

    /// The name as a `###` heading spells it.
    pub fn name(self) -> &'static str {
        match self {
            Group::Added => "Added",
            Group::Changed => "Changed",
            Group::Deprecated => "Deprecated",
            Group::Removed => "Removed",
            Group::Fixed => "Fixed",
            Group::Security => "Security",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.name() == name)
    }
}

/// The release nothing has been cut into yet.
pub const UNRELEASED: &str = "Unreleased";

/// How every release's name begins: `nightly-2026-09-24`.
pub const NIGHTLY: &str = "nightly-";

/// A `## ` section: `Unreleased`, or a version with its date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// `Unreleased`, or the version as the heading names it.
    pub version: String,
    /// `YYYY-MM-DD`; `None` for `Unreleased`.
    pub date: Option<String>,
    /// Its entries in the file's order, each with its group.
    pub entries: Vec<(Group, String)>,
}

impl Release {
    /// Release-note body shared by signed Remote metadata and completed Local builds.
    pub fn markdown_notes(&self) -> String {
        self.entries
            .iter()
            .map(|(group, line)| format!("### {}\n- {}", group.name(), line))
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Whether this is the section a release has not cut yet.
    pub fn is_unreleased(&self) -> bool {
        self.version == UNRELEASED
    }
}

/// What a `## ` line is.
enum Heading {
    Release(Release),
    /// A `## ` of no form we write: its entries are nobody's.
    Other,
}

fn heading(rest: &str) -> Heading {
    let Some(rest) = rest.strip_prefix('[') else {
        return Heading::Other;
    };
    let Some((version, after)) = rest.split_once(']') else {
        return Heading::Other;
    };
    let version = version.trim();
    let after = after.trim();
    if version.is_empty() {
        return Heading::Other;
    }
    if version == UNRELEASED && after.is_empty() {
        return Heading::Release(Release {
            version: version.to_owned(),
            date: None,
            entries: Vec::new(),
        });
    }
    match after.strip_prefix("- ").map(str::trim) {
        Some(date) if version != UNRELEASED && is_date(date) => Heading::Release(Release {
            version: version.to_owned(),
            date: Some(date.to_owned()),
            entries: Vec::new(),
        }),
        _ => Heading::Other,
    }
}

/// `YYYY-MM-DD`, by its shape.
fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// The releases of a changelog, in the file's order — newest first, as
/// keepachangelog writes them.
pub fn parse(text: &str) -> Vec<Release> {
    let mut releases: Vec<Release> = Vec::new();
    // Whether the lines being read belong to a release we keep, and to
    // which group of it.
    let mut open = false;
    let mut group: Option<Group> = None;
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("## ") {
            group = None;
            open = match heading(rest.trim()) {
                Heading::Release(release) => {
                    releases.push(release);
                    true
                }
                Heading::Other => false,
            };
        } else if let Some(rest) = line.strip_prefix("### ") {
            group = Group::named(rest.trim());
        } else if let Some(entry) = line.strip_prefix("- ") {
            let entry = entry.trim();
            if let (true, Some(group), Some(release)) = (open, group, releases.last_mut())
                && !entry.is_empty()
            {
                release.entries.push((group, entry.to_owned()));
            }
        } else if line.starts_with("####") {
            // Deeper than a group: what is under it is not the group's.
            group = None;
        }
    }
    releases
}

/// The releases from `upto` down to `after`, exclusive — what a build of
/// `after` has not seen of `upto`. Decision 9 of M5.16's spec.
///
/// `after` unknown — `None`, a version the file does not have, or one
/// **above** `upto`, which is a rollback — gives `upto`'s section alone:
/// what the release itself says, never the whole history. `upto` absent
/// gives nothing, and `Unreleased` is never a release to show.
pub fn between<'a>(releases: &'a [Release], after: Option<&str>, upto: &str) -> Vec<&'a Release> {
    let Some(top) = releases
        .iter()
        .position(|release| release.version == upto && !release.is_unreleased())
    else {
        return Vec::new();
    };
    let bottom = after
        .and_then(|after| releases.iter().position(|release| release.version == after))
        .filter(|bottom| *bottom > top)
        .unwrap_or(top + 1);
    releases[top..bottom].iter().collect()
}

/// What changed, one list per group in keepachangelog's order.
pub type Notes = Vec<(Group, Vec<String>)>;

/// The entries of `releases`, merged by group: the groups in
/// keepachangelog's order, and within one the releases in the order
/// given — newest first, as [`between`] gives them. Empty groups are
/// left out.
pub fn merged(releases: &[&Release]) -> Notes {
    Group::ALL
        .into_iter()
        .filter_map(|group| {
            let lines: Vec<String> = releases
                .iter()
                .flat_map(|release| &release.entries)
                .filter(|(of, _)| *of == group)
                .map(|(_, line)| line.clone())
                .collect();
            (!lines.is_empty()).then_some((group, lines))
        })
        .collect()
}

/// The first `at` entries of `notes` in the order they are shown, and
/// how many are left out — the hover card's *and N more*.
pub fn cut(notes: &Notes, at: usize) -> (Notes, usize) {
    let mut left = at;
    let mut more = 0;
    let mut kept = Notes::new();
    for (group, lines) in notes {
        let take = lines.len().min(left);
        left -= take;
        more += lines.len() - take;
        if take > 0 {
            kept.push((*group, lines[..take].to_vec()));
        }
    }
    (kept, more)
}

/// A name both forms of which are tags already — which only a tag made
/// by hand can explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken(pub String);

/// The name of a release made on `date` (`YYYY-MM-DD`) from the commit
/// `short` names, among the `tags` there are — the person's decision 3:
/// `nightly-<date>`, and `nightly-<date>-<short>` when the day has one.
/// The commit is the one released, not the release commit, which cannot
/// carry its own hash in the file it commits.
pub fn version_name(date: &str, short: &str, tags: &[&str]) -> Result<String, Taken> {
    let first = format!("{NIGHTLY}{date}");
    if !tags.contains(&first.as_str()) {
        return Ok(first);
    }
    let second = format!("{first}-{short}");
    if tags.contains(&second.as_str()) {
        return Err(Taken(second));
    }
    Ok(second)
}

/// Why a changelog cannot be released.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No `## [Unreleased]` to cut.
    NoUnreleased,
    /// `Unreleased` holds no entry: nothing a person would notice.
    EmptyUnreleased,
    /// The version is a heading of the file already.
    Exists(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoUnreleased => write!(f, "the changelog has no ## [Unreleased]"),
            Refusal::EmptyUnreleased => write!(
                f,
                "## [Unreleased] has no entry — nothing a person would notice to release"
            ),
            Refusal::Exists(version) => write!(f, "the changelog has {version} already"),
        }
    }
}

/// Whether the changelog has something to release.
pub fn check(text: &str) -> Result<(), Refusal> {
    let releases = parse(text);
    let unreleased = releases
        .iter()
        .find(|release| release.is_unreleased())
        .ok_or(Refusal::NoUnreleased)?;
    if unreleased.entries.is_empty() {
        return Err(Refusal::EmptyUnreleased);
    }
    Ok(())
}

/// The changelog with `Unreleased` cut into `version` of `date`: its
/// heading becomes `## [<version>] - <date>`, a fresh empty
/// `## [Unreleased]` goes above it, and nothing else moves.
pub fn release(text: &str, version: &str, date: &str) -> Result<String, Refusal> {
    check(text)?;
    if parse(text).iter().any(|release| release.version == version) {
        return Err(Refusal::Exists(version.to_owned()));
    }
    let mut out = String::with_capacity(text.len() + 64);
    let mut cut = false;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if !cut
            && let Some(rest) = bare.strip_prefix("## ")
            && matches!(heading(rest.trim()), Heading::Release(ref r) if r.is_unreleased())
        {
            let end = &line[bare.len()..];
            let end = if end.is_empty() { "\n" } else { end };
            out.push_str(bare);
            out.push_str(end);
            out.push_str(end);
            out.push_str(&format!("## [{version}] - {date}{end}"));
            cut = true;
            continue;
        }
        out.push_str(line);
    }
    Ok(out)
}
