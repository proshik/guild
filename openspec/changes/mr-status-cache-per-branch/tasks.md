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
- [ ] 4.3 Measure the column frame before/after: `view::column::perf_bench` (root notify on a large column) and the notifies of a steady refresh; a release dev window, visible, at a time the user chooses
- [ ] 4.4 If the notify fix is not enough, measure and propose a per-element update with its cost — no code without agreement
- [ ] 4.5 Decision 56, CHANGELOG `### Fixed`; `just check`; commit

## 5. Review

- [ ] 5.1 revust with the configured profile (dry run first), at most two rounds; verify and fix findings in this branch
