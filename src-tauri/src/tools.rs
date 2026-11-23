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
