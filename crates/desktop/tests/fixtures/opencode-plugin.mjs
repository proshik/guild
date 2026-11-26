import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { pathToFileURL } from 'node:url';

const [pluginPath, socket] = process.argv.slice(2);
// Status comes from OSC 7501; the plugin posts only an execution start
// (a prompt went in) and the session's identity.
const events = [
  ['session.execution.started', 'prompt'],
  ['session.retry.scheduled', null],
  ['session.execution.succeeded', null],
  ['session.execution.failed', null],
  ['session.execution.interrupted', null],
  ['session.status', 'session'],
  ['permission.asked', null],
  ['question.asked', null],
];
const expected = events.filter(([, event]) => event !== null);
const received = [];
let completed;
const done = new Promise(resolve => { completed = resolve; });
const server = createServer(async (req, res) => {
  let body = '';
  for await (const chunk of req) body += chunk;
  received.push(JSON.parse(body));
  res.writeHead(204).end();
  if (received.length === expected.length) completed();
});
await new Promise(resolve => server.listen(socket, resolve));
const timer = setTimeout(() => {
  console.error(`Expected ${expected.length} reports, received ${received.length}`);
  process.exit(1);
}, 2000);
try {
  const plugin = (await import(pathToFileURL(pluginPath))).default;
  plugin.setup({ event: { async *subscribe() {
    for (const [type] of events) {
      yield { type, data: { sessionID: 'ses_conversation', status: { type: 'idle' } } };
    }
  } } });
  await done;
  // Nothing further arrives after the expected two.
  await new Promise(resolve => setTimeout(resolve, 100));
  assert.deepEqual(received.map(p => p.opencode_event), expected.map(([, event]) => event));
  for (const payload of received) {
    assert.equal(payload.session_id, 'guild-record');
    assert.equal(payload.guild_session, 'guild-record');
    assert.equal(payload.oc_session, 'ses_conversation');
  }
} finally {
  clearTimeout(timer);
  await new Promise(resolve => server.close(resolve));
}
