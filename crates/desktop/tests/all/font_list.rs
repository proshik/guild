//! The list a font is chosen from — M5.9, and the thing that replaced a
//! button one clicked forty times.

use guild_desktop::fonts::{self, From, Which};

fn installed() -> Vec<String> {
    [
        "Arial",
        "Menlo",
        "MesloLGS Nerd Font Mono",
        "Inter",
        "Zapfino",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect()
}

fn families(rows: &[fonts::Row]) -> Vec<String> {
    rows.iter()
        .map(|row| row.family.clone().unwrap_or_else(|| "—".to_owned()))
        .collect()
}

/// The way back to no choice comes first, then what ships, then what the
/// machine has.
#[test]
fn the_list_is_the_system_then_ours_then_the_machines() {
    let rows = fonts::rows(Which::Ui, "", &installed());

    assert_eq!(rows[0].from, From::System);
    assert_eq!(rows[0].family, None);
    assert_eq!(rows[1].family.as_deref(), Some("Inter"));
    assert_eq!(rows[1].from, From::Bundled);
    assert_eq!(rows[2].family.as_deref(), Some("JetBrains Mono"));
    assert!(
        families(&rows).contains(&"Zapfino".to_owned()),
        "the interface offers every family: a face for reading is not a \
         face for a grid, and nothing can tell them apart by name"
    );
}

/// The code setting offers what looks like it can draw a grid — a guess,
/// and one that is made in one place.
#[test]
fn the_code_list_leaves_out_what_cannot_draw_a_grid() {
    let rows = fonts::rows(Which::Code, "", &installed());
    let names = families(&rows);

    assert!(names.contains(&"MesloLGS Nerd Font Mono".to_owned()));
    assert!(names.contains(&"Menlo".to_owned()));
    assert!(!names.contains(&"Zapfino".to_owned()));
    assert!(
        names.contains(&"JetBrains Mono".to_owned()),
        "and what ships is always offered"
    );
}

/// A machine that has Inter installed must not be offered it twice: two
/// rows that do the same thing are a list that looks broken.
#[test]
fn a_bundled_face_is_offered_once() {
    let rows = fonts::rows(Which::Ui, "inter", &installed());

    assert_eq!(families(&rows), vec!["Inter".to_owned()]);
}

/// Typing narrows it, and the way back to no choice is not a row that
/// matches a search — a person typing a name is looking for a face.
#[test]
fn typing_narrows_the_list() {
    let rows = fonts::rows(Which::Ui, "mono", &installed());

    assert_eq!(
        families(&rows),
        vec![
            "JetBrains Mono".to_owned(),
            "MesloLGS Nerd Font Mono".to_owned()
        ]
    );
    assert!(rows.iter().all(|row| row.from != From::System));
}

/// A face is found by its scattered letters, as every popup's list is
/// since M8.9: `jbm` is JetBrains Mono.
#[test]
fn a_face_is_found_by_its_scattered_letters() {
    let rows = fonts::rows(Which::Code, "jbm", &installed());

    assert_eq!(families(&rows), vec!["JetBrains Mono".to_owned()]);

    let rows = fonts::rows(Which::Code, "mlgs", &installed());
    assert_eq!(families(&rows), vec!["MesloLGS Nerd Font Mono".to_owned()]);
}

/// A machine with nothing installed still offers what ships — which is
/// the point of shipping it.
///
/// **Code and the terminal have no *from the system* row** since M12.11:
/// no choice is Lilex there, not the system's face, and the first button
/// beside *Custom* is the way back to it. **And the terminal is not offered
/// Lilex Duo**, which has no Nerd glyphs and a cell and a half for a wide
/// letter — decision 1.
#[test]
fn what_ships_is_offered_on_a_bare_machine() {
    assert_eq!(
        families(&fonts::rows(Which::Ui, "", &[])),
        vec![
            "—",
            "Inter",
            "JetBrains Mono",
            "Lilex",
            "Lilex Duo",
            "Lilex Nerd Font"
        ]
    );
    assert_eq!(
        families(&fonts::rows(Which::Code, "", &[])),
        vec![
            "Inter",
            "JetBrains Mono",
            "Lilex",
            "Lilex Duo",
            "Lilex Nerd Font"
        ]
    );
    assert_eq!(
        families(&fonts::rows(Which::Terminal, "", &[])),
        vec!["Inter", "JetBrains Mono", "Lilex", "Lilex Nerd Font"]
    );
}
