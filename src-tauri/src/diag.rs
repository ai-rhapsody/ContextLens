//! A small log kept on this computer only, so a problem report can say what happened: where
//! the selection was found, what a service answered, how Live connected. It never holds the
//! text being translated, the answers or the API keys, and nothing sends it anywhere.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

/// Past this size the log moves to `contextlens.old.log` and starts again.
const MAX_BYTES: u64 = 512 * 1024;

static FILE: OnceLock<PathBuf> = OnceLock::new();
static WRITE: Mutex<()> = Mutex::new(());

/// Where the log lives; call once at start-up.
pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = FILE.set(dir.join("contextlens.log"));
    log(&format!("start: ContextLens {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
}

pub fn path() -> Option<&'static PathBuf> {
    FILE.get()
}

/// Appends one line, with the time in UTC. Failing to write is ignored: the log must never
/// get in the way of the app.
pub fn log(line: &str) {
    let Some(path) = FILE.get() else { return };
    let _guard = WRITE.lock();
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {}", utc_now(), line.replace('\n', " "));
    }
}

/// The last `n` lines of the log.
pub fn tail(n: usize) -> Vec<String> {
    let Some(text) = FILE.get().and_then(|p| fs::read_to_string(p).ok()) else { return vec![] };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].iter().map(|l| l.to_string()).collect()
}

fn utc_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format_utc(secs)
}

/// `YYYY-MM-DD HH:MM:SSZ` for seconds since 1970 (Howard Hinnant's civil-from-days).
fn format_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_times_are_formatted() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00Z");
        assert_eq!(format_utc(1_793_259_000), "2026-10-29 07:30:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00:00Z");
    }
}
