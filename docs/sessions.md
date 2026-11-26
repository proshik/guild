# Sessions and saved instructions

Guild resumes a conversation using the harness's saved conversation identity.
Ordinary Claude resume is quiet: resuming alone does not mean Guild detected
stale instructions. Concrete CLI discovery and composition failures still appear
in the terminal's launch diagnostics.

Guild supplies additive CLI context so the harness can discover the window's
public commands. New context does not erase instructions already saved in a
Claude conversation, including earlier Guild guidance. Guild preserves saved
history and settings; it does not inspect instruction snapshots, rebuild saved
conversations or automatically compact history. If old guidance interferes,
start a fresh conversation. Compaction is not a promised way to remove saved
system text.
