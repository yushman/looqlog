//! The sources this process shows (ADR-0010, ADR-0011, `source-registry` spec): a
//! registry of `Session`s, each with a random id, an owner and a share of one memory
//! budget. Lock order is always registry, then source: readers hold a source lock only
//! while feeding one chunk, and never take the registry lock while holding it.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

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

/// Process-wide budget over `Session::retained_bytes()` of every source (ADR-0011).
pub const BUDGET_BYTES: usize = 4 << 30;

/// An upload with no tab connected for this long is unloaded (ADR-0011).
pub const ORPHAN_GRACE: Duration = Duration::from_secs(10 * 60);

/// How often orphaned uploads are looked for.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

pub type SourceId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Argv,
    Stdin,
    Upload,
}

/// Why a source is gone, as its tabs are told (`query-api` spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseReason {
    Unknown,
    Closed,
    Unloaded,
    Budget,
}

pub struct Source {
    pub kind: Kind,
    pub session: Session,
    pub name: String,
    /// Size of the file being read, when known.
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub warning: Option<String>,
    /// stdin reached EOF.
    pub ended: bool,
    /// When stdin delivered its first bytes (the detection hold's clock).
    pub first_data_at: Option<Instant>,
    /// Set once the source has left the registry; readers stop and tabs are told.
    pub closed: Option<CloseReason>,
}

pub type Shared = Arc<Mutex<Source>>;

/// What the page shows about the source, beside the session's own stats.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub kind: Kind,
    pub name: String,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub ended: bool,
    pub warn_bytes: u64,
    pub cap_bytes: u64,
}

/// One row of the landing's source list (`source-registry` spec).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRow {
    pub id: SourceId,
    pub name: String,
    pub kind: Kind,
    pub size: u64,
    pub state: &'static str,
    pub tabs: usize,
    pub error: Option<String>,
}

impl Source {
    pub fn new(kind: Kind, name: String, max_lines: Option<usize>) -> Self {
        Self {
            kind,
            session: new_session(max_lines),
            name,
            total_bytes: None,
            error: None,
            warning: None,
            ended: false,
            first_data_at: None,
            closed: None,
        }
    }

    pub fn info(&self) -> SourceInfo {
        SourceInfo {
            kind: self.kind,
            name: self.name.clone(),
            total_bytes: self.total_bytes,
            error: self.error.clone(),
            warning: self.warning.clone(),
            ended: self.ended,
            warn_bytes: WARN_BYTES,
            cap_bytes: CAP_BYTES,
        }
    }

    fn state(&self) -> &'static str {
        if self.error.is_some() {
            "error"
        } else if self.kind == Kind::Stdin {
            if self.ended {
                "ended"
            } else {
                "live"
            }
        } else if self.session.is_finished() {
            "ended"
        } else {
            "parsing"
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

struct Entry {
    shared: Shared,
    kind: Kind,
    retained: usize,
    tabs: usize,
    /// When the last tab left a finished upload; `None` while a tab is connected or the
    /// upload is still arriving.
    orphaned_since: Option<Instant>,
}

pub struct Registry {
    sources: HashMap<SourceId, Entry>,
    order: Vec<SourceId>,
    /// Ids that existed and why they went away, so a reload says "closed", not "unknown".
    gone: HashMap<SourceId, CloseReason>,
    budget: usize,
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_budget(BUDGET_BYTES)
    }
}

impl Registry {
    pub fn with_budget(budget: usize) -> Self {
        Self {
            sources: HashMap::new(),
            order: Vec::new(),
            gone: HashMap::new(),
            budget,
        }
    }

    /// 8 lowercase RFC 4648 base32 characters (40 bits) from the OS RNG, never one this
    /// process has handed out before.
    fn new_id(&self) -> SourceId {
        use rand::RngCore;
        const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
        loop {
            let mut bytes = [0u8; 8];
            rand::rngs::OsRng.fill_bytes(&mut bytes[..5]);
            let bits = u64::from_be_bytes(bytes) >> 24;
            let id: String = (0..8)
                .rev()
                .map(|i| ALPHABET[((bits >> (i * 5)) & 31) as usize] as char)
                .collect();
            if !self.sources.contains_key(&id) && !self.gone.contains_key(&id) {
                return id;
            }
        }
    }

    pub fn add(&mut self, source: Source) -> (SourceId, Shared) {
        let id = self.new_id();
        let kind = source.kind;
        let shared = Arc::new(Mutex::new(source));
        self.sources.insert(
            id.clone(),
            Entry {
                shared: shared.clone(),
                kind,
                retained: 0,
                tabs: 0,
                orphaned_since: None,
            },
        );
        self.order.push(id.clone());
        (id, shared)
    }

    pub fn get(&self, id: &str) -> Option<Shared> {
        self.sources.get(id).map(|e| e.shared.clone())
    }

    pub fn total_retained(&self) -> usize {
        self.sources.values().map(|e| e.retained).sum()
    }

    /// A tab connected to `id`: its source, or why there is none.
    pub fn attach(&mut self, id: &str) -> Result<Shared, CloseReason> {
        match self.sources.get_mut(id) {
            Some(entry) => {
                entry.tabs += 1;
                entry.orphaned_since = None;
                Ok(entry.shared.clone())
            }
            None => Err(self.gone.get(id).copied().unwrap_or(CloseReason::Unknown)),
        }
    }

    pub fn detach(&mut self, id: &str, now: Instant) {
        if let Some(entry) = self.sources.get_mut(id) {
            entry.tabs = entry.tabs.saturating_sub(1);
            if entry.tabs == 0 && entry.kind == Kind::Upload {
                entry.orphaned_since = Some(now);
            }
        }
    }

    /// An upload finished arriving; with no tab yet, its grace starts now.
    pub fn upload_done(&mut self, id: &str, now: Instant) {
        if let Some(entry) = self.sources.get_mut(id) {
            if entry.tabs == 0 {
                entry.orphaned_since = Some(now);
            }
        }
    }

    /// Records `id`'s retained bytes without enforcing the budget (stdin).
    pub fn record(&mut self, id: &str, retained: usize) {
        if let Some(entry) = self.sources.get_mut(id) {
            entry.retained = retained;
        }
    }

    /// Records `id`'s retained bytes and enforces the budget: tabless uploads are
    /// unloaded oldest orphan first; if the total still does not fit, `Err` names it.
    pub fn charge(&mut self, id: &str, retained: usize) -> Result<(), String> {
        self.record(id, retained);
        while self.total_retained() > self.budget {
            let victim = self
                .sources
                .iter()
                .filter(|(other, e)| other.as_str() != id && e.kind == Kind::Upload)
                .filter_map(|(other, e)| e.orphaned_since.map(|t| (t, other.clone())))
                .min();
            match victim {
                Some((_, victim)) => {
                    self.remove(&victim, CloseReason::Budget);
                }
                None => return Err(budget_error(self.budget)),
            }
        }
        Ok(())
    }

    /// Takes `id` out of the registry, tells its readers and tabs why, and releases its
    /// entries now rather than when the last tab notices.
    pub fn remove(&mut self, id: &str, reason: CloseReason) -> bool {
        let Some(entry) = self.sources.remove(id) else {
            return false;
        };
        self.order.retain(|o| o != id);
        self.gone.insert(id.to_string(), reason);
        let mut src = lock(&entry.shared);
        src.closed = Some(reason);
        src.session = new_session(None);
        true
    }

    /// A user's close request. The stdin source is refused: stopping reading would block
    /// or kill the producer.
    pub fn close(&mut self, id: &str) -> Result<(), CloseError> {
        match self.sources.get(id).map(|e| e.kind) {
            None => Err(CloseError::Unknown),
            Some(Kind::Stdin) => Err(CloseError::Stdin),
            Some(_) => {
                self.remove(id, CloseReason::Closed);
                Ok(())
            }
        }
    }

    /// Unloads uploads orphaned for at least [`ORPHAN_GRACE`].
    pub fn sweep(&mut self, now: Instant) -> usize {
        let expired: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, e)| {
                e.kind == Kind::Upload
                    && e.orphaned_since
                        .is_some_and(|t| now.saturating_duration_since(t) >= ORPHAN_GRACE)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            self.remove(id, CloseReason::Unloaded);
        }
        expired.len()
    }

    pub fn rows(&self) -> Vec<SourceRow> {
        self.order
            .iter()
            .filter_map(|id| {
                let entry = self.sources.get(id)?;
                let src = lock(&entry.shared);
                Some(SourceRow {
                    id: id.clone(),
                    name: src.name.clone(),
                    kind: src.kind,
                    size: src.total_bytes.unwrap_or(src.session.bytes_ingested()),
                    state: src.state(),
                    tabs: entry.tabs,
                    error: src.error.clone(),
                })
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloseError {
    Unknown,
    Stdin,
}

impl CloseError {
    pub fn message(&self) -> &'static str {
        match self {
            CloseError::Unknown => "no such source",
            CloseError::Stdin => {
                "stdin cannot be closed while it is being read: stopping would \
                 block or kill the program writing to it"
            }
        }
    }
}

pub fn budget_error(budget: usize) -> String {
    format!(
        "the looqlog memory budget of {} is full: close another source or open a smaller file",
        format_bytes(budget as u64)
    )
}

pub fn format_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB)
    } else if bytes as f64 >= MIB {
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

/// Adds the argv file as a source and starts reading it on a blocking thread. Returns
/// the id and what to print on the terminal: `Err` when the file cannot be read at all
/// (the server still runs and the source shows the same error), `Ok(Some(warning))`
/// above the warning size.
pub fn open_argv(
    registry: &Arc<Mutex<Registry>>,
    path: &Path,
) -> (SourceId, Result<Option<String>, String>) {
    let name = path.display().to_string();
    let (id, shared) = registry
        .lock()
        .expect("registry lock poisoned")
        .add(Source::new(Kind::Argv, name.clone(), None));
    let fail = |message: String| {
        lock(&shared).error = Some(message.clone());
        Err(message)
    };
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => return (id, fail(format!("could not open {name}: {err}"))),
    };
    let size = match file.metadata() {
        Ok(meta) if meta.is_dir() => {
            return (id, fail(format!("{name} is a directory, not a log file")))
        }
        Ok(meta) => meta.len(),
        Err(err) => return (id, fail(format!("could not read {name}: {err}"))),
    };
    if size > CAP_BYTES {
        return (id, fail(cap_error(&name, size)));
    }
    let warning = size_warning(size);
    {
        let mut src = lock(&shared);
        src.total_bytes = Some(size);
        src.warning = warning.clone();
    }
    let registry = registry.clone();
    let reader_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let read = file.read(&mut buf);
            let retained = {
                let mut src = lock(&shared);
                if src.closed.is_some() {
                    return;
                }
                match read {
                    Ok(0) => {
                        src.session.finish();
                        src.session.retained_bytes()
                    }
                    Ok(n) => {
                        src.session.ingest(&buf[..n]);
                        src.session.retained_bytes()
                    }
                    Err(err) => {
                        tracing::error!("reading {name} failed: {err}");
                        src.error = Some(format!("reading {name} failed: {err}"));
                        src.session.finish();
                        src.session.retained_bytes()
                    }
                }
            };
            let charged = registry
                .lock()
                .expect("registry lock poisoned")
                .charge(&reader_id, retained);
            let mut src = lock(&shared);
            if let Err(message) = charged {
                tracing::error!("{name}: {message}");
                src.error = Some(message);
                src.session.finish();
                return;
            }
            if src.session.is_finished() {
                return;
            }
        }
    });
    (id, Ok(warning))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(reg: &mut Registry) -> SourceId {
        reg.add(Source::new(Kind::Upload, "u".into(), None)).0
    }

    #[test]
    fn ids_are_distinct_base32_and_not_sequential() {
        let mut reg = Registry::default();
        let ids: Vec<SourceId> = (0..200).map(|_| upload(&mut reg)).collect();
        let distinct: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(distinct.len(), ids.len());
        assert!(ids.iter().all(|id| id.len() == 8
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_ne!(sorted, ids, "ids sort in creation order");
    }

    #[test]
    fn a_removed_id_reports_its_reason_and_is_never_reused() {
        let mut reg = Registry::default();
        let id = upload(&mut reg);
        reg.close(&id).unwrap();
        assert_eq!(reg.attach(&id).err(), Some(CloseReason::Closed));
        assert_eq!(reg.attach("zzzzzzzz").err(), Some(CloseReason::Unknown));
        assert!(reg.new_id() != id);
    }

    #[test]
    fn gc_respects_a_reconnect_within_the_grace() {
        let mut reg = Registry::default();
        let id = upload(&mut reg);
        let t0 = Instant::now();
        reg.upload_done(&id, t0);
        reg.attach(&id).unwrap();
        reg.detach(&id, t0);
        reg.attach(&id).unwrap();
        reg.detach(&id, t0 + Duration::from_secs(9 * 60));
        assert_eq!(reg.sweep(t0 + ORPHAN_GRACE + Duration::from_secs(1)), 0);
        assert!(reg.get(&id).is_some());
        assert_eq!(
            reg.sweep(t0 + Duration::from_secs(9 * 60) + ORPHAN_GRACE),
            1
        );
        assert_eq!(reg.attach(&id).err(), Some(CloseReason::Unloaded));
    }

    #[test]
    fn an_upload_with_a_tab_is_never_swept() {
        let mut reg = Registry::default();
        let id = upload(&mut reg);
        let t0 = Instant::now();
        reg.upload_done(&id, t0);
        reg.attach(&id).unwrap();
        assert_eq!(reg.sweep(t0 + ORPHAN_GRACE * 10), 0);
    }

    #[test]
    fn budget_evicts_the_oldest_orphan_first() {
        let mut reg = Registry::with_budget(100);
        let t0 = Instant::now();
        let (old, young, held) = (upload(&mut reg), upload(&mut reg), upload(&mut reg));
        for id in [&old, &young, &held] {
            reg.charge(id, 30).unwrap();
        }
        reg.upload_done(&young, t0 + Duration::from_secs(5));
        reg.upload_done(&old, t0);
        reg.attach(&held).unwrap();
        let new = upload(&mut reg);
        reg.charge(&new, 20).unwrap();
        assert!(reg.get(&old).is_none(), "the oldest orphan goes first");
        assert!(reg.get(&young).is_some());
        assert_eq!(reg.attach(&old).err(), Some(CloseReason::Budget));
        reg.charge(&new, 50).unwrap();
        assert!(reg.get(&young).is_none());
        assert!(reg.charge(&new, 80).is_err());
        assert!(
            reg.get(&held).is_some(),
            "a tab-held upload is never evicted"
        );
    }

    #[test]
    fn process_owned_sources_are_never_evicted_or_swept() {
        let mut reg = Registry::with_budget(100);
        let (argv, _) = reg.add(Source::new(Kind::Argv, "a.log".into(), None));
        let (stdin, _) = reg.add(Source::new(Kind::Stdin, "stdin".into(), Some(10)));
        reg.charge(&argv, 50).unwrap();
        reg.record(&stdin, 40);
        let now = Instant::now();
        reg.detach(&argv, now);
        reg.detach(&stdin, now);
        let new = upload(&mut reg);
        assert!(reg.charge(&new, 20).is_err());
        assert_eq!(reg.sweep(now + ORPHAN_GRACE * 10), 0);
        assert!(reg.get(&argv).is_some() && reg.get(&stdin).is_some());
    }

    #[test]
    fn stdin_close_is_refused_and_argv_close_is_not() {
        let mut reg = Registry::default();
        let (argv, shared) = reg.add(Source::new(Kind::Argv, "a.log".into(), None));
        let (stdin, _) = reg.add(Source::new(Kind::Stdin, "stdin".into(), Some(10)));
        assert_eq!(reg.close(&stdin), Err(CloseError::Stdin));
        assert_eq!(reg.close(&argv), Ok(()));
        assert_eq!(lock(&shared).closed, Some(CloseReason::Closed));
        assert_eq!(reg.close(&argv), Err(CloseError::Unknown));
        assert_eq!(reg.order, [stdin]);
    }
}
