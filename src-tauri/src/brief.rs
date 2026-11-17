//! Daily brief: pliki Markdown z podsumowaniem dnia (generowane przez AI) w Folderze wskazanym w ustawieniach.
//! Zadanie Harmonogramu zadań Windows uruchamia `notes-workbench.exe --daily-brief` przy logowaniu
//! i odblokowaniu komputera; okno pokazuje się tylko przy pierwszym takim wejściu danego dnia,
//! i tylko wtedy, gdy jest plik z dzisiejszą datą w nazwie.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Nazwa zadania w Harmonogramie zadań.
pub const TASK_NAME: &str = "Notes Workbench - Daily brief";
pub const ARG: &str = "--daily-brief";
/// Ten sam katalog co `app_config_dir()` Tauri (Roaming\<identyfikator>), ale dostępny przed startem Tauri.
const IDENTIFIER: &str = "eu.ciesielka.notesworkbench";

#[derive(Serialize, Clone)]
pub struct Brief {
    /// Nazwa pliku.
    pub name: String,
    /// Pełna ścieżka na dysku.
    pub file: String,
    /// Ścieżka względem Skarbca, gdy brief leży w Skarbcu (można go otworzyć jako Notatkę).
    pub note: Option<String>,
    pub content: String,
    /// Ostatnia modyfikacja pliku (ms od 1970).
    pub modified: Option<u64>,
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(IDENTIFIER))
}

fn seen_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("brief-seen.txt"))
}

/// Czy dzisiejszy brief był już pokazany (osobny plik, żeby nie ścigać się z zapisem ustawień przez okno główne).
pub fn seen_today() -> bool {
    seen_path().and_then(|p| fs::read_to_string(p).ok()).is_some_and(|s| s.trim() == today())
}

pub fn mark_seen() -> Result<(), String> {
    let p = seen_path().ok_or("Brak katalogu ustawień")?;
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    fs::write(p, today()).map_err(|e| e.to_string())
}

/// Plik `.md` z dzisiejszą datą (RRRR-MM-DD) w nazwie; przy kilku: najświeższy.
pub fn find_today(dir: &Path, vault: Option<&Path>) -> Result<Option<Brief>, String> {
    if !dir.is_dir() {
        return Err(format!("Folder Daily brief nie istnieje: {}", dir.display()));
    }
    let date = today();
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if !p.is_file() || !name.to_lowercase().ends_with(".md") || !name.contains(&date) {
            continue;
        }
        let m = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(bm, _)| m > *bm) {
            best = Some((m, p));
        }
    }
    let Some((m, p)) = best else { return Ok(None) };
    let content = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(Some(Brief {
        name: p.file_name().unwrap().to_string_lossy().to_string(),
        file: p.to_string_lossy().to_string(),
        note: vault.and_then(|v| relative(v, &p)),
        content,
        modified: m.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64),
    }))
}

/// Ścieżka `p` względem Skarbca (z `/`), bez względu na wielkość liter, jak w Windows.
fn relative(vault: &Path, p: &Path) -> Option<String> {
    let norm = |x: &Path| x.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string();
    let (v, f) = (norm(vault), norm(p));
    if f.len() > v.len() + 1 && f[..v.len()].eq_ignore_ascii_case(&v) && f.as_bytes()[v.len()] == b'/' {
        Some(f[v.len() + 1..].to_string())
    } else {
        None
    }
}

/// Folder briefów zapisany w ustawieniach (czytany wprost z pliku, przed startem Tauri).
pub fn configured_dir() -> Option<(PathBuf, Option<PathBuf>)> {
    let s = fs::read_to_string(config_dir()?.join("settings.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let dir = v.get("brief_dir")?.as_str()?;
    let vault = v.get("vault").and_then(|x| x.as_str()).map(PathBuf::from);
    Some((PathBuf::from(dir), vault))
}

/// Czy uruchomienie z Harmonogramu ma pokazać okno: jest dzisiejszy brief i nie był jeszcze pokazany.
pub fn should_show() -> bool {
    if seen_today() {
        return false;
    }
    let Some((dir, vault)) = configured_dir() else { return false };
    matches!(find_today(&dir, vault.as_deref()), Ok(Some(_)))
}

// ===== Harmonogram zadań =====

fn schtasks() -> Command {
    let mut c = Command::new("schtasks");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

pub fn autostart_enabled() -> bool {
    schtasks().args(["/Query", "/TN", TASK_NAME]).output().is_ok_and(|o| o.status.success())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn task_xml(exe: &Path, user: &str) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    let user = xml_escape(user);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Notes Workbench: pokazuje dzisiejszy Daily brief przy pierwszym wejściu na komputer danego dnia.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT15S</Delay>
    </LogonTrigger>
    <SessionStateChangeTrigger>
      <Enabled>true</Enabled>
      <StateChange>SessionUnlock</StateChange>
      <UserId>{user}</UserId>
      <Delay>PT3S</Delay>
    </SessionStateChangeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{ARG}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn run_schtasks(args: &[&str]) -> Result<(), String> {
    let o = schtasks().args(args).output().map_err(|e| format!("schtasks: {e}"))?;
    if o.status.success() {
        return Ok(());
    }
    // schtasks pisze w stronie kodowej konsoli; polskie znaki mogą się zniekształcić, ale sens zostaje.
    let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
    Err(if msg.is_empty() { format!("schtasks zakończył się kodem {:?}", o.status.code()) } else { msg })
}

/// Rejestruje (albo usuwa) zadanie Harmonogramu dla bieżącego użytkownika.
pub fn set_autostart(on: bool) -> Result<(), String> {
    if !on {
        return if autostart_enabled() { run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]) } else { Ok(()) };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => return Err("Nie znam nazwy użytkownika Windows".into()),
    };
    let xml = task_xml(&exe, &user);
    // schtasks /XML oczekuje pliku w UTF-16 (zgodnie z deklaracją w nagłówku).
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let tmp = std::env::temp_dir().join("notes-workbench-daily-brief.xml");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let tmp_s = tmp.to_string_lossy().to_string();
    let r = run_schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &tmp_s, "/F"]);
    let _ = fs::remove_file(&tmp);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_to_vault() {
        let v = Path::new(r"C:\Dane\Notatki");
        assert_eq!(
            relative(v, Path::new(r"c:\dane\Notatki\Dziennik\Daily_brief\a.md")).as_deref(),
            Some("Dziennik/Daily_brief/a.md")
        );
        assert_eq!(relative(v, Path::new(r"C:\Dane\NotatkiX\a.md")), None);
    }

    #[test]
    fn xml_escapes_paths() {
        let x = task_xml(Path::new(r"C:\A&B\nw.exe"), r"DOM\Paweł");
        assert!(x.contains(r"C:\A&amp;B\nw.exe"));
        assert!(x.contains("<Arguments>--daily-brief</Arguments>"));
    }
}
