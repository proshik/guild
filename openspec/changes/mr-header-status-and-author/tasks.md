## 1. The repository header's indicator

- [x] 1.1 Failing tests: `RepoStatus::indicator` for each state and precedence; a problem is not a row's (`Shown`); `Forge::indicators` changes when a refresh starts and lands; a Root test that a refresh starting notifies the root
- [x] 1.2 `Indicator`, `RepoStatus::indicator`, `Forge::indicators`; `poll_forge` notifies on an indicator change
- [x] 1.3 Lucide `clock`, `refresh-cw-off` in `assets/ui` and `icons::UI`; a turning glyph in `view::activity`; the header draws the indicator, its hover worded in `i18n`; a view test per state
- [x] 1.4 CHANGELOG `### Added`; `just check`; commit

## 2. "You created this merge request"

- [x] 2.1 Failing tests: the list's `author.id` is read (`merge_requests_glab.rs`); `Glab::account` reads `user`'s `id` through a stand-in glab and fails on garbage; the model marks an MR mine only when the host's account equals its author, keeps the last account when a read fails, and never marks without one; `refresh` reads the account once per host and only where an MR was found, and a failing read leaves the MR
- [x] 2.2 `MergeRequest::author`, `Glab::account`, `Answer::accounts`, `Shown::Mr::mine`, the picture
- [x] 2.3 Lucide `user` in `assets/ui` and `icons::UI`; the row's mark and hover; a view test
- [x] 2.4 Decision 56 revised; CHANGELOG `### Added`; `just check`; commit

## 3. Verification and Penpot

- [x] 3.1 The isolated dev window against the approved boards
- [x] 3.2 revust with the configured profile (dry run first), at most two rounds
- [ ] 3.3 Penpot: the proposal promoted into the Library and Screens, its boards removed, the token script checked
