## 1. Cache across a workspace switch

- [x] 1.1 Failing tests in `tests/all/forge.rs`: a repository hidden and shown again shows its last MR at once and is due; a hidden one is not asked; an answer landing while hidden never publishes; off forgets a hidden repository
- [x] 1.2 `Forge`: hide instead of remove in `retain`; skip hidden in `trigger`, `take_due`, `shown`; unhide and make due in `inputs`; `set_enabled(false)` drops the cache
- [x] 1.3 Decision 56, module comments, CHANGELOG `### Changed`; `just check`; commit

## 2. Per-branch answers and the repository's state

- [x] 2.1 Failing tests: `glab` read with one failed branch answers the other's MR (session `merge_requests_glab.rs`); deadline mid-batch times out only the rest; the model keeps a branch's own failure and its last MR; project-wide failures reach `status()` and no row; a 401 keeps badges
- [x] 2.2 `Glab::read_until` per branch; `Batch::result` per branch; `Repo` failures per branch, `problems`; `Progress`, `Problem`, `RepoStatus`, `Forge::status`, `Root::forge_status`, `Ui::mr_problem`; `Shown::Why` and its row arm removed; `refresh` turns proxy, policy and deadline-before-batch into project-wide errors
- [x] 2.3 Decision 56, module comments, CHANGELOG `### Fixed`/`### Changed`; `just check`; commit

## 3. The default branch is not asked

- [x] 3.1 Failing test: a Root whose repository shows worktrees on `main` (marked default) and `feat` asks glab for `feat` only
- [x] 3.2 `Worktree::main_branch`, filled in `root/git.rs::counted` from `git::main_branch`; excluded in `forge_take_in_the_column`; a git-level test that a real repository with `origin/HEAD` marks the right worktree
- [x] 3.3 Decision 56, CHANGELOG `### Changed`; `just check`; commit

## 4. Notify only on a visible change, and the measurement

- [x] 4.1 Failing tests: `land` of the same answer twice answers `false` the second time; a changed MR, a new failure, a new problem answer `true`; `poll_forge` does not notify `Root` for an unchanged answer
- [x] 4.2 `Forge::land` compares the repository's picture before and after
- [x] 4.3 Measure the column frame before/after: `view::column::perf_bench` (root notify on a large column) and the notifies of a steady refresh; a release dev window, visible, at a time the user chooses
  - Test platform, 40 repositories / 200 rows / 480 tabs: an answer with the same MRs 11.04 ms → 0.01 ms; a changed one 11.45 ms (a root notify 11.24 ms). Native release window (`examples/forge-frames.rs`, GPUI profiler `Window::draw`): same MRs 40 frames of 10.8 ms median → no frame; changed MR 11.3 ms; one repository 2.6–2.8 ms.
- [ ] 4.4 If the notify fix is not enough, measure and propose a per-element update with its cost — no code without agreement
  - **Deferred by the user's decision (2026-11-26): not now.** Not done. Measured: a changed MR still rebuilds the whole column — 11.3 ms a frame in a native release window with forty repositories and two hundred rows (11.45 ms on the test platform), against 2.7 ms for a one-repository column, the floor a per-row cached view could approach. Its cost — rows read many `Root` fields, and every row becoming a cached view risks stale regions (`docs/traps.md`, `view::region`) — was not taken on for this change.
- [x] 4.5 Decision 56, CHANGELOG `### Fixed`; `just check`; commit

## 5. Review

- [x] 5.1 revust with the configured profile (dry run first), at most two rounds; verify and fix findings in this branch
