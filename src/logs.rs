//! In-app log buffer. Captures `tracing` output into a bounded ring so
//! the GUI can display recent log lines without us needing to keep a
//! Windows console open. The `MakeWriter` impl lets us plug straight
//! into `tracing_subscriber::fmt`.

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;

use parking_lot::Mutex;

const CAPACITY: usize = 500;

pub struct LogBuffer {
    lines: VecDeque<String>,
    partial: String,
}

impl LogBuffer {
    fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(CAPACITY),
            partial: String::new(),
        }
    }

    /// Snapshot the current lines for display. Cheap because we only
    /// clone Strings, and the buffer is bounded.
    pub fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }

    fn push_chunk(&mut self, s: &str) {
        for ch in s.chars() {
            if ch == '\n' {
                let line = std::mem::take(&mut self.partial);
                self.lines.push_back(line);
                while self.lines.len() > CAPACITY {
                    self.lines.pop_front();
                }
            } else {
                self.partial.push(ch);
            }
        }
    }
}

#[derive(Clone)]
pub struct SharedLogs(pub Arc<Mutex<LogBuffer>>);

impl SharedLogs {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(LogBuffer::new())))
    }
}

/// `MakeWriter` factory: tracing-subscriber asks for a writer per event,
/// we just hand it another handle to the shared buffer.
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedLogs {
    type Writer = SharedLogs;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl io::Write for SharedLogs {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            self.0.lock().push_chunk(s);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
