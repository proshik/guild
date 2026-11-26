//! Private running-binary helper entry shared by the application and isolated native fixture.
use guild_session::update::transaction::{self, Failure};
use std::{ffi::OsString, path::PathBuf};
/// Private helper verbs; recovery requires an explicit closed-app assertion and live-reference guard.
pub fn entry(args: &[OsString]) -> Option<Result<(), Failure>> {
    if args
        .first()
        .is_none_or(|arg| arg != "--guild-update-helper" && arg != "--guild-update-recover")
    {
        return None;
    }
    Some((|| {
        let recovering = args[0] == "--guild-update-recover";
        if args.len() != if recovering { 5 } else { 3 } {
            return Err(Failure::Invalid);
        }
        let id = args[2]
            .to_str()
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .filter(|id| !id.is_nil())
            .ok_or(Failure::Invalid)?;
        if recovering {
            if args[4] != "--apps-closed" {
                return Err(Failure::Invalid);
            }
            let choice = match args[3].to_str() {
                Some("restore-previous") => transaction::Recovery::RestorePrevious,
                Some("keep-current") => transaction::Recovery::KeepCurrent,
                _ => return Err(Failure::Invalid),
            };
            super::recovery::recover(PathBuf::from(&args[1]), id, choice)
        } else {
            transaction::run_helper(PathBuf::from(&args[1]), id)
        }
    })())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_exact_private_verb_can_enter_and_malformed_requests_do_not_read_state() {
        assert_eq!(
            entry(&["--guild-update-recover".into()]),
            Some(Err(Failure::Invalid))
        );
        assert!(entry(&[]).is_none());
        assert!(entry(&["status".into()]).is_none());
        for args in [
            vec!["--guild-update-helper".into()],
            vec![
                "--guild-update-helper".into(),
                "/not-a-state".into(),
                "bad".into(),
            ],
            vec![
                "--guild-update-helper".into(),
                "/not-a-state".into(),
                uuid::Uuid::nil().to_string().into(),
            ],
        ] {
            assert_eq!(entry(&args), Some(Err(Failure::Invalid)));
        }
    }
}
