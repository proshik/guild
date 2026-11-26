//! A worktree's merge request, read from its forge — GLD-063.
//!
//! [`status`] is the provider-neutral model the column draws from. The
//! GitLab adapter beside it is `glab api` and nothing else: Guild holds no
//! token and writes nothing to GitLab (decision 56).
pub mod glab;
pub mod hosts;
pub mod remote;
pub mod status;
