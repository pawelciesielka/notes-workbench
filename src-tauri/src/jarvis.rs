//! Jarvis: lokalny tracker aktywności i zadań (`jarvis.py`, Python). Zob. docs/jarvis.md i ADR 0003.
//!
//! Podział pracy: wszystko, co Jarvis liczy (czas, kategorie, dopasowanie okien do zadań, zdarzenia strażnika),
//! bierzemy z `jarvis.py status --json`; fokus, start/stop i autostart idą przez jego CLI. Rejestr zadań
//! (`zadania.md`) i wybrane pola `config.json` zmieniamy tutaj, bo CLI nie ma do tego komend.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

pub const DEFAULT_DIR: &str = r"C:\WORK\jarvis";

// ===== Ścieżki =====

/// Katalog Jarvisa: z ustawień albo domyślny, o ile jest w nim `jarvis.py`.
pub fn resolve_dir(configured: Option<&str>) -> Option<PathBuf> {
    let p = PathBuf::from(configured.unwrap_or(DEFAULT_DIR));
    p.join("jarvis.py").is_file().then_some(p)
}

/// Prawdziwy interpreter Pythona. `python` z PATH bywa shimem pyenv (`python.bat`), który otwiera konsolę
/// i nie zawsze działa z `Command`, dlatego szukamy `python.exe`: najpierw w pyenv-win (to wybór użytkownika,
/// najnowsza wersja), potem w instalacjach z python.org, na końcu w PATH (z pominięciem shimów).
pub fn detect_python() -> Option<PathBuf> {
    let newest = |dir: PathBuf| -> Option<PathBuf> {
        let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let exe = e.path().join("python.exe");
            if exe.is_file() {
                let name = e.file_name().to_string_lossy().to_string();
                let ver: Vec<u32> = name
                    .trim_start_matches(|c: char| !c.is_ascii_digit())
                    .split(|c: char| !c.is_ascii_digit())
                    .filter_map(|x| x.parse().ok())
                    .collect();
                found.push((ver, exe));
            }
        }
        found.sort();
        found.pop().map(|(_, p)| p)
    };
    let pyenv = std::env::var_os("USERPROFILE").and_then(|h| newest(PathBuf::from(h).join(r".pyenv\pyenv-win\versions")));
    if pyenv.is_some() {
        return pyenv;
    }
    let org = std::env::var_os("LOCALAPPDATA").and_then(|l| newest(PathBuf::from(l).join(r"Programs\Python")));
    if org.is_some() {
        return org;
    }
    let out = hidden(Command::new("where")).arg("python.exe").output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .find(|p| p.is_file() && !p.to_string_lossy().to_lowercase().contains("shims") && !p.to_string_lossy().contains("WindowsApps"))
}

pub fn resolve_python(configured: Option<&str>) -> Option<PathBuf> {
    match configured {
        Some(p) if Path::new(p).is_file() => Some(PathBuf::from(p)),
        _ => detect_python(),
    }
}

fn hidden(mut c: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Uruchamia `python jarvis.py <args>` bez okna, w katalogu Jarvisa; zwraca stdout.
pub fn run(python: &Path, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = hidden(Command::new(python))
        .arg(dir.join("jarvis.py"))
        .args(args)
        .current_dir(dir)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .output()
        .map_err(|e| format!("Nie udało się uruchomić Pythona ({}): {e}", python.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        Err(format!("jarvis {}: {}", args.join(" "), if last.is_empty() { stdout.trim() } else { last }))
    }
}

/// Szybkie sprawdzenie bez Pythona: PID z `jarvis.pid` należy do działającego procesu Pythona.
pub fn tracker_running(dir: &Path) -> bool {
    let Some(pid) = fs::read_to_string(dir.join("jarvis.pid")).ok().and_then(|s| s.trim().parse::<u32>().ok()) else {
        return false;
    };
    hidden(Command::new("tasklist"))
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("python"))
}

/// Autostart Jarvisa: skrót `Jarvis.lnk` w folderze Autostart użytkownika (zakłada go `jarvis install`).
pub fn autostart_enabled() -> bool {
    std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\Startup\Jarvis.lnk"))
        .is_some_and(|p| p.exists())
}

// ===== Rejestr zadań =====

static TASK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[-*]\s*\[( |x|X)\]\s*(.+?)\s*$").unwrap());
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@(\d{4}-\d{2}-\d{2})").unwrap());
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([\w\-.+]+)").unwrap());
static PRIO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)!([1-3])\b").unwrap());
/// Jak `task_name()` w Jarvisie: nazwa bez `@data`, `#tagów` i `!priorytetu`.
static STRIP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*(?:[@#][\w\-.+]+|![1-3]\b)").unwrap());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    /// Pełna linia z pliku: po niej wskazujemy zadanie przy zmianie (numer linii mógł się przesunąć).
    pub raw: String,
    pub done: bool,
    pub name: String,
    /// `None`: bez `!N` w linii (Jarvis traktuje to jak 3).
    pub prio: Option<u8>,
    /// RRRR-MM-DD albo `None` (zadanie codzienne, aż do zrobienia).
    pub date: Option<String>,
    pub tags: Vec<String>,
}

pub fn parse_line(line: &str) -> Option<Task> {
    let c = TASK_RE.captures(line)?;
    let text = c.get(2).unwrap().as_str();
    Some(Task {
        raw: line.to_string(),
        done: c.get(1).unwrap().as_str() != " ",
        name: STRIP_RE.replace_all(text, "").trim().to_string(),
        prio: PRIO_RE.captures(text).and_then(|m| m[1].parse().ok()),
        date: DATE_RE.captures(text).map(|m| m[1].to_string()),
        tags: TAG_RE.captures_iter(text).map(|m| m[1].to_string()).collect(),
    })
}

/// Tekst zadania (bez `- [ ]`) w formacie Jarvisa: `Nazwa !1 @2026-11-20 #tag`.
pub fn task_text(name: &str, prio: Option<u8>, date: Option<&str>, tags: &[String]) -> String {
    let mut s = name.trim().to_string();
    if let Some(p) = prio {
        s += &format!(" !{p}");
    }
    if let Some(d) = date {
        s += &format!(" @{d}");
    }
    for t in tags {
        let t = t.trim().trim_start_matches('#').replace(' ', "_");
        if !t.is_empty() {
            s += &format!(" #{t}");
        }
    }
    s
}

pub fn line_of(t: &Task) -> String {
    format!("- [{}] {}", if t.done { "x" } else { " " }, task_text(&t.name, t.prio, t.date.as_deref(), &t.tags))
}

pub fn parse_tasks(body: &str) -> Vec<Task> {
    body.lines().filter_map(parse_line).collect()
}

/// Treść pliku: bez BOM; zapamiętujemy BOM i końce linii, żeby zapisać plik tak, jak był.
struct FileText {
    bom: bool,
    crlf: bool,
    body: String,
}

fn read_text(p: &Path) -> Result<FileText, String> {
    let s = fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let bom = s.starts_with('\u{feff}');
    let body = s.trim_start_matches('\u{feff}').to_string();
    Ok(FileText { bom, crlf: body.contains("\r\n"), body })
}

fn write_text(p: &Path, f: &FileText) -> Result<(), String> {
    let mut s = String::new();
    if f.bom {
        s.push('\u{feff}');
    }
    s += &if f.crlf { f.body.replace("\r\n", "\n").replace('\n', "\r\n") } else { f.body.clone() };
    fs::write(p, s).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn read_tasks(file: &Path) -> Result<Vec<Task>, String> {
    if !file.exists() {
        return Ok(vec![]);
    }
    Ok(parse_tasks(&read_text(file)?.body))
}

pub const CHANGED: &str = "Rejestr zadań zmienił się w międzyczasie (np. Jarvis dopisał zadania z porannego briefu). Lista została odświeżona, spróbuj jeszcze raz.";

/// Podmienia linię `old_raw` na `new_line` (pierwsze dokładne wystąpienie). Reszta tekstu bez zmian.
pub fn replace_line(body: &str, old_raw: &str, new_line: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut hit = false;
    for l in body.split('\n') {
        let (content, cr) = l.strip_suffix('\r').map_or((l, ""), |c| (c, "\r"));
        if !hit && content == old_raw {
            hit = true;
            out.push(format!("{new_line}{cr}"));
        } else {
            out.push(l.to_string());
        }
    }
    hit.then(|| out.join("\n"))
}

pub fn update_task(file: &Path, old_raw: &str, new: &Task) -> Result<Task, String> {
    let mut f = read_text(file)?;
    let line = if new.name.trim().is_empty() { return Err("Zadanie musi mieć nazwę".into()) } else { line_of(new) };
    f.body = replace_line(&f.body, old_raw, &line).ok_or(CHANGED)?;
    write_text(file, &f)?;
    Ok(parse_line(&line).unwrap())
}

pub const TASKS_HEADER: &str = "# Zadania\n\n<!--\nFormat:  - [ ] Nazwa zadania !1 @RRRR-MM-DD #slowo #klucz\n-->\n\n";

/// Dopisuje zadanie na końcu rejestru (jak `jarvis add`).
pub fn add_task(file: &Path, text: &str) -> Result<Task, String> {
    let line = format!("- [ ] {}", text.trim());
    let t = parse_line(&line).filter(|t| !t.name.is_empty()).ok_or("Zadanie musi mieć nazwę")?;
    let mut f = if file.exists() {
        read_text(file)?
    } else {
        if let Some(d) = file.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        FileText { bom: false, crlf: false, body: TASKS_HEADER.into() }
    };
    if !f.body.is_empty() && !f.body.ends_with('\n') {
        f.body.push('\n');
    }
    f.body += &line;
    f.body.push('\n');
    write_text(file, &f)?;
    Ok(t)
}

// ===== Fokus =====

/// Zadanie w fokusie z `state.json` (tylko gdy z dzisiejszą datą: Jarvis zeruje fokus codziennie).
pub fn focus(dir: &Path, today: &str) -> Option<String> {
    let v: Value = serde_json::from_str(&fs::read_to_string(dir.join("state.json")).ok()?).ok()?;
    if v.get("date")?.as_str()? != today {
        return None;
    }
    v.get("focus")?.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

// ===== config.json =====

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuardConfig {
    pub enabled: bool,
    /// 0 = poniedziałek.
    pub days: Vec<u8>,
    /// `["08:00", "18:00"]`.
    pub hours: [String; 2],
    pub warn_minutes: f64,
    pub overlay_minutes: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    pub guard: GuardConfig,
    pub tasks_file: String,
}

fn default_tasks_file(dir: &Path) -> String {
    dir.join("tasks.md").to_string_lossy().to_string()
}

/// Odczyt z wartościami domyślnymi Jarvisa tam, gdzie pola brak.
pub fn config_from(v: &Value, dir: &Path) -> Config {
    let g = v.get("guard").cloned().unwrap_or(Value::Null);
    let hours: Vec<String> =
        g.get("hours").and_then(|h| h.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    Config {
        guard: GuardConfig {
            enabled: g.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
            days: g
                .get("days")
                .and_then(|d| d.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u8)).collect())
                .unwrap_or_else(|| vec![0, 1, 2, 3, 4]),
            hours: [
                hours.first().cloned().unwrap_or_else(|| "08:00".into()),
                hours.get(1).cloned().unwrap_or_else(|| "18:00".into()),
            ],
            warn_minutes: g.get("warn_minutes").and_then(|x| x.as_f64()).unwrap_or(5.0),
            overlay_minutes: g.get("overlay_minutes").and_then(|x| x.as_f64()).unwrap_or(10.0),
        },
        tasks_file: v.get("tasks_file").and_then(|x| x.as_str()).map(String::from).unwrap_or_else(|| default_tasks_file(dir)),
    }
}

fn valid_hour(h: &str) -> bool {
    let p: Vec<&str> = h.split(':').collect();
    p.len() == 2
        && p[0].len() == 2
        && p[1].len() == 2
        && p[0].parse::<u8>().is_ok_and(|x| x < 24)
        && p[1].parse::<u8>().is_ok_and(|x| x < 60)
}

pub fn validate(c: &Config) -> Result<(), String> {
    let g = &c.guard;
    if !valid_hour(&g.hours[0]) || !valid_hour(&g.hours[1]) {
        return Err("Godziny w formacie GG:MM, np. 08:00".into());
    }
    if g.hours[0] >= g.hours[1] {
        return Err("Koniec pracy musi być po początku".into());
    }
    if g.days.iter().any(|d| *d > 6) {
        return Err("Nieprawidłowy dzień tygodnia".into());
    }
    if !(g.warn_minutes >= 1.0) || !(g.overlay_minutes >= 1.0) {
        return Err("Progi muszą wynosić co najmniej 1 minutę".into());
    }
    if g.overlay_minutes <= g.warn_minutes {
        return Err("Pełny ekran musi przyjść później niż powiadomienie".into());
    }
    if c.tasks_file.trim().is_empty() || !c.tasks_file.to_lowercase().ends_with(".md") {
        return Err("Rejestr zadań to plik .md".into());
    }
    Ok(())
}

/// Wpisuje do `v` tylko pola, którymi zarządza aplikacja; pozostałe klucze i ich kolejność zostają.
pub fn apply_config(v: &mut Value, c: &Config) {
    if !v.is_object() {
        *v = Value::Object(Default::default());
    }
    let o = v.as_object_mut().unwrap();
    let g = o.entry("guard").or_insert_with(|| Value::Object(Default::default()));
    if !g.is_object() {
        *g = Value::Object(Default::default());
    }
    let g = g.as_object_mut().unwrap();
    let num = |x: f64| if x.fract() == 0.0 { Value::from(x as i64) } else { Value::from(x) };
    g.insert("enabled".into(), Value::from(c.guard.enabled));
    let mut days = c.guard.days.clone();
    days.sort();
    days.dedup();
    g.insert("days".into(), Value::from(days));
    g.insert("hours".into(), Value::from(c.guard.hours.to_vec()));
    g.insert("warn_minutes".into(), num(c.guard.warn_minutes));
    g.insert("overlay_minutes".into(), num(c.guard.overlay_minutes));
    o.insert("tasks_file".into(), Value::from(c.tasks_file.trim()));
}

fn read_config_value(dir: &Path) -> Result<Value, String> {
    let p = dir.join("config.json");
    if !p.exists() {
        return Ok(Value::Object(Default::default()));
    }
    let s = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(|e| format!("config.json: {e}"))
}

pub fn read_config(dir: &Path) -> Result<Config, String> {
    Ok(config_from(&read_config_value(dir)?, dir))
}

pub fn write_config(dir: &Path, c: &Config) -> Result<(), String> {
    validate(c)?;
    let mut v = read_config_value(dir)?;
    apply_config(&mut v, c);
    let p = dir.join("config.json");
    fs::write(&p, serde_json::to_string_pretty(&v).unwrap() + "\n").map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_task_line_like_jarvis() {
        let t = parse_line("- [ ] Potwierdzić konta !1 #pzu #dbamosiebie @2026-11-20").unwrap();
        assert!(!t.done);
        assert_eq!(t.name, "Potwierdzić konta");
        assert_eq!(t.prio, Some(1));
        assert_eq!(t.date.as_deref(), Some("2026-11-20"));
        assert_eq!(t.tags, vec!["pzu", "dbamosiebie"]);
        let d = parse_line("  * [X] Zrobione #wdrożenie").unwrap();
        assert!(d.done);
        assert_eq!(d.prio, None);
        assert_eq!(d.tags, vec!["wdrożenie"]);
        assert!(parse_line("Format:  - [ ] tekst w komentarzu").is_none());
        assert!(parse_line("- [ ]").is_none());
    }

    #[test]
    fn composes_line_and_round_trips() {
        let t = Task {
            raw: String::new(),
            done: false,
            name: "Oferta ABC".into(),
            prio: Some(2),
            date: Some("2026-11-21".into()),
            tags: vec!["#oferta".into(), "klient abc".into(), " ".into()],
        };
        let l = line_of(&t);
        assert_eq!(l, "- [ ] Oferta ABC !2 @2026-11-21 #oferta #klient_abc");
        let back = parse_line(&l).unwrap();
        assert_eq!((back.name, back.prio, back.tags), ("Oferta ABC".into(), Some(2), vec!["oferta".into(), "klient_abc".into()]));
    }

    #[test]
    fn replaces_only_exact_line_and_keeps_crlf() {
        let body = "# Zadania\r\n<!-- - [ ] a -->\r\n- [ ] a\r\n- [ ] a b\r\n";
        let out = replace_line(body, "- [ ] a", "- [x] a").unwrap();
        assert_eq!(out, "# Zadania\r\n<!-- - [ ] a -->\r\n- [x] a\r\n- [ ] a b\r\n");
        assert!(replace_line(body, "- [ ] zniknęło", "x").is_none());
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nw-jarvis-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn edits_file_and_detects_conflict() {
        let d = tmp("tasks");
        let f = d.join("zadania.md");
        fs::write(&f, "\u{feff}# Zadania\n\n- [ ] Pierwsze !1 #a\n").unwrap();
        let t = add_task(&f, "Drugie @2026-11-20 #b").unwrap();
        assert_eq!(t.date.as_deref(), Some("2026-11-20"));
        let all = read_tasks(&f).unwrap();
        assert_eq!(all.len(), 2);
        let mut first = all[0].clone();
        first.done = true;
        first.date = Some("2026-11-22".into());
        update_task(&f, &all[0].raw, &first).unwrap();
        let s = fs::read_to_string(&f).unwrap();
        assert!(s.starts_with('\u{feff}'));
        assert!(s.contains("- [x] Pierwsze !1 @2026-11-22 #a\n- [ ] Drugie @2026-11-20 #b\n"));
        // stara treść linii już nie istnieje: konflikt, plik bez zmian
        assert_eq!(update_task(&f, &all[0].raw, &first).unwrap_err(), CHANGED);
        assert!(add_task(&f, "  #samtag ").is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn config_keeps_other_keys_and_order() {
        let d = tmp("cfg");
        fs::write(
            d.join("config.json"),
            r#"{"poll_seconds": 2, "guard": {"enabled": true, "days": [0,1,2,3,4], "hours": ["08:00","18:00"], "window_minutes": 15, "warn_minutes": 5, "overlay_minutes": 10}, "tasks_file": "C:\\x\\zadania.md", "categories": {"A": ["a"]}}"#,
        )
        .unwrap();
        let mut c = read_config(&d).unwrap();
        assert_eq!(c.guard.hours, ["08:00".to_string(), "18:00".to_string()]);
        c.guard.enabled = false;
        c.guard.days = vec![4, 0, 0];
        c.guard.hours = ["09:00".into(), "17:30".into()];
        c.guard.warn_minutes = 3.0;
        c.guard.overlay_minutes = 7.5;
        write_config(&d, &c).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(d.join("config.json")).unwrap()).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["poll_seconds", "guard", "tasks_file", "categories"]);
        assert_eq!(v["guard"]["window_minutes"], 15);
        assert_eq!(v["guard"]["days"], serde_json::json!([0, 4]));
        assert_eq!(v["guard"]["warn_minutes"], 3);
        assert_eq!(v["guard"]["overlay_minutes"], 7.5);
        assert_eq!(read_config(&d).unwrap().guard.hours[1], "17:30");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn validates_config() {
        let ok = Config {
            guard: GuardConfig { enabled: true, days: vec![0], hours: ["08:00".into(), "18:00".into()], warn_minutes: 5.0, overlay_minutes: 10.0 },
            tasks_file: r"C:\a\zadania.md".into(),
        };
        assert!(validate(&ok).is_ok());
        let mut c = ok.clone();
        c.guard.overlay_minutes = 5.0;
        assert!(validate(&c).is_err());
        let mut c = ok.clone();
        c.guard.hours[0] = "8:00".into();
        assert!(validate(&c).is_err());
        let mut c = ok.clone();
        c.guard.hours = ["18:00".into(), "08:00".into()];
        assert!(validate(&c).is_err());
        let mut c = ok;
        c.tasks_file = "zadania.txt".into();
        assert!(validate(&c).is_err());
    }

    #[test]
    fn focus_only_today() {
        let d = tmp("focus");
        fs::write(d.join("state.json"), r#"{"date": "2026-11-20", "focus": "miku"}"#).unwrap();
        assert_eq!(focus(&d, "2026-11-20").as_deref(), Some("miku"));
        assert_eq!(focus(&d, "2026-11-21"), None);
        let _ = fs::remove_dir_all(d);
    }
}
