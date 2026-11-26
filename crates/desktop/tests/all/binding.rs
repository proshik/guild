//! Which session a tab holds when its program continues another's
//! conversation — M13.3, decision 52.

use std::collections::HashMap;

use guild_desktop::binding::{self, Bind, Found, Record, Told};
use guild_session::{HarnessKind, SessionId};

fn id(n: u8) -> SessionId {
    format!("{n:08x}-0000-0000-0000-000000000000")
        .parse()
        .unwrap()
}

fn record(n: u8, harness: Option<HarnessKind>, theirs: Option<&str>) -> Record<'_> {
    Record {
        id: id(n),
        harness,
        harness_session: theirs,
    }
}

#[test]
fn a_conversation_is_the_record_its_harness_names_it_by() {
    let records = [
        record(1, Some(HarnessKind::Claude), None),
        record(2, Some(HarnessKind::Codex), Some("cx-9")),
        record(3, None, None),
    ];
    let of = |kind, conversation: &str| binding::record_of(kind, conversation, &records);
    assert_eq!(of(HarnessKind::Claude, &id(1).to_string()), Some(id(1)));
    assert_eq!(of(HarnessKind::Codex, "cx-9"), Some(id(2)));
    // By its own harness only: a Codex record's id is not a Claude
    // conversation, and a shell's id is no conversation at all.
    assert_eq!(of(HarnessKind::Claude, &id(2).to_string()), None);
    assert_eq!(of(HarnessKind::Claude, &id(3).to_string()), None);
    assert_eq!(of(HarnessKind::OpenCode, "cx-9"), None);
    assert_eq!(of(HarnessKind::Claude, "not-an-id"), None);
}

#[test]
fn a_tab_is_bound_only_to_another_record_nobody_else_holds_in_its_own_seat() {
    let found = |record, same_seat, running_elsewhere| {
        Some(Found {
            record,
            same_seat,
            running_elsewhere,
        })
    };
    assert_eq!(
        binding::binding(id(1), found(id(2), true, false)),
        Bind::To(id(2))
    );
    assert_eq!(
        binding::binding(id(1), found(id(2), true, true)),
        Bind::Elsewhere(id(2))
    );
    assert_eq!(
        binding::binding(id(1), found(id(2), false, false)),
        Bind::OtherSeat(id(2))
    );
    // Held elsewhere is said before another seat: it is the worse news.
    assert_eq!(
        binding::binding(id(1), found(id(2), false, true)),
        Bind::Elsewhere(id(2))
    );
    // No record: a new conversation, or `/clear`'s — nothing changes.
    assert_eq!(binding::binding(id(1), None), Bind::Nothing);
    // Its own conversation is no news.
    assert_eq!(
        binding::binding(id(1), found(id(1), true, false)),
        Bind::Nothing
    );
}

/// **A task resumed in a person's tab is still a task** to decision 35:
/// the process keeps the `GUILD_SESSION` it was started with, and the
/// window reads it through what the tab holds now.
#[test]
fn a_process_keeps_meaning_the_tab_it_was_started_in() {
    let mut started_as = HashMap::new();
    assert_eq!(binding::resolve(id(1), &started_as), id(1));
    started_as.insert(id(1), id(2));
    assert_eq!(binding::resolve(id(1), &started_as), id(2));
    assert_eq!(binding::resolve(id(9), &started_as), id(9));
}

/// A hook from a process Guild did not start carries no id, or not one of
/// ours — and names no tab.
#[test]
fn only_a_session_id_is_a_launch() {
    assert_eq!(binding::launched_as(Some(&id(4).to_string())), Some(id(4)));
    assert_eq!(binding::launched_as(Some("")), None);
    assert_eq!(binding::launched_as(Some("x")), None);
    assert_eq!(binding::launched_as(None), None);
}

/// The terminal probe finds the same conversation every two seconds; the
/// person hears about it once.
#[test]
fn a_tab_is_told_once_per_conversation() {
    let mut told = Told::default();
    assert!(told.first(7u32, id(2)));
    assert!(!told.first(7u32, id(2)), "the probe runs every two seconds");
    assert!(told.first(7u32, id(3)));
    assert!(told.first(8u32, id(2)));
    told.forget(&7u32);
    assert!(
        told.first(7u32, id(2)),
        "a tab closed and made again is a new tab"
    );
}
