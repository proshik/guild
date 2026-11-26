//! Shared headroom for state writes while every project's Git worker wakes.

use std::sync::{Condvar, Mutex, PoisonError};

// Command::output temporarily owns both ends of its stdio pipes during spawn.
// A worker per project with no shared bound reached 254 FDs on a 32-project
// workspace. Four readers leave headroom without raising the process limit.
const LIMIT: usize = 4;
static ACTIVE: Mutex<usize> = Mutex::new(0);
static AVAILABLE: Condvar = Condvar::new();

pub(super) struct Permit;

impl Permit {
    pub(super) fn acquire() -> Self {
        let active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
        let mut active = AVAILABLE
            .wait_while(active, |active| *active >= LIMIT)
            .unwrap_or_else(PoisonError::into_inner);
        *active += 1;
        Self
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
        *active -= 1;
        AVAILABLE.notify_one();
    }
}
