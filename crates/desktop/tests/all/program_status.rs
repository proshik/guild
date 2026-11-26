//! OSC 7501, the Program Status Protocol — the probe a program asks with.
//!
//! Claude Code takes a DA1 reply that arrives before the 7501 reply as "no"
//! for the whole session (issue 105534), so the reader answers the probe
//! itself, before the chunk is parsed. Here: that it finds the probe at all.

use guild_desktop::terminal::program_status::{ProbeScanner, Terminator, probe_answer};

#[test]
fn the_probe_is_found_with_either_terminator() {
    let mut scanner = ProbeScanner::default();
    assert_eq!(
        scanner.feed(b"text\x1b]7501;?\x07more"),
        vec![Terminator::Bel]
    );
    assert_eq!(scanner.feed(b"\x1b]7501;?\x1b\\"), vec![Terminator::St]);
    assert_eq!(
        scanner.feed(b"\x1b]7501;?\x07\x1b[c\x1b]7501;?\x1b\\"),
        vec![Terminator::Bel, Terminator::St],
        "two in one read, in order"
    );
}

#[test]
fn the_probe_is_found_split_at_every_byte() {
    for probe in [&b"\x1b]7501;?\x07"[..], &b"\x1b]7501;?\x1b\\"[..]] {
        let bytes = [b"ab".as_slice(), probe, b"\x1b[c"].concat();
        for cut in 0..=bytes.len() {
            let mut scanner = ProbeScanner::default();
            let mut found = scanner.feed(&bytes[..cut]);
            found.extend(scanner.feed(&bytes[cut..]));
            assert_eq!(found.len(), 1, "cut at {cut}: {found:?}");
        }
        let mut scanner = ProbeScanner::default();
        let found: Vec<Terminator> = bytes.iter().flat_map(|b| scanner.feed(&[*b])).collect();
        assert_eq!(found.len(), 1, "byte by byte");
    }
}

#[test]
fn look_alikes_are_not_the_probe() {
    for bytes in [
        &b"\x1b]7501;state=working\x07"[..],
        b"\x1b]75010;?\x07",
        b"\x1b]7501;?x\x07",
        b"\x1b]7501?\x07",
        b"\x1b]7501;?\x1bx",
        b"\x1b[7501;?\x07",
        b"]7501;?\x07",
    ] {
        assert!(
            ProbeScanner::default().feed(bytes).is_empty(),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn a_probe_right_after_an_abandoned_one_is_found() {
    assert_eq!(
        ProbeScanner::default().feed(b"\x1b]7501;\x1b]7501;?\x07"),
        vec![Terminator::Bel]
    );
    assert_eq!(
        ProbeScanner::default().feed(b"\x1b]7501;?\x1b\x1b]7501;?\x1b\\"),
        vec![Terminator::St],
        "an ESC that did not end the first begins the second"
    );
}

#[test]
fn the_reply_names_the_states_and_kinds_and_ends_like_the_probe() {
    assert_eq!(
        probe_answer(Terminator::Bel),
        "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x07"
    );
    assert_eq!(
        probe_answer(Terminator::St),
        "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"
    );
}

// ── Reports ─────────────────────────────────────────────────────────────
//
// `parse_report` reads what follows `OSC 7501 ;` up to the terminator. The
// rules are rev 0.4's: a missing or unknown state, an invalid id, an
// over-limit field, bad base64, non-UTF-8 text or a control character
// discard the report whole; an unknown key is ignored, a malformed pair is
// skipped and a repeated key's last value wins.

use guild_desktop::terminal::program_status::{Kind, Report, SEQUENCE_LIMIT, State, parse_report};

fn report(text: &str) -> Option<Report> {
    parse_report(text.as_bytes())
}

#[test]
fn every_state_is_read() {
    for (word, state) in [
        ("idle", State::Idle),
        ("working", State::Working),
        ("done", State::Done),
        ("blocked", State::Blocked),
        ("error", State::Error),
        ("clear", State::Clear),
    ] {
        let read = report(&format!("state={word}")).expect(word);
        assert_eq!(read.state, state);
        assert_eq!(read.id, None);
        assert_eq!(read.kind, None);
    }
}

#[test]
fn a_missing_or_unknown_state_discards_the_report() {
    assert_eq!(report("state=busy"), None);
    assert_eq!(report("state="), None);
    assert_eq!(report("app=codex"), None);
    assert_eq!(report(""), None);
    assert_eq!(report("?"), None, "the probe is not a report");
    assert_eq!(report("STATE=idle"), None, "keys are case-sensitive");
}

#[test]
fn kind_is_read_with_blocked_only() {
    for (word, kind) in [
        ("permission", Kind::Permission),
        ("question", Kind::Question),
        ("auth", Kind::Auth),
    ] {
        assert_eq!(
            report(&format!("state=blocked:kind={word}")).unwrap().kind,
            Some(kind)
        );
    }
    assert_eq!(report("state=blocked").unwrap().kind, None);
    assert_eq!(report("state=working:kind=question").unwrap().kind, None);
    let unknown =
        report("state=blocked:kind=coffee").expect("an unknown kind is not a report error");
    assert_eq!(unknown.kind, None);
}

#[test]
fn an_id_is_segments_of_a_narrow_alphabet() {
    assert_eq!(
        report("state=working:id=agent1").unwrap().id.as_deref(),
        Some("agent1")
    );
    assert_eq!(
        report("state=done:id=a/b.c/d_e+f-g").unwrap().id.as_deref(),
        Some("a/b.c/d_e+f-g")
    );
    let deep = ["s"; 8].join("/");
    assert!(
        report(&format!("state=idle:id={deep}")).is_some(),
        "eight deep"
    );
    let too_deep = ["s"; 9].join("/");
    assert_eq!(
        report(&format!("state=idle:id={too_deep}")),
        None,
        "nine deep"
    );
    let segment = "x".repeat(32);
    assert!(report(&format!("state=idle:id={segment}")).is_some());
    assert_eq!(
        report(&format!("state=idle:id={segment}x")),
        None,
        "33 in a segment"
    );
    let long = [segment.as_str(); 4].join("/") + "/x";
    assert!(long.len() > 128);
    assert_eq!(report(&format!("state=idle:id={long}")), None, "over 128");
    for bad in ["", "a//b", "/a", "a/", "a,b", "a=b"] {
        assert_eq!(report(&format!("state=idle:id={bad}")), None, "{bad:?}");
    }
}

#[test]
fn progress_and_app_are_bounded() {
    assert_eq!(
        report("state=working:progress=0").unwrap().progress,
        Some(0)
    );
    assert_eq!(
        report("state=working:progress=100").unwrap().progress,
        Some(100)
    );
    assert_eq!(report("state=working:progress=101"), None);
    // Review r2: a number too large for any integer is over the limit too,
    // not a malformed pair to skip.
    assert_eq!(report("state=done:progress=4294967296"), None);
    assert_eq!(report("state=done:progress=99999999999999999999999"), None);
    assert_eq!(
        report("state=working:progress=007").unwrap().progress,
        Some(7)
    );
    assert_eq!(
        report("state=working:progress=lots").unwrap().progress,
        None,
        "not a number: a malformed pair, skipped"
    );
    assert_eq!(
        report("state=idle:app=codex").unwrap().app.as_deref(),
        Some("codex")
    );
    assert!(report(&format!("state=idle:app={}", "a".repeat(32))).is_some());
    assert_eq!(report(&format!("state=idle:app={}", "a".repeat(33))), None);
}

#[test]
fn title_and_msg_are_base64_utf8_without_control_characters() {
    // "Привет" and "waiting for you".
    let read = report("state=blocked:title=0J/RgNC40LLQtdGC:msg=d2FpdGluZyBmb3IgeW91").unwrap();
    assert_eq!(read.title.as_deref(), Some("Привет"));
    assert_eq!(read.msg.as_deref(), Some("waiting for you"));
    assert_eq!(
        report("state=idle:title=!!!!").unwrap().title,
        None,
        "outside the value alphabet: a malformed pair, skipped"
    );
    assert_eq!(
        report("state=idle:title=A==="),
        None,
        "three padding characters"
    );
    assert_eq!(report("state=idle:title=AB=C"), None, "padding inside");
    assert_eq!(report("state=idle:title=YWJj="), None, "bad padding");
    assert_eq!(report("state=idle:title=/w=="), None, "0xff is not UTF-8");
    assert_eq!(
        report("state=idle:msg=YQph"),
        None,
        "a newline is a control character"
    );
    let title = "a".repeat(192);
    assert!(report(&format!("state=idle:title={}", encode(&title))).is_some());
    assert_eq!(
        report(&format!("state=idle:title={}", encode(&(title + "a")))),
        None,
        "193 bytes"
    );
    let msg = "m".repeat(2048);
    assert!(report(&format!("state=idle:msg={}", encode(&msg))).is_some());
    assert_eq!(
        report(&format!("state=idle:msg={}", encode(&(msg + "m")))),
        None,
        "2049 bytes"
    );
}

#[test]
fn raw_control_characters_or_non_utf8_discard_the_report() {
    assert_eq!(parse_report(b"state=idle:app=a\x01b"), None);
    assert_eq!(parse_report(b"state=idle\x7f"), None);
    assert_eq!(parse_report(b"state=idle:app=\xff"), None);
}

#[test]
fn unknown_keys_are_ignored_malformed_pairs_skipped_and_the_last_value_wins() {
    let read = report("future=thing:state=working:nonsense:=x:state=done").unwrap();
    assert_eq!(read.state, State::Done);
    assert_eq!(
        report("state=idle:state=wor king").unwrap().state,
        State::Idle,
        "a value outside the alphabet is a malformed pair; the earlier one stands"
    );
    assert_eq!(
        report("state=idle:id=a:id=b").unwrap().id.as_deref(),
        Some("b")
    );
    assert_eq!(
        report("state=idle:id=a:id=b//c"),
        None,
        "the last id is invalid"
    );
}

#[test]
fn the_sequence_limit_is_four_kilobytes() {
    assert_eq!(SEQUENCE_LIMIT, 4096);
}

/// Standard base64 with padding — what a program writes into `title`.
fn encode(text: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ── Records ─────────────────────────────────────────────────────────────
//
// A terminal keeps the latest report per record — the root, which is the
// session, and any `id` children — by the protocol's lifecycle.

use guild_desktop::terminal::program_status::{RECORD_LIMIT, Records};

fn apply(records: &mut Records, text: &str) {
    records.apply(report(text).expect(text));
}

#[test]
fn a_report_replaces_its_record_whole() {
    let mut records = Records::default();
    apply(&mut records, "state=blocked:kind=question:app=claude");
    apply(&mut records, "state=working");
    let root = records.root().unwrap();
    assert_eq!(root.state, State::Working);
    assert_eq!(root.kind, None, "nothing carried over");
    assert_eq!(root.app, None);
}

#[test]
fn the_root_is_the_record_without_an_id() {
    let mut records = Records::default();
    apply(&mut records, "state=working:id=agent1");
    assert!(records.root().is_none(), "a child is not the session");
    apply(&mut records, "state=idle");
    apply(&mut records, "state=done:id=agent2");
    assert_eq!(records.root().unwrap().state, State::Idle);
    assert_eq!(records.len(), 3);
}

#[test]
fn clear_with_an_id_removes_its_subtree_and_without_one_everything() {
    let mut records = Records::default();
    for text in [
        "state=idle",
        "state=working:id=a",
        "state=working:id=a/b",
        "state=working:id=a/b/c",
        "state=working:id=ab",
    ] {
        apply(&mut records, text);
    }
    apply(&mut records, "state=clear:id=a");
    assert_eq!(records.len(), 2, "the root and `ab` stay");
    assert!(records.child("ab").is_some());
    assert!(records.child("a/b").is_none());
    apply(&mut records, "state=clear");
    assert_eq!(records.len(), 0);
    assert!(records.root().is_none());
}

#[test]
fn at_most_256_records_are_kept_and_the_root_always_fits() {
    assert_eq!(RECORD_LIMIT, 256);
    let mut records = Records::default();
    for n in 0..300 {
        apply(&mut records, &format!("state=working:id=s{n}"));
    }
    assert_eq!(records.len(), RECORD_LIMIT - 1, "one place is the root's");
    assert!(
        records.child("s299").is_none(),
        "a new child past the limit is not kept"
    );
    apply(&mut records, "state=done:id=s0");
    assert_eq!(
        records.child("s0").unwrap().state,
        State::Done,
        "an existing one is replaced"
    );
    apply(&mut records, "state=working");
    assert_eq!(records.len(), RECORD_LIMIT);
    assert_eq!(records.root().unwrap().state, State::Working);
}

#[test]
fn working_and_blocked_end_with_the_process_or_the_prompt() {
    let mut records = Records::default();
    apply(&mut records, "state=working");
    apply(&mut records, "state=blocked:id=a");
    apply(&mut records, "state=done:id=b");
    apply(&mut records, "state=error:id=c");
    apply(&mut records, "state=idle:id=d");
    records.end_active();
    assert!(records.root().is_none());
    assert!(records.child("a").is_none());
    for id in ["b", "c", "d"] {
        assert!(records.child(id).is_some(), "{id} persists until replaced");
    }
    records.reset();
    assert_eq!(records.len(), 0);
}

// ── The root record's changes, in order ─────────────────────────────────
//
// The window polls on its own clock, so a `working` then `done` between two
// polls must not collapse into the last value: each change of the root
// record is queued, numbered, and drained in order. Children never enter.

use guild_desktop::terminal::program_status::CHANGE_LIMIT;

fn states(records: &mut Records) -> (Vec<Option<State>>, u64) {
    let (changes, lost) = records.take_changes();
    let mut last = None;
    for change in &changes {
        assert!(last < Some(change.sequence), "numbered in order");
        last = Some(change.sequence);
    }
    (
        changes
            .into_iter()
            .map(|change| change.record.map(|report| report.state))
            .collect(),
        lost,
    )
}

#[test]
fn every_change_of_the_root_record_is_queued_in_order() {
    let mut records = Records::default();
    apply(&mut records, "state=working");
    apply(&mut records, "state=working:id=agent1");
    apply(&mut records, "state=done");
    apply(&mut records, "state=working");
    apply(&mut records, "state=done");
    assert_eq!(
        states(&mut records),
        (
            vec![
                Some(State::Working),
                Some(State::Done),
                Some(State::Working),
                Some(State::Done)
            ],
            0
        )
    );
    assert_eq!(states(&mut records), (vec![], 0), "drained");
}

#[test]
fn a_report_equal_to_the_root_record_is_not_a_change() {
    let mut records = Records::default();
    apply(&mut records, "state=done");
    apply(&mut records, "state=done");
    apply(&mut records, "state=done:id=a");
    assert_eq!(states(&mut records), (vec![Some(State::Done)], 0));
}

#[test]
fn the_root_record_ending_is_a_change_too() {
    let mut records = Records::default();
    apply(&mut records, "state=working");
    records.end_active();
    apply(&mut records, "state=idle");
    apply(&mut records, "state=clear");
    apply(&mut records, "state=done");
    records.reset();
    records.reset();
    apply(&mut records, "state=done:id=a");
    apply(&mut records, "state=clear:id=a");
    assert_eq!(
        states(&mut records),
        (
            vec![
                Some(State::Working),
                None,
                Some(State::Idle),
                None,
                Some(State::Done),
                None
            ],
            0
        )
    );
}

#[test]
fn an_overflowing_queue_keeps_the_latest_changes_and_counts_the_lost() {
    let mut records = Records::default();
    let total = CHANGE_LIMIT + 6;
    for n in 0..total {
        apply(
            &mut records,
            if n % 2 == 0 {
                "state=working"
            } else {
                "state=done"
            },
        );
    }
    let (changes, lost) = records.take_changes();
    assert_eq!(changes.len(), CHANGE_LIMIT);
    assert_eq!(lost, 6);
    assert_eq!(
        changes.last().unwrap().record.as_ref().map(|r| r.state),
        Some(State::Done),
        "the latest state survives"
    );
    assert_eq!(records.take_changes().1, 0, "the count is reset once told");
}
