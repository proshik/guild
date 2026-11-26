//! The status bar's tool buttons, read off section 5 of the design
//! system: the one row that names five numbers at once.

use guild_desktop::scale::BASE;
use guild_desktop::theme::{radius, space};

fn design_system() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design-system.md");
    std::fs::read_to_string(path).expect("the design system")
}

/// `| Status bar tool button | 22 (icon 14, radius 6, gap 4, groups 12 apart) |`
/// — every number in it is a token, and the tokens are the numbers.
#[test]
fn the_status_bar_s_buttons_are_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Status bar tool button |"))
        .expect("the buttons' row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [24.0, 16.0, 6.0, 2.0, 12.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::STATUS_BUTTON), 24.0);
    assert_eq!(px(space::STATUS_ICON), 16.0);
    assert_eq!(px(radius::STATUS_BUTTON), 6.0);
    assert_eq!(px(space::STATUS_BUTTON_GAP), 2.0);
    assert_eq!(px(space::STATUS_GROUP_GAP), 12.0);
}

/// Section 6.16's dock head: a tab 24 high at radius 6 with the tool's
/// icon at 14, and a close button 22 square at the right. The numbers
/// are read out of the document, so the document cannot drift from the
/// tokens without this failing.
#[test]
fn a_dock_head_s_tabs_are_the_document_s() {
    let document = design_system();
    let section = document
        .split("### 6.16 Dock head with more than one tool")
        .nth(1)
        .expect("section 6.16");
    let px = |rems: gpui::Rems| rems.0 * BASE;

    assert!(
        section.contains("each 24 high at radius 6"),
        "the document says what a tab is"
    );
    assert_eq!(px(space::HEAD_TAB), 24.0);
    assert_eq!(px(radius::STATUS_BUTTON), 6.0);
    assert_eq!(px(space::STATUS_ICON), 16.0);

    // **The pair at the head's right, not a close button.** M8.3.5
    // replaced the cross with a ⋮ and a bar — a dock is hidden rather
    // than closed — and the document went on carrying the old sentence
    // for a paragraph, which this assertion was pinning. It reads the
    // sentence that is true now, and the same token stands behind it.
    assert!(
        section.contains("each 24 square at radius 6"),
        "and what the ⋮ and the bar at the head's right are"
    );
    assert!(
        section.contains("**A bar, not a cross:**"),
        "and why it is not a cross"
    );
    assert_eq!(px(space::STATUS_BUTTON), 24.0);
}

/// **A menu, in numbers** — M12.2 task 8: every number of section
/// 5's row is a token, and the tokens are the numbers.
#[test]
fn a_menu_is_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Menu |"))
        .expect("the menu's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(
        numbers,
        [32.0, 180.0, 5.0, 14.0, 16.0, 6.0, 6.0, 8.0, 8.0, 13.0],
        "{row}"
    );
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::MENU_ROW), 32.0);
    assert_eq!(px(space::MENU_MIN), 180.0);
    assert_eq!(px(space::MENU_PADDING), 5.0);
    assert_eq!(px(space::MENU_INSET), 14.0);
    assert_eq!(px(space::MENU_ICON), 16.0);
    assert_eq!(px(space::MENU_ICON_GAP), 6.0);
    assert_eq!(px(space::MENU_RULE), 6.0);
    assert_eq!(px(radius::ROW), 8.0);
    assert_eq!(px(guild_desktop::theme::size::BODY), 13.0);
}

/// **The pickers take the menu's card and two row heights** — M12.7 task
/// 9: a one-line row is `PICKER_ROW`, and the branch popup keeps its own
/// `BRANCH_ROW`. Section 5's row is read rather than trusted.
#[test]
fn a_picker_row_is_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Picker row |"))
        .expect("the picker row's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [28.0, 44.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::PICKER_ROW), 28.0);
    assert_eq!(px(space::BRANCH_ROW), 44.0);
}

/// **M8.9's two popups and their preview, in numbers** — handed out by the
/// seams commit so that neither group edits `theme` or section 5: Search
/// Everywhere 680 wide with a list 440 high, Find in Files 760 wide with a
/// list 280 high, and a preview 220 high under either. Every number of the
/// three rows is a token, and the tokens are the numbers.
#[test]
fn search_everywhere_and_find_in_files_are_the_document_s() {
    let document = design_system();
    let numbers = |name: &str| -> Vec<f32> {
        let row = document
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{name} in section 5"));
        row.split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect()
    };
    assert_eq!(numbers("| Search Everywhere popup |"), [680.0, 440.0]);
    assert_eq!(numbers("| Find in Files popup |"), [760.0, 280.0]);
    assert_eq!(numbers("| A popup's preview |"), [220.0]);

    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::EVERYWHERE), 680.0);
    assert_eq!(px(space::EVERYWHERE_LIST), 440.0);
    assert_eq!(px(space::FIND_IN_FILES), 760.0);
    assert_eq!(px(space::FIND_IN_FILES_LIST), 280.0);
    assert_eq!(px(space::PREVIEW), 220.0);
}

/// `| Update branch sources |` — the popup under the title bar's ↓: its
/// width, its rows (a menu's) and how tall its list grows before it
/// scrolls. Every number is a token, and the tokens are the numbers.
#[test]
fn the_update_branch_sources_are_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Update branch sources |"))
        .expect("the source popup's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [300.0, 32.0, 224.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::UPDATE_SOURCES), 300.0);
    assert_eq!(px(space::MENU_ROW), 32.0);
    assert_eq!(px(space::UPDATE_SOURCES_LIST), 224.0);
}

/// `| Language server state |` — M8.5.1: how wide a server's detail may
/// run, the bar beside it, and the list a click opens.
#[test]
fn the_server_state_is_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Language server state |"))
        .expect("the server state's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [240.0, 48.0, 3.0, 360.0], "{row}");

    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::SERVER_DETAIL), 240.0);
    assert_eq!(px(space::PROGRESS_WIDTH), 48.0);
    assert_eq!(px(space::PROGRESS_HEIGHT), 3.0);
    assert_eq!(px(space::SERVER_LIST), 360.0);
}

/// The tree's measures, 2026-09-18 — M8.10. Both numbers are
/// the document's: `| Indent step in the tree | 18 |` in section 5, and
/// section 6.4's own `Height 22`.
#[test]
fn the_tree_s_row_and_its_indent_are_the_document_s() {
    let document = design_system();
    let cell = |name: &str| -> f32 {
        let row = document
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{name} in section 5"));
        row.split('|')
            .nth(2)
            .and_then(|cell| cell.trim().parse().ok())
            .unwrap_or_else(|| panic!("{name} has no number"))
    };
    let section = document
        .split("### 6.4 File tree row")
        .nth(1)
        .expect("section 6.4");
    let height: f32 = section
        .split_once("Height ")
        .and_then(|(_, rest)| rest.split([' ', '(']).next().and_then(|n| n.parse().ok()))
        .expect("section 6.4 opens with the row's height");

    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::TREE_INDENT), cell("| Indent step in the tree |"));
    assert_eq!(px(space::TREE_ROW), height);
}

/// Section 6.7's commit button: a minimum size of 72×28 and an arc of 8,
/// with 10 either side of the label. The numbers are the document's.
#[test]
fn the_commit_button_is_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Commit button |"))
        .expect("the commit button's row in section 6.7");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [28.0, 72.0, 14.0, 10.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::BUTTON), 28.0);
    assert_eq!(px(space::BUTTON_MIN), 72.0);
    assert_eq!(px(radius::CARD), 14.0);
    assert_eq!(px(space::BUTTON_SIDE), 10.0);
}

/// **The terminal is dense** — 2026-09-24: a 12-point face on 16-point
/// lines fits more lines than the editor's 1.70, which Guild's terminal
/// took before. The editor keeps its own leading.
#[test]
fn the_terminal_leads_densely() {
    use guild_desktop::theme::leading;
    assert_eq!(leading::TERMINAL, 16.0 / 12.0);
}

/// The numbers of one row of section 5, in the order written.
fn row_numbers(name: &str) -> Vec<f32> {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with(&format!("| {name} |")))
        .unwrap_or_else(|| panic!("the row `{name}` in section 5"));
    row.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect()
}

/// The shared titlebar Update height and padding, section 6.9.
#[test]
fn the_update_control_is_the_document_s() {
    assert_eq!(row_numbers("Update control"), [24.0, 7.0]);
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::STATUS_BUTTON), 24.0);
    assert_eq!(px(space::PILL_PADDING), 7.0);
}

/// What an update brings: the hover card's width and its cut, and the
/// *What's new* card's width and its list's height.
#[test]
fn the_release_notes_cards_are_the_document_s() {
    assert_eq!(row_numbers("Release notes"), [360.0, 8.0, 460.0, 440.0]);
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::NOTES_CARD), 360.0);
    assert_eq!(guild_desktop::update::HOVER_CUT, 8);
    assert_eq!(px(space::WHATS_NEW), 460.0);
    assert_eq!(px(space::WHATS_NEW_LIST), 440.0);
}

/// The title bar's tools, M12.10: the usage chip at the branch button's
/// height with 6 either side and a disc of 7, its card 360 wide with a
/// fully rounded bar 5 high and one-line window rows,
/// and Open In a button 22 square.
#[test]
fn the_title_bar_s_tools_are_the_document_s() {
    assert_eq!(
        row_numbers("Title bar tools"),
        [18.0, 6.0, 7.0, 360.0, 5.0, 48.0, 24.0]
    );
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::TITLE_CHIP), 18.0);
    assert_eq!(px(space::CHIP_PADDING), 6.0);
    assert_eq!(px(space::DISC), 7.0);
    assert_eq!(px(space::USAGE_CARD), 360.0);
    assert_eq!(px(space::USAGE_BAR_HEIGHT), 5.0);
    assert_eq!(px(space::STATUS_BUTTON) * 2.0, 48.0);
    assert_eq!(px(space::STATUS_BUTTON), 24.0);
}

/// The title bar's contents stand a point below its centre line —
/// 2 of padding above them, the traffic lights' frame at 11 — the
/// person's optical centring of 2026-09-25: geometric centring read as
/// high between the window's light top edge and the card below.
#[test]
fn the_title_bar_s_contents_stand_a_point_low() {
    assert_eq!(row_numbers("Title bar offset"), [34.0, 2.0, 11.0, 1.0]);
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::TITLEBAR), 34.0);
    assert_eq!(px(space::TITLEBAR_OFFSET), 2.0);
    assert_eq!(space::TRAFFIC_LIGHTS_TOP, 11.0);
}

/// `| Start screen | a column 560 wide; the default card 60 high with a disc of 32; a tile 84 high, 8 apart; a row 32 |`
/// — section 6.35, M14.5: the six numbers the screen is drawn at, each a
/// token, so the document cannot drift from the drawing without this
/// failing.
#[test]
fn the_start_screen_s_measures_are_the_document_s() {
    assert_eq!(
        row_numbers("Start screen"),
        [560.0, 60.0, 32.0, 84.0, 8.0, 32.0]
    );
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::START_COLUMN), 560.0);
    assert_eq!(px(space::START_CARD), 60.0);
    assert_eq!(px(space::START_DISC), 32.0);
    assert_eq!(px(space::START_TILE), 84.0);
    assert_eq!(px(space::CARD_GAP), 8.0);
    assert_eq!(px(space::MENU_ROW), 32.0);
}

/// Decision 17 of M12.8: an area and a pane are at least 320 across and
/// 160 down — the two floors a split is refused under.
#[test]
fn an_area_s_floors_are_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Area and pane |"))
        .expect("the area's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [320.0, 160.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::AREA_MIN_WIDTH), 320.0);
    assert_eq!(px(space::AREA_MIN_HEIGHT), 160.0);
}

/// Decision 19 of M12.8: the focused pane's outline, P3 in the mockups.
#[test]
fn the_pane_outline_is_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Pane outline |"))
        .expect("the pane's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [6.0, 2.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(radius::PANE), 6.0);
    assert_eq!(px(space::PANE_INSET), 2.0);
}

/// Decision 20: the compact start screen's two thresholds.
#[test]
fn the_compact_start_screen_s_thresholds_are_the_document_s() {
    let document = design_system();
    let row = document
        .lines()
        .find(|line| line.starts_with("| Start screen, compact |"))
        .expect("the compact start screen's row in section 5");
    let numbers: Vec<f32> = row
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers, [440.0, 520.0], "{row}");
    let px = |rems: gpui::Rems| rems.0 * BASE;
    assert_eq!(px(space::START_RECENT_FROM), 440.0);
    assert_eq!(px(space::START_FOOT_FROM), 520.0);
}
