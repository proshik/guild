/* A stand-in agent for the launch's door check (GLD-078): it records
 * whether its status door — Codex's hook block, opencode's plugin — was on
 * disk before its terminal's process began. No provider, network or user
 * configuration: every path is under GUILD_LAUNCH_FIXTURE_DIR.
 *
 * "Before" is measured against the start time of the terminal's session
 * leader (the process the launch forked), not against this program's own
 * check, so a door written just after the spawn is caught however quickly
 * the window wrote it. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/sysctl.h>
#include <unistd.h>

/* The start of `pid`, in microseconds since the epoch, or -1. */
static long long started(pid_t pid) {
    int mib[4] = {CTL_KERN, KERN_PROC, KERN_PROC_PID, pid};
    struct kinfo_proc info;
    size_t size = sizeof(info);
    if (sysctl(mib, 4, &info, &size, NULL, 0) != 0 || size == 0) return -1;
    return (long long)info.kp_proc.p_starttime.tv_sec * 1000000LL +
           info.kp_proc.p_starttime.tv_usec;
}

/* Whether `path` holds `needle`; an empty needle asks only that it exists. */
static int holds(const char *path, const char *needle) {
    FILE *in = fopen(path, "rb");
    if (!in) return 0;
    static char text[1 << 16];
    size_t length = fread(text, 1, sizeof(text) - 1, in);
    fclose(in);
    text[length] = 0;
    return !*needle || strstr(text, needle) != NULL;
}

int main(int argc, char **argv) {
    (void)argc;
    const char *dir = getenv("GUILD_LAUNCH_FIXTURE_DIR");
    const char *session = getenv("GUILD_SESSION");
    if (!dir || !session) return 2;
    const char *name = strrchr(argv[0], '/');
    name = name ? name + 1 : argv[0];
    char door[4096], path[4096];
    const char *needle;
    if (strcmp(name, "codex") == 0) {
        snprintf(door, sizeof(door), "%s/codex-home/config.toml", dir);
        needle = "# >>> guild hooks >>>";
    } else if (strcmp(name, "opencode") == 0) {
        snprintf(door, sizeof(door), "%s/xdg-home/opencode/plugins/opencode-status.js", dir);
        needle = "";
    } else {
        return 4;
    }
    const char *verdict = "absent";
    struct stat meta;
    if (holds(door, needle) && stat(door, &meta) == 0) {
        long long written = (long long)meta.st_mtimespec.tv_sec * 1000000LL +
                            meta.st_mtimespec.tv_nsec / 1000;
        long long leader = started(getsid(0));
        verdict = leader < 0 ? "unknown" : written <= leader ? "before" : "after";
    }
    snprintf(path, sizeof(path), "%s/%s.door", dir, session);
    FILE *out = fopen(path, "w");
    if (!out) return 3;
    fprintf(out, "%s\n", verdict);
    fclose(out);
    for (;;) pause();
}
