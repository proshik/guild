/* Darwin libproc omits zombies. Read their public kernel metadata through the
 * SDK's real kinfo_proc layout, never a Rust copy of private ABI offsets. */
#include <errno.h>
#include <stdint.h>
#include <sys/types.h>
#include <sys/sysctl.h>
#include <sys/proc.h>

int guild_process_generation(int32_t pid, int32_t *state,
                             int64_t *seconds, int64_t *microseconds) {
    if (pid <= 0 || !state || !seconds || !microseconds) return EINVAL;
    int mib[] = {CTL_KERN, KERN_PROC, KERN_PROC_PID, pid};
    struct kinfo_proc info = {0};
    size_t size = sizeof(info);
    if (sysctl(mib, 4, &info, &size, NULL, 0) < 0) return errno ? errno : EIO;
    if (size == 0) return ESRCH;
    if (size != sizeof(info) || info.kp_proc.p_pid != pid) return EIO;
    *state = (int32_t)info.kp_proc.p_stat;
    *seconds = (int64_t)info.kp_proc.p_starttime.tv_sec;
    *microseconds = (int64_t)info.kp_proc.p_starttime.tv_usec;
    return 0;
}
