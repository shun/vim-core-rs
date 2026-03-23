use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

enum DebugLogSink {
    Stderr,
    File(File),
}

pub(crate) struct DebugLogConfig {
    pub(crate) path: Option<PathBuf>,
}

impl Default for DebugLogConfig {
    fn default() -> Self {
        Self { path: None }
    }
}

struct DebugLogState {
    sink: Mutex<DebugLogSink>,
}

static DEBUG_LOG_STATE: OnceLock<DebugLogState> = OnceLock::new();

fn state() -> &'static DebugLogState {
    DEBUG_LOG_STATE.get_or_init(|| DebugLogState {
        sink: Mutex::new(DebugLogSink::Stderr),
    })
}

pub(crate) fn configure(config: &DebugLogConfig) -> io::Result<()> {
    let next_sink = match config.path.as_deref() {
        Some(path) => DebugLogSink::File(open_log_file(path)?),
        None => DebugLogSink::Stderr,
    };

    let mut sink = state()
        .sink
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *sink = next_sink;
    Ok(())
}

pub(crate) fn emit(args: fmt::Arguments<'_>) {
    let mut sink = state()
        .sink
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match &mut *sink {
        DebugLogSink::Stderr => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{args}");
            let _ = stderr.flush();
        }
        DebugLogSink::File(file) => {
            let _ = writeln!(file, "{args}");
            let _ = file.flush();
        }
    }
}

fn open_log_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}
