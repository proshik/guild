/* A local process/argv fixture. No provider, network or user configuration. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    const char *dir = getenv("GUILD_LAUNCH_FIXTURE_DIR");
    const char *session = getenv("GUILD_SESSION");
    if (!dir || !session) return 2;
    char path[4096], cwd[4096];
    snprintf(path, sizeof(path), "%s/%s.argv", dir, session);
    FILE *out = fopen(path, "wb");
    if (!out) return 3;
    for (int i = 0; i < argc; ++i) fwrite(argv[i], 1, strlen(argv[i]) + 1, out);
    fclose(out);
    snprintf(path, sizeof(path), "%s/%s.meta", dir, session);
    out = fopen(path, "w");
    fprintf(out, "%d\n%s\n%s\n", getpid(), getenv("GUILD_LAUNCH_FIXTURE_WRAPPED") ? getenv("GUILD_LAUNCH_FIXTURE_WRAPPED") : "", getcwd(cwd, sizeof(cwd)));
    fclose(out);
    snprintf(path, sizeof(path), "%s/%s.ready", dir, session);
    out = fopen(path, "w"); fclose(out);
    for (;;) pause();
}
