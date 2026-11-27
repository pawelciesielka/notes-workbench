//! Narzędzia: akcje systemowe pod ręką (przycisk Narzędzia na pasku bocznym).

use std::process::Command;

fn powershell() -> Command {
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-Command"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Otwiera plik hosts w Notatniku jako administrator (Windows pyta przez UAC).
pub fn edit_hosts() -> Result<(), String> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let hosts = format!(r"{root}\System32\drivers\etc\hosts");
    let script = format!(
        "Start-Process -FilePath notepad.exe -ArgumentList '\"{}\"' -Verb RunAs -ErrorAction Stop",
        hosts.replace('\'', "''")
    );
    let out = powershell().arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    // Odmowa w oknie UAC kończy Start-Process błędem „anulowana przez użytkownika”.
    if err.contains("canceled") || err.contains("anulowan") || err.contains("1223") {
        return Err("Anulowano: Windows nie dostał zgody administratora.".into());
    }
    Err(format!("Nie udało się otworzyć pliku hosts: {}", err.trim()))
}

/// Skrypt PowerShell dodany przez użytkownika jako narzędzie.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct ScriptTool {
    pub id: String,
    pub name: String,
    /// Pełna ścieżka do pliku `.ps1`.
    pub path: String,
    /// Uruchamiaj jako administrator (Windows pyta przez UAC).
    #[serde(default)]
    pub admin: bool,
    /// Pokaż okno PowerShell: skrypt może pytać i czekać na Enter, aplikacja nie czeka na jego koniec.
    #[serde(default = "yes")]
    pub window: bool,
}

fn yes() -> bool {
    true
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Uruchamia skrypt i czeka na koniec. Zwraca ostatnie linie wyjścia (bez trybu administratora).
pub fn run_script(t: &ScriptTool) -> Result<String, String> {
    let path = std::path::Path::new(&t.path);
    if !path.is_file() {
        return Err(format!("Skrypt nie istnieje: {}", t.path));
    }
    let cancelled = |err: &str| err.contains("canceled") || err.contains("anulowan") || err.contains("1223");
    if t.window {
        // Zwykłe okno konsoli: widać wyjście, skrypt może pytać. Nie czekamy na koniec.
        let args = format!("-NoProfile -ExecutionPolicy Bypass -File \"{}\"", t.path);
        let script = format!(
            "Start-Process -FilePath powershell.exe -ArgumentList {}{} -ErrorAction Stop",
            ps_quote(&args),
            if t.admin { " -Verb RunAs" } else { "" }
        );
        let out = powershell().arg(script).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok("Uruchomiono w osobnym oknie.".into());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if cancelled(&err) {
            return Err("Anulowano: Windows nie dostał zgody administratora.".into());
        }
        return Err(format!("Nie udało się uruchomić skryptu: {}", err.trim()));
    }
    if t.admin {
        // Podniesiony proces nie odda nam wyjścia; czekamy na koniec i sprawdzamy kod wyjścia.
        // -NonInteractive: pytanie (Read-Host) kończy skrypt błędem zamiast zawiesić go w ukrytym oknie.
        let args = format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\"", t.path);
        let script = format!(
            "$p = Start-Process -FilePath powershell.exe -ArgumentList {} -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ErrorAction Stop; exit $p.ExitCode",
            ps_quote(&args)
        );
        let out = powershell().arg(script).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok(String::new());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if cancelled(&err) {
            return Err("Anulowano: Windows nie dostał zgody administratora.".into());
        }
        let code = out.status.code().unwrap_or(-1);
        return Err(if err.trim().is_empty() {
            format!("Skrypt zakończył się kodem {code}. Jeśli o coś pyta albo czeka na Enter, włącz „Pokaż okno PowerShell”.")
        } else {
            format!("Skrypt zakończył się błędem: {}", err.trim())
        });
    }
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", &t.path]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let out = c.output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let tail = |s: &str| {
        let lines: Vec<&str> = s.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
        lines[lines.len().saturating_sub(3)..].join("\n")
    };
    if out.status.success() {
        return Ok(tail(&stdout));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let msg = if err.trim().is_empty() { tail(&stdout) } else { tail(&err) };
    Err(format!(
        "Skrypt zakończył się kodem {}{}",
        out.status.code().unwrap_or(-1),
        if msg.is_empty() { ".".into() } else { format!(": {msg}") }
    ))
}
