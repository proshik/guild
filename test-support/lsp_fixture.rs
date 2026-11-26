//! A fake server's lifetime observed through EOF, with bounded failure cleanup.
use std::{
    io::{ErrorKind, Read},
    os::unix::net::{UnixListener, UnixStream},
    time::{Duration, Instant},
};

pub struct LspFixture {
    _root: tempfile::TempDir,
    listener: UnixListener,
    socket: std::path::PathBuf,
    stream: Option<UnixStream>,
    pub record_documents: bool,
}
impl LspFixture {
    pub fn new() -> Self {
        // Darwin's Unix socket paths are short; /tmp also keeps CI paths bounded.
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let socket = root.path().join("lsp.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        Self {
            _root: root,
            listener,
            socket,
            stream: None,
            record_documents: false,
        }
    }
    pub fn command(&self, descendant: bool) -> (String, Vec<String>) {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-lsp-lifetime.py");
        let mut args = vec![
            script.to_string_lossy().into_owned(),
            self.socket.to_string_lossy().into_owned(),
        ];
        if self.record_documents {
            args.push("--document-events".into());
        }
        if descendant {
            args.push("--descendant".into());
        }
        ("python3".into(), args)
    }
    pub fn ready(&mut self) {
        let until = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    assert!(Instant::now() < until, "fake server never connected");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("fake lifetime accept: {err}"),
            }
        };
        // Keep failure cleanup even if readiness itself fails.
        self.stream = Some(stream.try_clone().unwrap());
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut ready = [0; 5];
        stream.read_exact(&mut ready).unwrap();
        assert_eq!(&ready, b"ready");
        stream.set_nonblocking(true).unwrap();
        self.stream = Some(stream);
    }
    pub fn assert_stopped(&mut self) {
        let until = Instant::now() + Duration::from_secs(10);
        let stream = self
            .stream
            .as_mut()
            .expect("observe readiness before cleanup");
        loop {
            match stream.read(&mut [0]) {
                Ok(0) => return,
                Ok(_) => panic!("unexpected data on lifetime socket"),
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < until,
                        "owned LSP or descendant survived cleanup"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("fake lifetime read: {err}"),
            }
        }
    }
}
