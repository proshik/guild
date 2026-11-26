//! One owned, bounded LSP connection. Only workers wait for IO/capacity.
use super::{Incoming, parse, read_header};
use crate::transport::{Interruptible, Queue, Workers};
use guild_session::process::ProcessGroup;
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const ITEMS: usize = 256;
const BYTES: usize = 32 * 1024 * 1024;
const STDERR_FRAGMENT: usize = 8 * 1024;
const REQUEST_RETENTION: Duration = Duration::from_secs(120);
type Answers = Arc<Mutex<State>>;
#[derive(Debug, Default)]
struct State {
    pending: HashMap<u64, Pending>,
    retained_bytes: usize,
    reason: Option<String>,
}
#[derive(Debug)]
struct Pending {
    answer: Option<Result<Value, String>>,
    bytes: usize,
    document: Option<String>,
    started: Instant,
}
impl State {
    fn remove(&mut self, id: u64) -> Option<Pending> {
        let p = self.pending.remove(&id)?;
        self.retained_bytes -= p.bytes;
        Some(p)
    }
    fn finish(&mut self, reason: String) {
        if self.reason.is_none() {
            self.reason = Some(reason);
        }
        self.pending.clear();
        self.retained_bytes = 0;
    }
}
struct Connection {
    state: Answers,
    outgoing: Arc<Queue<Vec<u8>>>,
    events: Arc<Queue<Incoming>>,
    stop: Arc<crate::transport::Stop>,
}
impl Connection {
    fn fail(&self, reason: impl Into<String>) {
        self.state.lock().unwrap().finish(reason.into());
        self.stop.store(true, Ordering::Release);
        self.outgoing.close();
        self.events.close();
    }
    fn send(&self, bytes: Vec<u8>) {
        let count = bytes.len();
        if let Err(e) = self.outgoing.try_push(bytes, count) {
            self.fail(format!("LSP outgoing {e} (256 operations / 32 MiB)"));
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.fail("language server connection closed");
    }
}
fn encode(value: Value) -> Result<Vec<u8>, String> {
    struct Limited(Vec<u8>);
    impl Write for Limited {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if b.len() > super::BODY_LIMIT - self.0.len() {
                return Err(std::io::Error::other(
                    "LSP outgoing body exceeds 16 MiB limit",
                ));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut body = Limited(Vec::new());
    serde_json::to_writer(&mut body, &value)
        .map_err(|_| "LSP outgoing body exceeds 16 MiB or cannot be encoded".to_owned())?;
    Ok(super::frame(&body.0))
}
/// Exclusive lease; raw protocol IDs never authorize retained payload access.
#[derive(Debug)]
pub struct Request {
    id: u64,
    answers: Answers,
    outgoing: Weak<Connection>,
    settled: AtomicBool,
}
#[derive(Debug)]
pub(crate) struct Cancellation {
    id: u64,
    answers: Weak<Mutex<State>>,
    outgoing: Weak<Connection>,
}
fn cancel(answers: &Answers, outgoing: &Weak<Connection>, id: u64) {
    let removed = answers.lock().unwrap().remove(id).is_some();
    if removed
        && let Some(c) = outgoing.upgrade()
        && let Ok(b) = encode(
            serde_json::json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}),
        )
    {
        // Local revocation is authoritative. A best-effort protocol hint must
        // not evict independent leases when the ordered outgoing queue is full.
        let count = b.len();
        let _ = c.outgoing.try_push(b, count);
    }
}
impl Request {
    pub fn take_answer(&self) -> Option<Result<Value, String>> {
        let mut s = self.answers.lock().unwrap();
        if self.settled.load(Ordering::Acquire) {
            return None;
        }
        let result = match s.pending.get_mut(&self.id) {
            Some(p) if p.started.elapsed() >= REQUEST_RETENTION => {
                Some(Err("language server request expired".into()))
            }
            Some(p) => p.answer.take(),
            None => Some(Err(s
                .reason
                .clone()
                .unwrap_or_else(|| "language server request cancelled".into()))),
        };
        if result.is_some() {
            s.remove(self.id);
            self.settled.store(true, Ordering::Release);
        }
        result
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        Cancellation {
            id: self.id,
            answers: Arc::downgrade(&self.answers),
            outgoing: self.outgoing.clone(),
        }
    }
    pub(crate) fn cancel(&self) {
        cancel(&self.answers, &self.outgoing, self.id);
    }
    #[cfg(test)]
    pub(crate) fn age_for_test(&self, age: Duration) {
        if let Some(p) = self.answers.lock().unwrap().pending.get_mut(&self.id) {
            p.started = Instant::now() - age;
        }
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Some(a) = self.answers.upgrade() {
            cancel(&a, &self.outgoing, self.id);
        }
    }
}
pub struct Client {
    name: String,
    connection: Arc<Connection>,
    answers: Answers,
    workers: Option<Workers>,
    next_id: u64,
}
impl Client {
    pub fn start(command: &str, args: &[String], cwd: &Path) -> std::io::Result<Self> {
        let mut cmd = Command::new(command);
        if let Some(path) = guild_session::login_path::path() {
            cmd.env("PATH", path);
        }
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::network::apply(&mut cmd, guild_session::network::Scope::Tools)
            .map_err(std::io::Error::other)?;
        // The cleanup worker is installed before any process exists. Even its
        // own thread-creation failure cannot leave a child to stop/join on GPUI.
        let child_slot = Arc::new(Mutex::new(None::<ProcessGroup>));
        let cleanup_child = child_slot.clone();
        let mut workers = Workers::new(move || {
            if let Some(mut child) = cleanup_child.lock().unwrap().take()
                && let Err(e) = child.stop()
            {
                tracing::warn!(%e, "LSP process group cleanup failed");
            }
        })?;
        let mut child = ProcessGroup::spawn(&mut cmd).map_err(|e| {
            std::io::Error::new(e.kind(), format!("could not start {command}: {e}"))
        })?;
        #[cfg(test)]
        let owned_pid = child.id();
        let stdin = child.take_stdin().expect("piped");
        let stdout = child.take_stdout().expect("piped");
        let stderr = child.take_stderr().expect("piped");
        *child_slot.lock().unwrap() = Some(child);
        let answers = Arc::new(Mutex::new(State::default()));
        let c = Arc::new(Connection {
            state: answers.clone(),
            outgoing: Queue::new(ITEMS, BYTES, &workers.stop),
            events: Queue::new(ITEMS, BYTES, &workers.stop),
            stop: workers.stop.clone(),
        });
        let mut stdin = Interruptible::new(stdin, workers.stop.clone())?;
        let frame_deadline = Arc::new(Mutex::new(None));
        let stdout =
            Interruptible::new(stdout, workers.stop.clone())?.with_deadline(frame_deadline.clone());
        let frame_timeout = REQUEST_RETENTION;
        #[cfg(test)]
        let frame_timeout = TEST_FRAME_TIMEOUT
            .with(|t| t.take())
            .unwrap_or(frame_timeout);
        let mut stderr = Interruptible::new(stderr, workers.stop.clone())?;
        let writing = c.clone();
        workers.handles.push(
            std::thread::Builder::new()
                .name(format!("lsp-write-{command}"))
                .spawn(move || {
                    while let Some(mut permit) = writing.outgoing.pop(true, &writing.stop) {
                        let bytes = permit.value.take().unwrap();
                        if let Err(e) = stdin.write_all(&bytes) {
                            writing.fail(format!("LSP input IO failed: {e}"));
                            break;
                        }
                        // bytes and permit live through the complete write.
                    }
                })?,
        );
        let reading = c.clone();
        workers.handles.push(
            std::thread::Builder::new()
                .name(format!("lsp-read-{command}"))
                .spawn(move || {
                    let mut stdout = super::reader(stdout);
                    loop {
                        *frame_deadline.lock().unwrap() = None;
                        let length = match read_header(&mut stdout, || {
                            *frame_deadline.lock().unwrap() = Some(Instant::now() + frame_timeout);
                        }) {
                            Ok(Some(n)) => n,
                            Ok(None) => {
                                reading.fail("LSP output closed");
                                break;
                            }
                            Err(e) => {
                                reading.fail(format!("LSP framing/IO failure: {e}"));
                                break;
                            }
                        };
                        // Reserve before body allocation/decoding, so a frame waiting
                        // for a slow event consumer is included in the byte/item cap.
                        let mut event_permit =
                            match reading.events.reserve_wait(length, &reading.stop) {
                                Ok(p) => p,
                                Err(_) => break,
                            };
                        // Reader-capacity backpressure is not peer frame time.
                        *frame_deadline.lock().unwrap() = Some(Instant::now() + frame_timeout);
                        let mut body = vec![0; length];
                        if let Err(e) = stdout.read_exact(&mut body) {
                            reading.fail(format!("LSP bounded body IO/truncation: {e}"));
                            break;
                        }
                        let bytes = body.len();
                        match parse(&body) {
                            Some(Incoming::Answer { id, result }) => {
                                let mut s = reading.state.lock().unwrap();
                                if s.reason.is_some() {
                                    break;
                                }
                                if let Some(p) = s.pending.get(&id) {
                                    if p.started.elapsed() >= REQUEST_RETENTION {
                                        s.remove(id);
                                    } else if p.answer.is_none() {
                                        if bytes > BYTES - s.retained_bytes {
                                            drop(s);
                                            reading
                                                .fail("LSP retained answers exceed 32 MiB limit");
                                            break;
                                        }
                                        let p = s.pending.get_mut(&id).unwrap();
                                        p.bytes += bytes;
                                        p.answer = Some(result);
                                        s.retained_bytes += bytes;
                                    }
                                }
                            }
                            Some(event) => {
                                event_permit.value = Some(event);
                                drop(body);
                                if let Err(e) = reading.events.enqueue(event_permit) {
                                    if !reading.stop.load(Ordering::Acquire) {
                                        reading.fail(format!("LSP event {e}"));
                                    }
                                    break;
                                }
                            }
                            None => {
                                reading.fail("LSP body is malformed JSON or protocol message");
                                break;
                            }
                        }
                    }
                })?,
        );
        #[cfg(test)]
        if FAIL_PARTIAL.with(|f| f.replace(false)) {
            PARTIAL_OWNER.with(|o| {
                *o.borrow_mut() = Some((owned_pid, workers.finished.clone(), workers.handles.len()))
            });
            return Err(std::io::Error::other(
                "owned fixture: stderr thread creation failed",
            ));
        }
        let stop = workers.stop.clone();
        let name = command.to_owned();
        workers.handles.push(std::thread::Builder::new().name(format!("lsp-err-{command}")).spawn(move||{
   let mut fragment=[0;STDERR_FRAGMENT];
   while !stop.load(Ordering::Acquire){match stderr.read(&mut fragment){Ok(0)|Err(_)=>break,Ok(n)=>{tracing::debug!(server=%name,"{}",String::from_utf8_lossy(&fragment[..n]));}}}
  })?);
        Ok(Self {
            name: command.to_owned(),
            connection: c,
            answers,
            workers: Some(workers),
            next_id: 1,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn request(&mut self, method: &str, params: Value) -> Request {
        let id = self.next_id;
        self.next_id += 1;
        let lease = Request {
            id,
            answers: self.answers.clone(),
            outgoing: Arc::downgrade(&self.connection),
            settled: AtomicBool::new(false),
        };
        // Registration and terminal state are serialized by this same lock.
        let mut s = self.answers.lock().unwrap();
        if s.reason.is_some() {
            return lease;
        }
        if s.pending.len() == ITEMS {
            drop(s);
            self.connection
                .fail("LSP pending request limit reached (256)");
            return lease;
        }
        let uri = params.pointer("/textDocument/uri").and_then(Value::as_str);
        let metadata = uri.map_or(0, str::len);
        if metadata > BYTES - s.retained_bytes {
            drop(s);
            self.connection
                .fail("LSP retained request metadata/answers exceed 32 MiB limit");
            return lease;
        }
        let document = uri.map(str::to_owned);
        s.retained_bytes += metadata;
        s.pending.insert(
            id,
            Pending {
                answer: None,
                bytes: metadata,
                document,
                started: Instant::now(),
            },
        );
        drop(s);
        self.send_value(
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        );
        lease
    }
    pub fn cancel_document(&self, uri: &str) {
        let ids: Vec<_> = self
            .answers
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter(|(_, p)| p.document.as_deref() == Some(uri))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            cancel(&self.answers, &Arc::downgrade(&self.connection), id);
        }
    }
    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.answers.lock().unwrap().pending.len()
    }
    pub fn notify(&self, method: &str, params: Value) {
        self.send_value(serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}));
    }
    pub fn answer(&self, id: u64, result: Value) {
        self.send_value(serde_json::json!({"jsonrpc":"2.0","id":id,"result":result}));
    }
    fn send_value(&self, value: Value) {
        if !self.is_alive() {
            return;
        }
        match encode(value) {
            Ok(b) => self.connection.send(b),
            Err(e) => self.connection.fail(e),
        }
    }
    pub fn poll(&mut self) -> Events {
        let ids: Vec<_> = self
            .answers
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter(|(_, p)| p.started.elapsed() >= REQUEST_RETENTION)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            cancel(&self.answers, &Arc::downgrade(&self.connection), id);
        }
        let mut batch = Events {
            messages: Vec::new(),
            permits: Vec::new(),
        };
        for _ in 0..ITEMS {
            let Some(mut p) = self.connection.events.pop(false, &self.connection.stop) else {
                break;
            };
            batch.messages.push(p.value.take().unwrap());
            batch.permits.push(p);
        }
        batch
    }
    pub fn is_alive(&self) -> bool {
        self.answers.lock().unwrap().reason.is_none()
    }
    pub fn failure_reason(&self) -> Option<String> {
        self.answers.lock().unwrap().reason.clone()
    }
    pub fn stop(&mut self) {
        self.connection.fail("language server stopped");
        self.workers.take();
    }
}
/// Drained events retain transport permits through caller processing.
pub struct Events {
    messages: Vec<Incoming>,
    permits: Vec<crate::transport::Permit<Incoming>>,
}
impl Events {
    pub fn iter(&self) -> std::slice::Iter<'_, Incoming> {
        self.messages.iter()
    }
}
pub struct EventIter {
    messages: std::vec::IntoIter<Incoming>,
    _permits: Vec<crate::transport::Permit<Incoming>>,
}
impl Iterator for EventIter {
    type Item = Incoming;
    fn next(&mut self) -> Option<Incoming> {
        self.messages.next()
    }
}
impl IntoIterator for Events {
    type Item = Incoming;
    type IntoIter = EventIter;
    fn into_iter(self) -> EventIter {
        EventIter {
            messages: self.messages.into_iter(),
            _permits: self.permits,
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(test)]
pub(crate) mod lifetime_tests {
    use super::*;
    use std::time::{Duration, Instant};

    // A release notification gates responses. The barrier is dispatched after
    // every answer, so assertions never depend on a reader-thread sleep.
    pub(crate) fn fake() -> Client {
        Client::start(
            "/usr/bin/python3",
            &[
                "-u".into(),
                "-c".into(),
                r#"
import json, signal, sys
signal.alarm(20)
held = []
def send(v):
    b = json.dumps(v).encode()
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(b) + b)
    sys.stdout.buffer.flush()
while True:
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line: sys.exit(0)
        if line == b'\r\n': break
        if line.lower().startswith(b'content-length:'):
            length = int(line.split(b':')[1])
    m = json.loads(sys.stdin.buffer.read(length))
    if m.get('method') == 'release':
        selected = m.get('params', {}).get('method')
        ready = [r for r in held if selected is None or r['method'] == selected]
        held = [r for r in held if r not in ready]
        for r in ready:
            send(dict(jsonrpc='2.0', id=r['id'], result='x' * 1048576))
            if r.get('params', {}).get('duplicate'):
                send(dict(jsonrpc='2.0', id=r['id'], result='duplicate'))
        send(dict(jsonrpc='2.0', id=999999, result='unsolicited'))
        send(dict(jsonrpc='2.0', method='barrier', params={}))
    elif m.get('method') == 'exit': sys.exit(0)
    elif 'id' in m: held.append(m)
"#
                .into(),
            ],
            Path::new("/"),
        )
        .unwrap()
    }

    pub(crate) fn released(client: &mut Client) {
        release_method(client, None);
    }

    pub(crate) fn release_method(client: &mut Client, method: Option<&str>) {
        client.notify("release", serde_json::json!({"method":method}));
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if client.poll().iter().any(|m| {
                matches!(m,
                Incoming::Notification { method, .. } if method == "barrier")
            }) {
                return;
            }
            assert!(Instant::now() < until, "fake LSP release barrier");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn dropped_request_rejects_late_payload_and_unknown_ids() {
        let mut client = fake();
        for _ in 0..100 {
            let _abandoned = client.request("held", serde_json::json!({}));
        }
        released(&mut client);
        assert!(
            client.answers.lock().unwrap().pending.is_empty(),
            "a dropped consumer retained its late one-MiB payload or unsolicited answer"
        );
    }

    #[test]
    fn eof_releases_unconsumed_payloads_without_another_request() {
        let mut client = fake();
        let _waiting = client.request("held", serde_json::json!({}));
        released(&mut client);
        assert!(!client.answers.lock().unwrap().pending.is_empty());
        client.notify("exit", serde_json::json!({}));
        let until = Instant::now() + Duration::from_secs(10);
        while client.is_alive() {
            assert!(Instant::now() < until, "fake LSP EOF");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            client.answers.lock().unwrap().pending.is_empty(),
            "EOF left payloads retained by the client"
        );
    }
    #[test]
    fn concurrent_requests_cancel_independently_before_and_after_delivery() {
        let mut client = fake();
        let first = client.request(
            "held",
            serde_json::json!({"textDocument":{"uri":"file:///a"}}),
        );
        let second = client.request(
            "held",
            serde_json::json!({"textDocument":{"uri":"file:///b"}, "duplicate":true}),
        );
        let workspace = client.request("workspace/symbol", serde_json::json!({"query":"x"}));
        client.cancel_document("file:///a");
        assert_eq!(client.pending_count(), 2);
        released(&mut client);
        assert!(first.take_answer().unwrap().is_err());
        assert_eq!(
            second
                .take_answer()
                .unwrap()
                .unwrap()
                .as_str()
                .unwrap()
                .len(),
            1048576
        );
        assert!(
            second.take_answer().is_none(),
            "a response is consumed once"
        );
        assert_eq!(client.pending_count(), 1);
        drop(workspace); // Also release an answer that arrived before cancellation.
        assert_eq!(client.pending_count(), 0);
        let expired = client.request("held", serde_json::json!({}));
        expired.age_for_test(Duration::from_secs(121));
        released(&mut client);
        assert_eq!(
            client.pending_count(),
            0,
            "poll expires owners without another ask"
        );
        assert!(expired.take_answer().unwrap().is_err());
    }

    #[test]
    fn operation_cancellation_and_stop_release_owned_requests() {
        let mut client = fake();
        let first = client.request("held", serde_json::json!({}));
        let operation = first.cancellation();
        let control = client.request("held", serde_json::json!({}));
        drop(operation);
        released(&mut client);
        assert!(first.take_answer().unwrap().is_err());
        assert_eq!(client.pending_count(), 1);
        client.stop();
        assert_eq!(client.pending_count(), 0);
        assert!(control.take_answer().unwrap().is_err());
        let after_stop = client.request("held", serde_json::json!({}));
        assert!(after_stop.take_answer().unwrap().is_err());
        assert_eq!(client.pending_count(), 0);
    }
}

#[cfg(test)]
mod transport_bounds_tests {
    use super::*;
    #[test]
    fn unread_server_cannot_retain_unbounded_owned_requests() {
        let mut client = Client::start("/bin/sleep", &["20".into()], Path::new("/")).unwrap();
        let held: Vec<_> = (0..300)
            .map(|_| client.request("held", serde_json::json!({})))
            .collect();
        assert!(
            client.pending_count() <= 256,
            "pending request count exceeded the connection budget"
        );
        assert!(
            !client.is_alive(),
            "failed capacity admission must explicitly close the connection"
        );
        assert!(held.iter().all(|r| r.take_answer().is_some()));
    }
}

#[cfg(test)]
mod pressure_tests {
    use super::*;
    fn until(mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !f() {
            assert!(Instant::now() < deadline, "owned IO fixture deadline");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn finish(c: Client) {
        let done = c.workers.as_ref().unwrap().finished.clone();
        drop(c);
        until(|| done.load(Ordering::Acquire));
    }
    fn python(script: &str) -> Client {
        Client::start(
            "/usr/bin/python3",
            &[
                "-I".into(),
                "-u".into(),
                "-c".into(),
                format!("import signal;signal.alarm(15)\n{script}"),
            ],
            Path::new("/"),
        )
        .unwrap()
    }
    #[test]
    fn slow_event_consumer_retains_exact_capacity_and_resumes_in_order() {
        let mut c = python(
            r#"
import sys,json,time
for i in range(300):
 b=json.dumps({'jsonrpc':'2.0','method':'ordered','params':{'i':i}}).encode()
 sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n'%len(b)+b);sys.stdout.buffer.flush()
time.sleep(12)
"#,
        );
        until(|| c.connection.events.queued_items() == 256);
        let batch = c.poll();
        assert_eq!(batch.iter().count(), 256);
        assert_eq!(
            c.connection.events.usage().0,
            256,
            "processing batch retains permits"
        );
        for (i, event) in batch.iter().enumerate() {
            assert!(matches!(event,Incoming::Notification{params,..} if params["i"]==i));
        }
        drop(batch);
        let mut observed = 256;
        until(|| {
            for event in c.poll() {
                if let Incoming::Notification { params, .. } = event {
                    assert_eq!(params["i"], observed);
                    observed += 1;
                }
            }
            observed == 300
        });
        let done = c.workers.as_ref().unwrap().finished.clone();
        c.stop();
        until(|| c.connection.events.usage() == (0, 0));
        until(|| done.load(Ordering::Acquire));
    }
    #[test]
    fn retained_answers_byte_budget_clears_leases_and_reports_failure() {
        let mut c = python(
            r#"
import sys,json,time
for _ in range(3):
 length=0
 while True:
  line=sys.stdin.buffer.readline()
  if line==b'\r\n':break
  if line.lower().startswith(b'content-length:'):length=int(line.split(b':')[1])
 m=json.loads(sys.stdin.buffer.read(length));b=json.dumps({'id':m['id'],'result':'x'*(12*1024*1024)}).encode()
 sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n'%len(b)+b);sys.stdout.buffer.flush()
time.sleep(12)
"#,
        );
        let held: Vec<_> = (0..3).map(|_| c.request("large", Value::Null)).collect();
        until(|| !c.is_alive());
        assert_eq!(c.pending_count(), 0);
        assert_eq!(c.answers.lock().unwrap().retained_bytes, 0);
        for lease in held {
            assert!(lease.take_answer().unwrap().unwrap_err().contains("32 MiB"));
        }
        finish(c);
    }
    #[test]
    fn outgoing_active_write_keeps_byte_permit_and_full_admission_fails_connection() {
        let mut c = Client::start("/bin/sleep", &["15".into()], Path::new("/")).unwrap();
        let payload = Value::String("x".repeat(12 * 1024 * 1024));
        c.notify("held", payload.clone());
        until(|| c.connection.outgoing.usage().0 == 1);
        c.notify("held", payload.clone());
        assert!(c.is_alive());
        assert_eq!(c.connection.outgoing.usage().0, 2);
        let lease = c.request("held", payload);
        assert!(!c.is_alive());
        assert!(
            lease
                .take_answer()
                .unwrap()
                .unwrap_err()
                .contains("outgoing")
        );
        until(|| c.connection.outgoing.usage() == (0, 0));
        finish(c);
    }
    #[test]
    fn cancellation_at_full_outgoing_capacity_preserves_independent_leases() {
        for byte_capacity in [false, true] {
            for mode in ["drop", "document", "timeout"] {
                let dir = tempfile::tempdir().unwrap();
                let gate = dir.path().join("release");
                let mut c = python(&format!(
                    r#"
import sys,json,time,pathlib
f=sys.stdin.buffer
def read():
 n=0
 while True:
  l=f.readline()
  if l==b'\r\n':break
  if l.lower().startswith(b'content-length:'):n=int(l.split(b':')[1])
 return json.loads(f.read(n))
def send(v):
 b=json.dumps(v).encode();sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n'%len(b)+b);sys.stdout.buffer.flush()
a,b,d=read(),read(),read()
send({{'id':d['id'],'result':'already answered'}})
send({{'method':'gated','params':{{}}}})
while not pathlib.Path({}).exists():time.sleep(.002)
send({{'id':b['id'],'result':'independent completion'}})
while True:read()
"#,
                    serde_json::to_string(&gate.to_string_lossy()).unwrap()
                ));
                let cancelled = c.request(
                    "held",
                    serde_json::json!({"textDocument":{"uri":"file:///cancel"}}),
                );
                let live = c.request("held", Value::Null);
                let answered = c.request("held", Value::Null);
                until(|| {
                    c.poll().iter().any(
                        |e| matches!(e, Incoming::Notification { method, .. } if method=="gated"),
                    )
                });
                // The peer has stopped reading. The active write retains its permit.
                let frame =
                    encode(serde_json::json!({"method":"filler","params":"x".repeat(1024*1024)}))
                        .unwrap();
                c.connection.send(frame.clone());
                until(|| c.connection.outgoing.queued_items() == 0);
                if byte_capacity {
                    let remaining = BYTES - frame.len();
                    for size in [remaining / 2, remaining - remaining / 2] {
                        let overhead = encode(serde_json::json!({"method":"filler","params":""}))
                            .unwrap()
                            .len();
                        // Content-Length grows by six digits for these large frames.
                        let mut payload = size - overhead - 6;
                        loop {
                            let b = encode(
                                serde_json::json!({"method":"filler","params":"x".repeat(payload)}),
                            )
                            .unwrap();
                            if b.len() == size {
                                c.connection.send(b);
                                break;
                            }
                            payload =
                                (payload as isize + size as isize - b.len() as isize) as usize;
                        }
                    }
                    assert_eq!(c.connection.outgoing.usage().1, BYTES);
                } else {
                    for _ in 1..ITEMS {
                        c.notify("filler", Value::Null);
                    }
                    assert_eq!(c.connection.outgoing.usage().0, ITEMS);
                }
                let usage = c.connection.outgoing.usage();
                match mode {
                    "drop" => drop(cancelled),
                    "document" => {
                        c.cancel_document("file:///cancel");
                        drop(cancelled);
                    }
                    _ => {
                        cancelled.age_for_test(Duration::from_secs(121));
                        drop(c.poll());
                        drop(cancelled);
                    }
                }
                assert!(
                    c.is_alive(),
                    "{mode} at byte_capacity={byte_capacity} killed independent leases"
                );
                assert_eq!(
                    c.connection.outgoing.usage(),
                    usage,
                    "best-effort cancel must not create a backlog"
                );
                assert_eq!(answered.take_answer().unwrap().unwrap(), "already answered");
                std::fs::write(&gate, "").unwrap();
                let mut result = None;
                until(|| {
                    drop(c.poll());
                    result = live.take_answer();
                    result.is_some()
                });
                assert_eq!(result.unwrap().unwrap(), "independent completion");
                finish(c);
            }
        }
    }
    #[test]
    fn malformed_frame_and_unterminated_stderr_do_not_hold_shutdown() {
        let mut c = python(
            "import os,time\nos.write(2,b'x'*(1024*1024));os.write(1,b'Content-Length: 16777217\\r\\n\\r\\n');time.sleep(12)",
        );
        let lease = c.request("held", Value::Null);
        until(|| !c.is_alive());
        assert!(lease.take_answer().unwrap().unwrap_err().contains("16 MiB"));
        let done = c.workers.as_ref().unwrap().finished.clone();
        let stop = Instant::now();
        c.stop();
        assert!(stop.elapsed() < Duration::from_millis(100));
        until(|| done.load(Ordering::Acquire));
    }
    #[test]
    fn exact_event_byte_capacity_includes_decoded_and_processing_frames() {
        let mut c = python(
            r#"
import os,time
limit=16*1024*1024;prefix=b'{"method":"held","params":"';suffix=b'"}'
body=prefix+b'x'*(limit-len(prefix)-len(suffix))+suffix
for _ in range(2):
 data=b'Content-Length: %d\r\n\r\n'%len(body)+body
 while data:
  n=os.write(1,data);data=data[n:]
body=b'{"method":"tail"}';os.write(1,b'Content-Length: %d\r\n\r\n'%len(body)+body);time.sleep(12)
"#,
        );
        until(|| c.connection.events.queued_items() == 2);
        assert_eq!(c.connection.events.usage(), (2, BYTES));
        let batch = c.poll();
        assert_eq!(batch.iter().count(), 2);
        assert_eq!(c.connection.events.usage(), (2, BYTES));
        drop(batch);
        until(|| {
            c.poll()
                .iter()
                .any(|e| matches!(e,Incoming::Notification{method,..} if method=="tail"))
        });
        finish(c);
    }
}

#[cfg(test)]
mod retained_pipe_tests {
    use super::*;
    #[test]
    fn separately_owned_other_group_retaining_pipes_cannot_hold_local_workers() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("holder.sock");
        let held = dir.path().join("held");
        // This helper is owned directly by the TEST, in a different process group.
        // It receives copies of the actual server's stdout/stderr via SCM_RIGHTS.
        let mut cmd = Command::new("/usr/bin/python3");
        cmd.args(["-I","-u","-c",r#"
import socket,array,pathlib,time,signal,sys
signal.alarm(15);s=socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM);s.bind(sys.argv[1])
_,anc,_,_=s.recvmsg(1,socket.CMSG_SPACE(8));fds=array.array('i');fds.frombytes(anc[0][2]);pathlib.Path(sys.argv[2]).touch();time.sleep(12)
"#]);
        cmd.arg(&socket)
            .arg(&held)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut helper = ProcessGroup::spawn(&mut cmd).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        let script = format!(
            r#"
import socket,array,time,signal
signal.alarm(15);s=socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM);s.connect({socket:?});s.sendmsg([b'x'],[(socket.SOL_SOCKET,socket.SCM_RIGHTS,array.array('i',[1,2]))]);time.sleep(12)
"#,
            socket = socket.to_string_lossy()
        );
        let mut c = Client::start(
            "/usr/bin/python3",
            &["-I".into(), "-u".into(), "-c".into(), script],
            dir.path(),
        )
        .unwrap();
        while !held.exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        c.notify("blocked", Value::String("x".repeat(5 * 1024 * 1024)));
        let finished = c.workers.as_ref().unwrap().finished.clone();
        let start = Instant::now();
        c.stop();
        assert!(start.elapsed() < Duration::from_millis(100));
        let until = Instant::now() + Duration::from_secs(2);
        while !finished.load(Ordering::Acquire) {
            assert!(Instant::now() < until, "local workers waited for peer EOF");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            helper.try_wait().unwrap().is_none(),
            "production killed separately owned helper"
        );
        helper.stop().unwrap();
    }
}

#[cfg(test)]
mod ordered_transport_tests {
    use super::*;
    fn until(mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !f() {
            assert!(Instant::now() < deadline, "owned transport deadline");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn ordinary_full_sync_and_large_unicode_completion_are_ordered() {
        let mut c=Client::start("/usr/bin/python3", &["-I".into(),"-u".into(),"-c".into(),r#"
import sys,json,signal,time
signal.alarm(15);methods=[]
for _ in range(3):
 n=0
 while True:
  line=sys.stdin.buffer.readline()
  if line==b'\r\n':break
  if line.lower().startswith(b'content-length:'):n=int(line.split(b':')[1])
 m=json.loads(sys.stdin.buffer.read(n));methods.append(m['method'])
 if 'id' in m:
  b=json.dumps({'id':m['id'],'result':{'methods':methods,'completion':'🦀'*(355*1024//4)}},ensure_ascii=False).encode()
  sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n'%len(b)+b);sys.stdout.buffer.flush()
time.sleep(12)
"#.into()],Path::new("/")).unwrap();
        c.notify(
            "textDocument/didChange",
            serde_json::json!({"text":"x".repeat(5*1024*1024)}),
        );
        c.notify("textDocument/didSave", Value::Null);
        let lease = c.request("textDocument/completion", Value::Null);
        let mut received = None;
        until(|| {
            received = lease.take_answer();
            received.is_some()
        });
        let answer = received.unwrap().unwrap();
        assert_eq!(
            answer["methods"],
            serde_json::json!([
                "textDocument/didChange",
                "textDocument/didSave",
                "textDocument/completion"
            ])
        );
        assert_eq!(answer["completion"].as_str().unwrap().len(), 355 * 1024);
        assert_eq!(c.pending_count(), 0);
        assert_eq!(c.answers.lock().unwrap().retained_bytes, 0);
    }
    #[test]
    fn full_event_capacity_stop_joins_even_while_consumer_holds_batch() {
        let mut c = Client::start(
            "/usr/bin/python3",
            &[
                "-I".into(),
                "-u".into(),
                "-c".into(),
                r#"
import os,signal,time
signal.alarm(15);body=b'{"method":"held","params":null}'
for i in range(400):os.write(1,b'Content-Length: %d\r\n\r\n'%len(body)+body)
time.sleep(12)
"#
                .into(),
            ],
            Path::new("/"),
        )
        .unwrap();
        until(|| c.connection.events.queued_items() == 256);
        let batch = c.poll();
        let finished = c.workers.as_ref().unwrap().finished.clone();
        c.stop();
        until(|| finished.load(Ordering::Acquire));
        assert_eq!(c.connection.events.usage().0, 256);
        drop(batch);
        assert_eq!(c.connection.events.usage(), (0, 0));
    }
    #[test]
    fn eof_racing_registration_cannot_leave_a_live_lease() {
        for _ in 0..5 {
            let mut c = Client::start(
                "/usr/bin/python3",
                &[
                    "-I".into(),
                    "-c".into(),
                    "import time;time.sleep(.01)".into(),
                ],
                Path::new("/"),
            )
            .unwrap();
            let leases: Vec<_> = (0..100).map(|_| c.request("held", Value::Null)).collect();
            until(|| !c.is_alive());
            let after = c.request("after-eof", Value::Null);
            assert!(after.take_answer().unwrap().is_err());
            for l in leases {
                assert!(l.take_answer().unwrap().is_err());
            }
            assert_eq!(c.pending_count(), 0);
        }
    }
}

#[cfg(test)]
thread_local! {
 static FAIL_PARTIAL:std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
 static PARTIAL_OWNER:std::cell::RefCell<Option<(u32,Arc<AtomicBool>,usize)>> = const {std::cell::RefCell::new(None)};
}
#[cfg(test)]
mod partial_start_tests {
    use super::*;
    #[test]
    fn failure_after_two_started_workers_reaps_and_joins_owned_process() {
        FAIL_PARTIAL.with(|f| f.set(true));
        let result = Client::start("/bin/sleep", &["15".into()], Path::new("/"));
        assert!(result.is_err());
        let (pid, finished, threads) = PARTIAL_OWNER.with(|o| o.borrow_mut().take().unwrap());
        assert_eq!(threads, 2);
        let end = Instant::now() + Duration::from_secs(3);
        while !finished.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}

#[cfg(test)]
thread_local! { static TEST_FRAME_TIMEOUT:std::cell::Cell<Option<Duration>> = const {std::cell::Cell::new(None)}; }
#[cfg(test)]
mod incomplete_frame_tests {
    use super::*;
    #[test]
    fn partial_header_and_body_have_a_peer_deadline_but_idle_does_not() {
        for fragment in [b"Content-".as_slice(), b"Content-Length: 10\r\n\r\nxx"] {
            TEST_FRAME_TIMEOUT.with(|t| t.set(Some(Duration::from_millis(50))));
            let script = format!(
                "import os,time,signal;signal.alarm(5);os.write(1,bytes({:?}));time.sleep(3)",
                fragment
            );
            let mut c = Client::start(
                "/usr/bin/python3",
                &["-I".into(), "-c".into(), script],
                Path::new("/"),
            )
            .unwrap();
            let lease = c.request("held", Value::Null);
            let end = Instant::now() + Duration::from_secs(2);
            while c.is_alive() {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(
                lease
                    .take_answer()
                    .unwrap()
                    .unwrap_err()
                    .contains("deadline")
            );
            let finished = c.workers.as_ref().unwrap().finished.clone();
            drop(c);
            while !finished.load(Ordering::Acquire) {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        TEST_FRAME_TIMEOUT.with(|t| t.set(Some(Duration::from_millis(50))));
        let mut c = Client::start("/bin/sleep", &["3".into()], Path::new("/")).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            c.is_alive(),
            "idle connection must not time out before a frame starts"
        );
        c.stop();
    }
}

#[cfg(test)]
mod idle_wait_tests {
    use super::*;
    #[test]
    fn silent_client_has_no_periodic_transport_timeouts() {
        let mut c = Client::start("/bin/sleep", &["10".into()], Path::new("/")).unwrap();
        let stop = c.connection.stop.clone();
        let done = c.workers.as_ref().unwrap().finished.clone();
        let end = Instant::now() + Duration::from_secs(3);
        while stop.waiting.load(Ordering::SeqCst) < 3 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        stop.timeouts.store(0, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        let count = stop.timeouts.load(Ordering::SeqCst);
        c.stop();
        while !done.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            count, 0,
            "silent Client periodically timed out IO/capacity waits"
        );
    }
}
