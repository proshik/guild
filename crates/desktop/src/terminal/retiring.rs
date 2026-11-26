//! The session terminals whose process is still going after its tab went.
//!
//! Closing a tab hangs its terminal up and leaves the process a grace to
//! go before it is killed and reaped — on a thread, so the window never
//! waits (`Emulator::let_go`). Until that ends the conversation is still
//! held by a process, and continuing it then would put a second one on the
//! same transcript. A terminal tagged with its session is counted here from
//! the hang-up until its child is reaped, so the one asking to continue
//! can wait for the first to be gone.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static ALIVE: LazyLock<Mutex<HashMap<String, usize>>> = LazyLock::new(Default::default);

/// A terminal tagged `tag` hung up: its process is going.
pub(crate) fn begin(tag: &str) {
    *ALIVE.lock().unwrap().entry(tag.to_owned()).or_default() += 1;
}

/// That terminal's process has been reaped.
pub(crate) fn end(tag: &str) {
    let mut alive = ALIVE.lock().unwrap();
    if let Some(count) = alive.get_mut(tag) {
        *count -= 1;
        if *count == 0 {
            alive.remove(tag);
        }
    }
}

/// Whether a process of `tag`'s is still going.
pub fn alive(tag: &str) -> bool {
    ALIVE.lock().unwrap().contains_key(tag)
}

/// Wait, off the window's thread, until no process of `tag`'s is going,
/// at most `timeout`; whether it is gone.
pub fn wait_gone(tag: &str, timeout: Duration) -> bool {
    let until = Instant::now() + timeout;
    loop {
        if !alive(tag) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_alive_from_the_hang_up_until_the_last_of_its_processes_is_reaped() {
        let tag = format!("retiring-test-{}", std::process::id());
        assert!(!alive(&tag));
        begin(&tag);
        begin(&tag);
        assert!(alive(&tag));
        assert!(!wait_gone(&tag, Duration::from_millis(30)), "still going");
        end(&tag);
        assert!(alive(&tag), "one of two is still going");
        let other = tag.clone();
        let reaper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            end(&other);
        });
        assert!(wait_gone(&tag, Duration::from_secs(2)), "gone once reaped");
        reaper.join().unwrap();
        end(&tag);
        assert!(!alive(&tag), "an extra end is harmless");
    }
}
