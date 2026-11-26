//! The process's open-file limit, raised once at the start.
//!
//! macOS hands an application started from the Dock a soft `RLIMIT_NOFILE`
//! of 256 (`launchctl limit maxfiles`), and the window never asked for more.
//! A terminal holds about four descriptors (the pty's master and its copies)
//! and an agent session's hook and IDE connections three more, so thirty-odd
//! sessions used up 256 on 2026-11-07: the state store could not even open its
//! own directory, and every save and read failed with *Too many open files*.
//! It was not a leak, only a limit sized for a smaller application.

/// `OPEN_MAX` in `<sys/syslimits.h>`: what macOS accepts as a soft limit on
/// open files, whatever the hard one says. `setrlimit` refuses
/// `RLIM_INFINITY` as a soft limit.
const OPEN_MAX: u64 = 10_240;

/// Raise the soft open-file limit toward the hard one.
///
/// Called by the executable before it starts any thread or window. Never
/// lowers a limit that is already higher, never touches the hard limit,
/// and a refusal is logged rather than stopping the start — the window
/// worked at 256 for a while and still can. The children it starts inherit
/// the raised limit, as they would from a terminal.
pub fn raise() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: an out-parameter call on this process's own limit.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        let err = std::io::Error::last_os_error();
        tracing::warn!(%err, "the open-file limit could not be read");
        return;
    }
    let Some(soft) = target(limit.rlim_cur, limit.rlim_max, per_process()) else {
        return;
    };
    let raised = libc::rlimit {
        rlim_cur: soft,
        rlim_max: limit.rlim_max,
    };
    // SAFETY: as above; the hard limit is handed back unchanged.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raised) } != 0 {
        let err = std::io::Error::last_os_error();
        tracing::warn!(%err, from = limit.rlim_cur, to = soft, "the open-file limit could not be raised");
        return;
    }
    tracing::info!(
        from = limit.rlim_cur,
        to = soft,
        "the open-file limit was raised"
    );
}

/// The soft limit to ask for, or `None` to leave the current one alone.
///
/// The hard limit, capped at what macOS accepts for one process: `OPEN_MAX`
/// and `kern.maxfilesperproc`, whichever is lower. Only ever upwards.
fn target(soft: u64, hard: u64, per_process: Option<u64>) -> Option<u64> {
    let ceiling = hard.min(OPEN_MAX).min(per_process.unwrap_or(u64::MAX));
    (ceiling > soft).then_some(ceiling)
}

/// `kern.maxfilesperproc`, if the kernel answers.
fn per_process() -> Option<u64> {
    let mut value: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: a NUL-terminated name and an out-parameter of the size given.
    let answered = unsafe {
        libc::sysctlbyname(
            c"kern.maxfilesperproc".as_ptr(),
            (&raw mut value).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } == 0;
    (answered && value > 0).then_some(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFINITY: u64 = libc::RLIM_INFINITY;

    #[test]
    fn the_docks_256_rises_to_open_max_under_an_unlimited_hard_limit() {
        assert_eq!(target(256, INFINITY, Some(184_320)), Some(10_240));
    }

    #[test]
    fn a_lower_per_process_cap_wins_over_open_max() {
        assert_eq!(target(256, INFINITY, Some(4_096)), Some(4_096));
    }

    #[test]
    fn a_hard_limit_below_the_caps_is_the_ceiling() {
        assert_eq!(target(256, 2_048, Some(184_320)), Some(2_048));
    }

    #[test]
    fn an_unknown_per_process_cap_leaves_open_max() {
        assert_eq!(target(256, INFINITY, None), Some(10_240));
    }

    #[test]
    fn a_soft_limit_already_at_or_above_the_ceiling_is_left_alone() {
        assert_eq!(target(10_240, INFINITY, Some(184_320)), None);
        assert_eq!(target(1_048_576, INFINITY, Some(184_320)), None);
        assert_eq!(target(INFINITY, INFINITY, Some(184_320)), None);
        assert_eq!(target(2_048, 2_048, None), None);
    }

    /// The kernel's own answer is read, so a cap is real rather than assumed.
    #[test]
    fn the_kernel_names_a_per_process_cap() {
        assert!(per_process().is_some_and(|cap| cap > 0));
    }
}
