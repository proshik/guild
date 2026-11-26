/* Deterministic bootstrap failure/retry fixture. No provider or network calls. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    const char *dir = getenv("GUILD_FAILURE_FIXTURE");
    const char *session = getenv("GUILD_SESSION");
    if (!dir || !session) return 2;
    char path[4096], cwd[4096];
    snprintf(path, sizeof(path), "%s/%s.attempts", dir, session);
    FILE *old = fopen(path, "r"); int attempt = 0;
    if (old) { fscanf(old, "%d", &attempt); fclose(old); }
    FILE *out = fopen(path, "w"); if (!out) return 3;
    fprintf(out, "%d", ++attempt); fclose(out);
    snprintf(path, sizeof(path), "%s/%s.%d.argv", dir, session, attempt);
    out = fopen(path, "w"); if (!out) return 4;
    fprintf(out, "cwd=%s\nsession=%s\n", getcwd(cwd, sizeof(cwd)), session);
    for (int i=1;i<argc;++i) fprintf(out, "%s\n", argv[i]);
    const char *config = getenv("OPENCODE_CONFIG_CONTENT");
    if (config) fprintf(out, "opencode_config=%s\n", config);
    fclose(out);
    if (attempt > 1) { printf("RETRY_OK: exact pane/session/cwd retained; no prompt replay\r\n"); fflush(stdout); for (;;) pause(); }
    if (getenv("GUILD_FAILURE_GENERIC")) printf("authentication failed (code -32603)\r\n");
    else printf("\033[31mError: account/read failed during TUI bootstrap: account/read failed: workspace routing discovery failed (code -32603)\033[0m\r\n");
    fflush(stdout);
    close(0); close(1); close(2);
    usleep(300000);
    return 7;
}
