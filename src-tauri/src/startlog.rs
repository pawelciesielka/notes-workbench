//! Dziennik czasów startu: `%LOCALAPPDATA%\eu.ciesielka.notesworkbench\start.log`.
//! Każde uruchomienie dopisuje nagłówek (data, czas od włączenia systemu) i kroki z czasem od startu procesu.
//! Zapisujemy tylko pierwsze `WINDOW` sekund, żeby zwykła praca nie zaśmiecała pliku.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(120);
/// Po przekroczeniu rozmiaru zostawiamy tylko końcówkę pliku (ostatnie uruchomienia).
const MAX_BYTES: u64 = 256 * 1024;
const KEEP_BYTES: usize = 128 * 1024;

static START: OnceLock<Instant> = OnceLock::new();
/// Ścieżka pliku; dopóki jej nie znamy (przed `setup`), wpisy czekają w buforze.
static FILE: Mutex<(Option<PathBuf>, Vec<String>)> = Mutex::new((None, Vec::new()));

#[cfg(windows)]
fn uptime() -> Option<Duration> {
    extern "system" {
        fn GetTickCount64() -> u64;
    }
    Some(Duration::from_millis(unsafe { GetTickCount64() }))
}

#[cfg(not(windows))]
fn uptime() -> Option<Duration> {
    None
}

/// Wywoływane na samym początku `run()`.
pub fn begin() {
    START.get_or_init(Instant::now);
    let up = uptime().map_or(String::new(), |u| {
        let s = u.as_secs();
        format!(", system działa od {}h {:02}min", s / 3600, s % 3600 / 60)
    });
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    write(format!("\n=== start {now}{up}, wersja {}", env!("CARGO_PKG_VERSION")));
}

/// Wskazuje katalog dziennika i zapisuje wszystko, co zebrało się wcześniej.
pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("start.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        if let Ok(old) = fs::read(&path) {
            let tail = &old[old.len() - KEEP_BYTES..];
            let cut = tail.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
            let _ = fs::write(&path, &tail[cut..]);
        }
    }
    let pending = {
        let mut g = FILE.lock().unwrap();
        g.0 = Some(path);
        std::mem::take(&mut g.1)
    };
    for line in pending {
        write(line);
    }
}

fn elapsed() -> Option<Duration> {
    START.get().map(|s| s.elapsed()).filter(|e| *e < WINDOW)
}

/// Krok startu z czasem od uruchomienia procesu.
pub fn mark(msg: &str) {
    if let Some(e) = elapsed() {
        write(format!("{:>8} ms  {msg}", e.as_millis()));
    }
}

/// Mierzy czas `f` i zapisuje go razem z momentem zakończenia.
pub fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    if elapsed().is_none() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    mark(&format!("{label} ({} ms)", t.elapsed().as_millis()));
    r
}

fn write(line: String) {
    let mut g = FILE.lock().unwrap();
    match &g.0 {
        Some(p) => {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
            }
        }
        None => g.1.push(line),
    }
}
