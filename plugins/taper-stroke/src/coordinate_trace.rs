//! Opt-in diagnostic build only. No file I/O in the default feature set.
use std::{
    fmt,
    fs::OpenOptions,
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
static RECORDS: AtomicUsize = AtomicUsize::new(0);
static LOCK: Mutex<()> = Mutex::new(());
/// At most 128 geometry-only records per loaded process. Errors never affect rendering.
pub fn record(message: fmt::Arguments<'_>) {
    let index = RECORDS.fetch_add(1, Ordering::Relaxed);
    if index >= 128 {
        return;
    }
    let Ok(_guard) = LOCK.lock() else {
        return;
    };
    let path = std::env::temp_dir().join(format!(
        "ONMK_TaperStroke-coordinates-{}.log",
        std::process::id()
    ));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "v0.3.1 coordinate-trace {index}: {message}");
    }
}
