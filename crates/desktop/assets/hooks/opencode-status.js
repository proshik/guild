// Guild's OpenCode plugin — M14.
//
// OpenCode has no config hook command like Claude or Codex; what it says
// reaches a consumer through a *plugin* subscribed to the service's events.
// This is that plugin, in the form opencode V2 loads — a default export
// `{ id, setup(ctx) }` whose `setup` subscribes to the public event stream
// (`ctx.event.subscribe()`). **Status is not its business any more**:
// OpenCode 2.2 reports that itself through OSC 7501. It posts two things the
// window cannot read off the terminal — the session's own id (`session`,
// from any status event) and that an execution started (`prompt`), which a
// continuation's turn-start acknowledgement reads.
//
// **A no-op anywhere else.** Guild sets `GUILD_HOOK_SOCKET` and
// `GUILD_SESSION` only on the opencode it launches, and this plugin does
// nothing without them — which is what makes living in opencode's *global*
// plugins directory safe, for the person's own sessions as much as the
// window's. (opencode V2 has no per-launch plugin flag; the plugins
// directory is the only door.)
import { request } from "node:http";

function post(body) {
  const socket = process.env.GUILD_HOOK_SOCKET;
  if (!socket || !process.env.GUILD_SESSION) return;
  // `guild_session` beside `oc_session`, as every harness's envelope carries
  // it — M13.3: what the process was started as, so a tab whose session
  // changed is still found.
  const payload = JSON.stringify({
    ...body,
    session_id: process.env.GUILD_SESSION,
    guild_session: process.env.GUILD_SESSION,
    guild_runtime: process.env.GUILD_RUNTIME,
  });
  const req = request(
    {
      method: "POST",
      socketPath: socket,
      path: "/hook",
      headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(payload) },
    },
    (res) => res.resume(),
  );
  req.setTimeout(1000, () => req.destroy());
  req.on("error", () => {});
  req.end(payload);
}

// Read a field off a V2 event without depending on whether its payload sits
// under `event.data` or is spread on the event itself — the shape moves
// between opencode versions, so both are read.
function field(event, name) {
  const data = event?.data;
  if (data && typeof data === "object" && name in data) return data[name];
  const properties = event?.properties;
  if (properties && typeof properties === "object" && name in properties) {
    return properties[name];
  }
  return event?.[name];
}

function eventOf(event) {
  switch (event?.type) {
    // An execution started: a prompt went in.
    case "session.execution.started":
      return "prompt";
    // Only for the session id it carries; its state is OSC 7501's.
    case "session.status":
      return "session";
    default:
      return null;
  }
}

export default {
  id: "guild-status",
  setup(ctx) {
    if (!process.env.GUILD_HOOK_SOCKET || !process.env.GUILD_SESSION) {
      return;
    }
    const controller = new AbortController();
    void (async () => {
      try {
        for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
          const opencode_event = eventOf(event);
          if (!opencode_event) continue;
          const oc_session = field(event, "sessionID");
          post({
            opencode_event,
            // The harness's own session id, so the window can resume by it — M14.
            oc_session: typeof oc_session === "string" && oc_session ? oc_session : undefined,
          });
        }
      } catch {
        // A dead subscription must not bring the loader down; the window
        // simply stops hearing it.
      }
    })();
    return () => controller.abort();
  },
};