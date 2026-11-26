//! Internal byte/item permits and locally interruptible Unix IO.
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    os::fd::AsRawFd,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
trait StopTarget: Send + Sync {
    fn shutdown(&self);
}
struct WakeDescriptor {
    reader: std::os::unix::net::UnixStream,
    writer: std::os::unix::net::UnixStream,
}
// Seed owns setup before IO construction; afterward only IO wrappers hold
// descriptors strongly, so retaining a completed grid does not retain wake FDs.
struct WakeOwner {
    initial: Option<Arc<WakeDescriptor>>,
    weak: std::sync::Weak<WakeDescriptor>,
}
pub(crate) struct Stop {
    flag: AtomicBool,
    wake: Mutex<WakeOwner>,
    targets: Mutex<Vec<std::sync::Weak<dyn StopTarget>>>,
    #[cfg(test)]
    pub waiting: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub timeouts: std::sync::atomic::AtomicUsize,
}
impl Stop {
    fn new() -> io::Result<Arc<Self>> {
        // std UnixStream::pair creates CLOEXEC descriptors. Verify rather than
        // exposing a non-CLOEXEC fallback to concurrent child construction.
        let (reader, writer) = std::os::unix::net::UnixStream::pair()?;
        for fd in [reader.as_raw_fd(), writer.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            if flags < 0 || flags & libc::FD_CLOEXEC == 0 {
                return Err(io::Error::other("transport stop descriptor is not CLOEXEC"));
            }
        }
        let wake = Arc::new(WakeDescriptor { reader, writer });
        Ok(Arc::new(Self {
            flag: AtomicBool::new(false),
            wake: Mutex::new(WakeOwner {
                weak: Arc::downgrade(&wake),
                initial: Some(wake),
            }),
            targets: Mutex::new(Vec::new()),
            #[cfg(test)]
            waiting: Default::default(),
            #[cfg(test)]
            timeouts: Default::default(),
        }))
    }
    pub fn load(&self, order: Ordering) -> bool {
        self.flag.load(order)
    }
    pub fn store(&self, value: bool, order: Ordering) {
        debug_assert!(value, "stop signals are terminal");
        if !self.flag.swap(true, order) {
            // EOF on this LOCAL pair stays readable for every polling worker;
            // no worker consumes a wake byte or depends on a peer's pipe EOF.
            let wake = self.wake.lock().unwrap();
            let descriptor = wake.initial.clone().or_else(|| wake.weak.upgrade());
            drop(wake);
            if let Some(descriptor) = descriptor {
                let _ = descriptor.writer.shutdown(std::net::Shutdown::Write);
            }
            let targets = std::mem::take(&mut *self.targets.lock().unwrap());
            for target in targets {
                if let Some(target) = target.upgrade() {
                    target.shutdown();
                }
            }
        }
    }
    fn descriptor(&self) -> io::Result<Arc<WakeDescriptor>> {
        let mut wake = self.wake.lock().unwrap();
        wake.initial
            .take()
            .or_else(|| wake.weak.upgrade())
            .ok_or_else(|| io::Error::other("transport IO ownership already released"))
    }
    fn register(&self, target: Arc<dyn StopTarget>) {
        let mut targets = self.targets.lock().unwrap();
        if self.load(Ordering::Acquire) {
            drop(targets);
            target.shutdown();
        } else {
            targets.push(Arc::downgrade(&target));
        }
    }
    #[cfg(test)]
    fn begin_wait(&self) {
        self.waiting.fetch_add(1, Ordering::SeqCst);
    }
    #[cfg(test)]
    fn end_wait(&self, timeout: bool) {
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        if timeout {
            self.timeouts.fetch_add(1, Ordering::SeqCst);
        }
    }
}
struct State<T> {
    queue: VecDeque<(T, usize)>,
    items: usize,
    bytes: usize,
    closed: bool,
}
pub(crate) struct Queue<T> {
    state: Mutex<State<T>>,
    wake: Condvar,
    items: usize,
    bytes: usize,
}
impl<T> Queue<T> {
    pub fn new(items: usize, bytes: usize, stop: &Stop) -> Arc<Self>
    where
        T: Send + 'static,
    {
        let queue = Arc::new(Self {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                items: 0,
                bytes: 0,
                closed: false,
            }),
            wake: Condvar::new(),
            items,
            bytes,
        });
        stop.register(queue.clone());
        queue
    }
    pub fn try_push(&self, value: T, bytes: usize) -> Result<(), &'static str> {
        self.try_push_before(value, bytes, || {})
    }
    pub fn try_push_before(
        &self,
        value: T,
        bytes: usize,
        before: impl FnOnce(),
    ) -> Result<(), &'static str> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Err("transport closed");
        }
        if s.items == self.items || bytes > self.bytes.saturating_sub(s.bytes) {
            return Err("transport operation/byte capacity exceeded");
        }
        before();
        s.items += 1;
        s.bytes += bytes;
        s.queue.push_back((value, bytes));
        self.wake.notify_all();
        Ok(())
    }
    pub fn reserve_wait(
        self: &Arc<Self>,
        bytes: usize,
        stop: &Stop,
    ) -> Result<Permit<T>, &'static str> {
        if bytes > self.bytes {
            return Err("transport operation exceeds byte capacity");
        }
        let mut s = self.state.lock().unwrap();
        while !s.closed
            && !stop.load(Ordering::Acquire)
            && (s.items == self.items || bytes > self.bytes - s.bytes)
        {
            #[cfg(test)]
            stop.begin_wait();
            s = self.wake.wait(s).unwrap();
            #[cfg(test)]
            stop.end_wait(false);
        }
        if s.closed || stop.load(Ordering::Acquire) {
            return Err("transport closed");
        }
        s.items += 1;
        s.bytes += bytes;
        Ok(Permit {
            value: None,
            bytes,
            queue: self.clone(),
            charged: true,
        })
    }
    pub fn enqueue(&self, mut permit: Permit<T>) -> Result<(), &'static str> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Err("transport closed");
        }
        s.queue.push_back((
            permit.value.take().expect("filled reserved operation"),
            permit.bytes,
        ));
        permit.charged = false;
        self.wake.notify_all();
        Ok(())
    }
    pub fn pop(self: &Arc<Self>, wait: bool, stop: &Stop) -> Option<Permit<T>> {
        let mut s = self.state.lock().unwrap();
        loop {
            if s.closed || stop.load(Ordering::Acquire) {
                return None;
            }
            if let Some((value, bytes)) = s.queue.pop_front() {
                return Some(Permit {
                    value: Some(value),
                    bytes,
                    queue: self.clone(),
                    charged: true,
                });
            }
            if !wait {
                return None;
            }
            #[cfg(test)]
            stop.begin_wait();
            s = self.wake.wait(s).unwrap();
            #[cfg(test)]
            stop.end_wait(false);
        }
    }
    pub fn close(&self) {
        let mut s = self.state.lock().unwrap();
        s.closed = true;
        while let Some((_, bytes)) = s.queue.pop_front() {
            s.items -= 1;
            s.bytes -= bytes;
        }
        self.wake.notify_all();
    }
    #[cfg(test)]
    pub fn queued_items(&self) -> usize {
        self.state.lock().unwrap().queue.len()
    }
    #[cfg(test)]
    pub fn usage(&self) -> (usize, usize) {
        let s = self.state.lock().unwrap();
        (s.items, s.bytes)
    }
}
impl<T: Send> StopTarget for Queue<T> {
    fn shutdown(&self) {
        self.close();
    }
}
pub(crate) struct Permit<T> {
    pub value: Option<T>,
    bytes: usize,
    queue: Arc<Queue<T>>,
    charged: bool,
}
impl<T> Drop for Permit<T> {
    fn drop(&mut self) {
        if !self.charged {
            return;
        }
        let mut s = self.queue.state.lock().unwrap();
        s.items -= 1;
        s.bytes -= self.bytes;
        self.queue.wake.notify_all();
    }
}
pub(crate) fn nonblocking(fd: i32) -> io::Result<()> {
    // SAFETY: caller owns fd; preserve existing status flags (and descriptor CLOEXEC).
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) type Deadline = Arc<Mutex<Option<std::time::Instant>>>;
pub(crate) struct Interruptible<T> {
    inner: T,
    stop: Arc<Stop>,
    wake: Arc<WakeDescriptor>,
    deadline: Option<Deadline>,
}
impl<T: AsRawFd> Interruptible<T> {
    pub fn new(inner: T, stop: Arc<Stop>) -> io::Result<Self> {
        nonblocking(inner.as_raw_fd())?;
        let wake = stop.descriptor()?;
        Ok(Self {
            inner,
            stop,
            wake,
            deadline: None,
        })
    }
    pub fn with_deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = Some(deadline);
        self
    }
    pub fn wait_readable(&self) -> io::Result<()> {
        self.ready(libc::POLLIN)
    }
    fn ready(&self, events: i16) -> io::Result<()> {
        loop {
            if self.stop.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "transport stopped",
                ));
            }
            let deadline = self.deadline.as_ref().and_then(|d| *d.lock().unwrap());
            let timeout = if let Some(at) = deadline {
                let remaining = at.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "LSP frame assembly deadline exceeded",
                    ));
                }
                remaining
                    .as_millis()
                    .saturating_add(1)
                    .min(i32::MAX as u128) as i32
            } else {
                -1
            };
            let mut descriptors = [
                libc::pollfd {
                    fd: self.inner.as_raw_fd(),
                    events,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake.reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            #[cfg(test)]
            self.stop.begin_wait();
            // SAFETY: two live locally owned descriptors and writable pollfds.
            // Normal idle waits indefinitely; only an active frame has a timer.
            let n = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, timeout) };
            #[cfg(test)]
            self.stop.end_wait(n == 0);
            if n > 0 {
                if descriptors[1].revents != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "transport stopped",
                    ));
                }
                return Ok(());
            }
            if n < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error());
            }
        }
    }
}
impl<T: AsRawFd + Read> Read for Interruptible<T> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        loop {
            self.ready(libc::POLLIN)?;
            match self.inner.read(b) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                r => return r,
            }
        }
    }
}
impl<T: AsRawFd + Write> Write for Interruptible<T> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        loop {
            self.ready(libc::POLLOUT)?;
            match self.inner.write(b) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                r => return r,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Completion {
    done: Arc<AtomicBool>,
    closing: AtomicBool,
    signal: tokio::sync::Notify,
}
static OWNERS: std::sync::OnceLock<Mutex<Vec<std::sync::Weak<Completion>>>> =
    std::sync::OnceLock::new();
fn owners() -> &'static Mutex<Vec<std::sync::Weak<Completion>>> {
    OWNERS.get_or_init(Default::default)
}
/// Snapshot only already-stopping owners; other Roots collect their own on quit.
pub(crate) fn finish_closing() -> impl std::future::Future<Output = ()> {
    let pending: Vec<_> = owners()
        .lock()
        .unwrap()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .filter(|o| o.closing.load(Ordering::Acquire))
        .collect();
    async move {
        for o in pending {
            loop {
                let mut wake = Box::pin(o.signal.notified());
                wake.as_mut().enable();
                if o.done.load(Ordering::Acquire) {
                    break;
                }
                wake.await;
            }
        }
    }
}
/// Started before worker construction; owns cleanup and joins away from GPUI.
pub(crate) struct Workers {
    pub stop: Arc<Stop>,
    pub handles: Vec<JoinHandle<()>>,
    reaper: Option<std::sync::mpsc::Sender<Vec<JoinHandle<()>>>>,
    completion: Arc<Completion>,
    #[cfg(test)]
    pub finished: Arc<AtomicBool>,
}
impl Workers {
    pub fn new(cleanup: impl FnOnce() + Send + 'static) -> io::Result<Self> {
        let (stop, (send, receive)) = (
            Stop::new()?,
            std::sync::mpsc::channel::<Vec<JoinHandle<()>>>(),
        );
        let finished = Arc::new(AtomicBool::new(false));
        let completion = Arc::new(Completion {
            done: finished.clone(),
            closing: AtomicBool::new(false),
            signal: tokio::sync::Notify::new(),
        });
        let done = completion.clone();
        std::thread::Builder::new()
            .name("transport-reap".into())
            .spawn(move || {
                let handles = receive.recv().unwrap_or_default();
                cleanup();
                for h in handles {
                    let _ = h.join();
                }
                done.done.store(true, Ordering::Release);
                done.signal.notify_waiters();
                owners()
                    .lock()
                    .unwrap()
                    .retain(|o| o.upgrade().is_some_and(|o| !o.done.load(Ordering::Acquire)));
            })?;
        owners().lock().unwrap().push(Arc::downgrade(&completion));
        Ok(Self {
            stop,
            handles: Vec::new(),
            reaper: Some(send),
            completion,
            #[cfg(test)]
            finished,
        })
    }
    /// Hand completed process workers to the reaper without interrupting the
    /// reader's trailing output. The caller retains Stop for close/quit.
    pub fn drain(mut self) {
        self.release();
    }
    fn release(&mut self) {
        self.completion.closing.store(true, Ordering::Release);
        if let Some(reaper) = self.reaper.take() {
            let _ = reaper.send(std::mem::take(&mut self.handles));
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        if self.reaper.is_some() {
            self.stop.store(true, Ordering::Release);
            self.release();
        }
    }
}
