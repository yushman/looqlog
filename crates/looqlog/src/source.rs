//! The one log source this process shows (ADR-0010, `file-ingest` spec): its
//! `Session`, what it is, and whether reading it failed. Readers hold the lock only
//! while feeding one chunk, so queries interleave with ingestion.

use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use looqlog_core::{Session, SessionConfig};
use serde::Serialize;

/// Read size for the argv file (design.md D4).
pub const CHUNK_BYTES: usize = 64 * 1024;

/// Above this a file is still read, with a warning. The measured worst case is 10.24 bytes
/// of retained heap per input byte (logfmt; `mem_probe -- session`, docs/devlog.md), so
/// 50 MiB means roughly half a gigabyte.
pub const WARN_BYTES: u64 = 50 * 1024 * 1024;

/// Above this a file is refused: ~4.1 GB of retained heap at the measured worst case.
pub const CAP_BYTES: u64 = 400 * 1024 * 1024;

pub struct Source {
    /// Bumped whenever the source is replaced; connections reset their view on change.
    pub id: u64,
    pub session: Session,
    pub stdin: bool,
    pub name: Option<String>,
    /// Size of the file being read, when known.
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub warning: Option<String>,
    /// stdin reached EOF.
    pub ended: bool,
    /// When stdin delivered its first bytes (the detection hold's clock).
    pub first_data_at: Option<Instant>,
}

pub type Shared = Arc<Mutex<Source>>;

/// What the page shows about the source, beside the session's own stats.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub kind: &'static str,
    pub name: Option<String>,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub ended: bool,
    pub warn_bytes: u64,
    pub cap_bytes: u64,
}

impl Source {
    pub fn new(stdin: bool, max_lines: Option<usize>) -> Self {
        Self {
            id: 0,
            session: new_session(max_lines),
            stdin,
            name: None,
            total_bytes: None,
            error: None,
            warning: None,
            ended: false,
            first_data_at: None,
        }
    }

    /// Replaces the current file source with a fresh, empty session.
    pub fn replace(&mut self, name: Option<String>, total_bytes: Option<u64>) -> u64 {
        self.id += 1;
        self.session = new_session(None);
        self.name = name;
        self.total_bytes = total_bytes;
        self.error = None;
        self.warning = total_bytes.and_then(size_warning);
        self.id
    }

    pub fn info(&self) -> SourceInfo {
        SourceInfo {
            kind: if self.stdin { "stdin" } else { "file" },
            name: self.name.clone(),
            total_bytes: self.total_bytes,
            error: self.error.clone(),
            warning: self.warning.clone(),
            ended: self.ended,
            warn_bytes: WARN_BYTES,
            cap_bytes: CAP_BYTES,
        }
    }
}

pub fn new_session(max_lines: Option<usize>) -> Session {
    Session::new(SessionConfig {
        max_lines,
        reference: Some(chrono::Utc::now()),
    })
}

pub fn lock(shared: &Shared) -> MutexGuard<'_, Source> {
    shared.lock().expect("source lock poisoned")
}

pub fn format_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB)
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

pub fn size_warning(size: u64) -> Option<String> {
    (size > WARN_BYTES).then(|| {
        format!(
            "this file is {}, above the {} warning threshold — parsing it takes a while and \
             the looqlog process may use several times that in memory",
            format_bytes(size),
            format_bytes(WARN_BYTES)
        )
    })
}

pub fn cap_error(name: &str, size: u64) -> String {
    format!(
        "{name} is {}, above the {} limit: the looqlog process cannot hold the index for a \
         file that size (measured at up to ~10x the file size in memory). Split the file \
         (e.g. by time range) or filter it down, then open it again.",
        format_bytes(size),
        format_bytes(CAP_BYTES)
    )
}

/// Starts reading the argv file into the source on a blocking thread. Returns what to
/// print on the terminal: `Err` when the file cannot be read at all (the server still
/// runs and the page shows the same error), `Ok(Some(warning))` above the warning size.
pub fn open_argv(shared: &Shared, path: &Path) -> Result<Option<String>, String> {
    let name = path.display().to_string();
    let fail = |message: String| {
        let mut src = lock(shared);
        src.name = Some(name.clone());
        src.error = Some(message.clone());
        Err(message)
    };
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => return fail(format!("could not open {name}: {err}")),
    };
    let size = match file.metadata() {
        Ok(meta) if meta.is_dir() => return fail(format!("{name} is a directory, not a log file")),
        Ok(meta) => meta.len(),
        Err(err) => return fail(format!("could not read {name}: {err}")),
    };
    if size > CAP_BYTES {
        return fail(cap_error(&name, size));
    }
    let my_id = {
        let mut src = lock(shared);
        src.replace(Some(name.clone()), Some(size))
    };
    let warning = size_warning(size);
    let shared = shared.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let read = file.read(&mut buf);
            let mut src = lock(&shared);
            if src.id != my_id {
                return; // replaced by an upload
            }
            match read {
                Ok(0) => {
                    src.session.finish();
                    return;
                }
                Ok(n) => src.session.ingest(&buf[..n]),
                Err(err) => {
                    tracing::error!("reading {name} failed: {err}");
                    src.error = Some(format!("reading {name} failed: {err}"));
                    src.session.finish();
                    return;
                }
            }
        }
    });
    Ok(warning)
}
