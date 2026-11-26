// Application-owned ambient API discovery, scoped to a terminal launch.
const text = __GUILD_DISCOVERY_JSON__;
export default {
  id: "guild-cli-discovery",
  async setup(ctx) {
    if (!process.env.GUILD_SOCKET || !process.env.GUILD_SESSION) return;
    const add = (event) => {
      if (!event.system.some((part) => part.text === text)) {
        event.system.push({ type: "text", text });
      }
    };
    const registrations = [];
    try {
      for (const name of ["context", "compaction", "generate"]) {
        registrations.push(await ctx.session.hook(name, add));
      }
    } catch (error) {
      for (const registration of registrations) await registration.dispose();
      throw error;
    }
    return async () => {
      for (const registration of registrations) await registration.dispose();
    };
  },
};
