//! Bound ZIP metadata before the upstream parser can allocate its directory table.
//! Entry count and variable-size ZIP64 records are checked before header reads;
//! constructor reads also have a finite budget, including corrupt-EOCD fallback.
use std::{
    cell::Cell,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    os::unix::fs::FileExt,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
const MAX_DIRECTORY: u64 = 32 * 1024 * 1024;
pub(super) struct Reader<'a> {
    file: File,
    cancel: &'a AtomicBool,
    pub checking: Rc<Cell<bool>>,
    remaining: u64,
}
impl<'a> Reader<'a> {
    pub fn new(file: File, cancel: &'a AtomicBool) -> Self {
        Self {
            file,
            cancel,
            checking: Rc::new(Cell::new(true)),
            remaining: MAX_DIRECTORY * 4,
        }
    }
    fn before_read(&mut self) -> io::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::other("update cancelled"));
        }
        if !self.checking.get() {
            return Ok(());
        }
        if self.remaining == 0 {
            return Err(io::Error::other("ZIP metadata read limit exceeded"));
        }
        let position = self.file.stream_position()?;
        let mut header = [0u8; 56];
        let n = self.file.read_at(&mut header, position)?;
        if n >= 4 && &header[..4] == b"PK\x06\x06" {
            if n < 56 {
                return Err(io::Error::other("incomplete ZIP64 header"));
            }
            let value = |at| u64::from_le_bytes(header[at..at + 8].try_into().unwrap());
            if value(4) < 44
                || value(4) > 65536
                || value(24) > 100000
                || value(32) > 100000
                || value(40) > MAX_DIRECTORY
            {
                return Err(io::Error::other("ZIP64 metadata exceeds update limits"));
            }
        } else if n >= 4 && &header[..4] == b"PK\x05\x06" {
            if n < 22 {
                return Err(io::Error::other("incomplete ZIP directory header"));
            }
            let size = u32::from_le_bytes(header[12..16].try_into().unwrap());
            // ZIP64's sentinel is followed through its separately bounded record.
            if size != u32::MAX && u64::from(size) > MAX_DIRECTORY {
                return Err(io::Error::other("ZIP directory exceeds update limits"));
            }
        }
        Ok(())
    }
}
impl Read for Reader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.before_read()?;
        let limit = if self.checking.get() {
            buffer.len().min(self.remaining as usize)
        } else {
            buffer.len()
        };
        let n = self.file.read(&mut buffer[..limit])?;
        if self.checking.get() {
            self.remaining -= n as u64;
        }
        Ok(n)
    }
}
impl Seek for Reader<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::other("update cancelled"));
        }
        self.file.seek(from)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zip64_count_and_extension_are_rejected_before_header_bytes_reach_the_parser() {
        for (count, extension) in [(100_001u64, 44u64), (1, 1024 * 1024)] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("header");
            let mut header = [0u8; 56];
            header[..4].copy_from_slice(b"PK\x06\x06");
            header[4..12].copy_from_slice(&extension.to_le_bytes());
            header[24..32].copy_from_slice(&count.to_le_bytes());
            header[32..40].copy_from_slice(&count.to_le_bytes());
            std::fs::write(&path, header).unwrap();
            let cancel = AtomicBool::new(false);
            let mut reader = Reader::new(File::open(path).unwrap(), &cancel);
            let mut bytes = [0; 4];
            assert!(
                reader.read(&mut bytes).is_err(),
                "oversized ZIP64 metadata must fail before upstream allocation"
            );
        }
    }
    #[test]
    fn metadata_budget_is_finite_but_does_not_limit_authenticated_payload_reads() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data");
        std::fs::write(&path, [b'x'; 16]).unwrap();
        let cancel = AtomicBool::new(false);
        let mut reader = Reader::new(File::open(path).unwrap(), &cancel);
        reader.remaining = 8;
        let mut bytes = [0; 16];
        assert_eq!(reader.read(&mut bytes).unwrap(), 8);
        assert!(reader.read(&mut bytes).is_err());
        reader.checking.set(false);
        assert_eq!(reader.read(&mut bytes).unwrap(), 8);
    }
    #[test]
    fn cancellation_is_a_terminal_read_error_for_read_exact_callers() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data");
        std::fs::write(&path, b"data").unwrap();
        let cancel = AtomicBool::new(true);
        let mut reader = Reader::new(File::open(path).unwrap(), &cancel);
        let error = reader.read(&mut [0; 4]).unwrap_err();
        assert_ne!(
            error.kind(),
            io::ErrorKind::Interrupted,
            "read_exact retries Interrupted indefinitely; cancellation must stop the ZIP parser"
        );
    }
}
