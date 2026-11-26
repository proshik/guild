//! Compatibility with state written before terminal-only sessions.
use crate::scratch::Scratch;
use guild_session::{Desk, Runs, SessionMode, SessionRecord, Settings, Store};

#[test]
fn saved_stream_records_load_as_terminal_and_write_terminal() {
    for legacy in [false, true] {
        let dir = Scratch::absent("guild-stream-compat-");
        std::fs::create_dir_all(&dir).unwrap();
        let mut desk = Desk::default();
        let project = desk.add_project(std::path::Path::new("/work/example"));
        let record = SessionRecord::new(
            "kept conversation".into(),
            project,
            "/work/example".into(),
            Runs::Claude(SessionMode::Terminal),
        );
        let id = record.id;
        desk.add_session(record);
        let mut value = serde_json::to_value(&desk).unwrap();
        let record = &mut value["sessions"][0];
        if legacy {
            record.as_object_mut().unwrap().remove("runs");
            record["mode"] = "stream".into();
        } else {
            record["runs"] = serde_json::json!({"claude":"stream"});
        }
        let path = dir.join("workspace.json");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let store = Store::<Desk>::new(path.clone());
        let restored = store.load().unwrap();
        let record = restored.session(id).unwrap();
        assert_eq!(record.runs, Runs::Claude(SessionMode::Terminal));
        assert_eq!(record.title, "kept conversation");
        store.save(&restored).unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            written["sessions"][0]["runs"],
            serde_json::json!({"claude":"terminal"})
        );
    }
}

#[test]
fn legacy_default_mode_keeps_settings_and_is_not_written() {
    let settings: Settings =
        serde_json::from_value(serde_json::json!({"default_mode":"stream", "column_folded":true}))
            .unwrap();
    assert!(settings.column_folded);
    assert!(
        serde_json::to_value(settings)
            .unwrap()
            .get("default_mode")
            .is_none()
    );
}

#[test]
fn transcript_cost_wins_and_saved_cost_is_a_fallback() {
    for transcript in [None, Some(0.0), Some(3.0)] {
        let mut desk = Desk::default();
        let project = desk.add_project(std::path::Path::new("/work/example"));
        let mut record = SessionRecord::new(
            "conversation".into(),
            project,
            "/work/example".into(),
            Runs::Claude(SessionMode::Terminal),
        );
        record.cost_usd = 2.0;
        record.usage.cost_usd = transcript;
        desk.add_session(record);
        assert_eq!(desk.total_usage().cost_usd, Some(transcript.unwrap_or(2.0)));
    }
}

/// The same disk decision serves startup restore and reopening a closed record.
#[test]
fn legacy_stream_restore_keeps_conversation_and_missing_transcript_rules() {
    use guild_session::{Conversation, Resumption, continuation, resumption, transcript};
    for open in [true, false] {
        for contents in [
            None,
            Some("{}\n"),
            Some("{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n"),
        ] {
            let dir = Scratch::absent("guild-stream-restore-");
            std::fs::create_dir_all(dir.join("project")).unwrap();
            let mut desk = Desk::default();
            let project = desk.add_project(std::path::Path::new("/work/example"));
            let mut record = SessionRecord::new(
                "conversation".into(),
                project,
                "/work/example".into(),
                Runs::Claude(SessionMode::Terminal),
            );
            record.open = open;
            let key = record.id.to_string();
            let mut saved = serde_json::to_value(record).unwrap();
            saved["runs"] = serde_json::json!({"claude":"stream"});
            let restored: SessionRecord = serde_json::from_value(saved).unwrap();
            if let Some(contents) = contents {
                std::fs::write(dir.join("project").join(format!("{key}.jsonl")), contents).unwrap();
            }
            let result = resumption(restored.runs, &key, None, || {
                continuation(
                    transcript::has_conversation(&dir, &key),
                    transcript::is_id_taken(&dir, &key),
                )
            });
            let expected = match contents {
                None => Resumption::Continue(Conversation::New(key)),
                Some("{}\n") => Resumption::Forget,
                Some(_) => Resumption::Continue(Conversation::Resume(key)),
            };
            assert_eq!(restored.runs, Runs::Claude(SessionMode::Terminal));
            assert_eq!(restored.open, open);
            assert_eq!(result, expected);
        }
    }
}
