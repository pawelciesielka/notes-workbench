//! Podłączanie serwera MCP Notes Workbench do klientów AI: Claude Desktop, Claude Code i Codex.
//!
//! Serwerem jest sama aplikacja uruchomiona z `--mcp`. Kopiujemy ją w stałe miejsce poza katalogiem
//! instalacji (`~/.notes-workbench/bin`), bo działający serwer blokuje plik, a instalator nowej wersji
//! musi móc podmienić aplikację. W konfiguracji klientów zmieniamy tylko wpis `notes-workbench`.

use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const NAME: &str = "notes-workbench";

#[derive(Serialize, Clone)]
pub struct Client {
    pub id: &'static str,
    pub name: &'static str,
    /// Program jest zainstalowany (ma swój katalog konfiguracji).
    pub detected: bool,
    pub connected: bool,
    pub config: Option<String>,
    /// Co zrobić po podłączeniu, żeby klient zobaczył serwer.
    pub after: &'static str,
}

#[derive(Serialize)]
pub struct Status {
    pub command: String,
    pub args: Vec<String>,
    pub vault: String,
    pub clients: Vec<Client>,
    /// Wpis do ręcznego wklejenia w innych programach (Cursor, Windsurf…).
    pub snippet: String,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn home() -> PathBuf {
    env_path("USERPROFILE").or_else(|| env_path("HOME")).unwrap_or_default()
}

/// Stałe miejsce serwera, wpisywane do konfiguracji klientów.
pub fn server_path() -> PathBuf {
    home().join(".notes-workbench").join("bin").join("notes-workbench-mcp.exe")
}

pub fn server_args(vault: &str) -> Vec<String> {
    vec!["--mcp".into(), "--vault".into(), vault.into()]
}

/// Kopiuje bieżącą aplikację jako serwer, jeśli kopia jest starsza albo jej nie ma.
/// Gdy kopia jest w użyciu (działa w Claude), zostawiamy ją: zaktualizuje się przy kolejnym podłączeniu.
pub fn install_server() -> Result<PathBuf, String> {
    let src = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst = server_path();
    let meta = |p: &Path| fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()));
    if let (Some((sl, sm)), Some((dl, dm))) = (meta(&src), meta(&dst)) {
        if sl == dl && dm >= sm {
            return Ok(dst);
        }
    }
    fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = dst.with_extension("exe.new");
    fs::copy(&src, &tmp).map_err(|e| format!("Kopiowanie serwera: {e}"))?;
    match fs::rename(&tmp, &dst) {
        Ok(()) => Ok(dst),
        Err(_) if dst.exists() => {
            let _ = fs::remove_file(&tmp);
            Ok(dst)
        }
        Err(e) => Err(format!("Zapis serwera {}: {e}", dst.display())),
    }
}

// ===== Pliki JSON (Claude Desktop, Claude Code) =====

fn read_json(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display())),
        _ => Ok(json!({})),
    }
}

fn write_json(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    // Jednorazowa kopia zapasowa sprzed pierwszej zmiany.
    let bak = path.with_extension("json.przed-notes-workbench.bak");
    if path.exists() && !bak.exists() {
        let _ = fs::copy(path, &bak);
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(v).unwrap() + "\n").map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn json_has(path: &Path) -> bool {
    read_json(path).ok().is_some_and(|v| v["mcpServers"].get(NAME).is_some())
}

pub fn json_set(path: &Path, entry: Value) -> Result<(), String> {
    let mut v = read_json(path)?;
    if !v.is_object() {
        return Err(format!("{}: nieoczekiwana zawartość", path.display()));
    }
    let servers = v.as_object_mut().unwrap().entry("mcpServers").or_insert_with(|| json!({}));
    servers.as_object_mut().ok_or("mcpServers nie jest obiektem")?.insert(NAME.into(), entry);
    write_json(path, &v)
}

pub fn json_remove(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut v = read_json(path)?;
    if let Some(s) = v.get_mut("mcpServers").and_then(Value::as_object_mut) {
        if s.remove(NAME).is_some() {
            write_json(path, &v)?;
        }
    }
    Ok(())
}

// ===== Codex (TOML) =====

pub fn toml_has(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
        .is_some_and(|d| d.get("mcp_servers").and_then(|t| t.get(NAME)).is_some())
}

pub fn toml_set(path: &Path, command: &str, args: &[String]) -> Result<(), String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    if doc.get("mcp_servers").is_none() {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        doc.insert("mcp_servers", toml_edit::Item::Table(t));
    }
    let servers = doc["mcp_servers"].as_table_mut().ok_or("mcp_servers nie jest tabelą")?;
    let mut t = toml_edit::Table::new();
    t.insert("command", toml_edit::value(command));
    let mut a = toml_edit::Array::new();
    args.iter().for_each(|x| a.push(x.as_str()));
    t.insert("args", toml_edit::value(a));
    servers.insert(NAME, toml_edit::Item::Table(t));
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let bak = path.with_extension("toml.przed-notes-workbench.bak");
    if path.exists() && !bak.exists() {
        let _ = fs::copy(path, &bak);
    }
    fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

pub fn toml_remove(path: &Path) -> Result<(), String> {
    let Ok(text) = fs::read_to_string(path) else { return Ok(()) };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    let removed = doc.get_mut("mcp_servers").and_then(|t| t.as_table_mut()).and_then(|t| t.remove(NAME)).is_some();
    if removed {
        fs::write(path, doc.to_string()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ===== Klienci =====

/// Plik konfiguracji Claude Desktop. Wersja ze Sklepu Windows trzyma dane w `%LOCALAPPDATA%\Claude-Data\…`,
/// zwykła instalacja w `%APPDATA%\Claude`.
fn claude_desktop_config() -> (bool, PathBuf) {
    if let Some(local) = env_path("LOCALAPPDATA") {
        if let Ok(rd) = fs::read_dir(local.join("Claude-Data")) {
            for e in rd.flatten() {
                let p = e.path().join("claude_desktop_config.json");
                if p.is_file() {
                    return (true, p);
                }
            }
        }
    }
    let roaming = env_path("APPDATA").unwrap_or_default().join("Claude");
    (roaming.is_dir(), roaming.join("claude_desktop_config.json"))
}

fn claude_code_config() -> (bool, PathBuf) {
    let p = home().join(".claude.json");
    let detected = p.is_file() || home().join(".claude").is_dir() || find_exe("claude").is_some();
    (detected, p)
}

fn codex_config() -> (bool, PathBuf) {
    let d = home().join(".codex");
    (d.is_dir(), d.join("config.toml"))
}

/// Program w PATH albo w typowym miejscu instalacji CLI.
fn find_exe(name: &str) -> Option<PathBuf> {
    let exe = format!("{name}.exe");
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.push(home().join(".local").join("bin"));
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

fn hidden_cmd(exe: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: bez mignięcia konsoli
    }
    c
}

pub fn status(vault: &str) -> Status {
    let command = server_path().to_string_lossy().to_string();
    let args = server_args(vault);
    let (dd, dp) = claude_desktop_config();
    let (cd, cp) = claude_code_config();
    let (xd, xp) = codex_config();
    let snippet = serde_json::to_string_pretty(&json!({ NAME: { "command": command, "args": args } })).unwrap();
    Status {
        clients: vec![
            Client {
                id: "claude-desktop",
                name: "Claude Desktop",
                detected: dd,
                connected: json_has(&dp),
                config: Some(dp.to_string_lossy().into()),
                after: "Zamknij Claude całkowicie (ikona w zasobniku → Quit) i uruchom ponownie.",
            },
            Client {
                id: "claude-code",
                name: "Claude Code",
                detected: cd,
                connected: json_has(&cp),
                config: Some(cp.to_string_lossy().into()),
                after: "Działa w nowych sesjach Claude Code (w każdym projekcie).",
            },
            Client {
                id: "codex",
                name: "Codex",
                detected: xd,
                connected: toml_has(&xp),
                config: Some(xp.to_string_lossy().into()),
                after: "Działa w nowych sesjach Codex; aplikację Codex uruchom ponownie.",
            },
        ],
        command,
        args,
        vault: vault.into(),
        snippet,
    }
}

pub fn connect(id: &str, vault: &str) -> Result<(), String> {
    let exe = install_server()?;
    let command = exe.to_string_lossy().to_string();
    let args = server_args(vault);
    match id {
        "claude-desktop" => json_set(&claude_desktop_config().1, json!({ "command": command, "args": args })),
        "claude-code" => {
            // Claude Code ciągle zapisuje swój plik; jego własne polecenie robi to bezpiecznie.
            if let Some(cli) = find_exe("claude") {
                let _ = hidden_cmd(&cli).args(["mcp", "remove", "--scope", "user", NAME]).output();
                let out = hidden_cmd(&cli)
                    .args(["mcp", "add", "--scope", "user", NAME, "--", &command])
                    .args(&args)
                    .output()
                    .map_err(|e| e.to_string())?;
                if out.status.success() {
                    return Ok(());
                }
            }
            json_set(&claude_code_config().1, json!({ "type": "stdio", "command": command, "args": args, "env": {} }))
        }
        "codex" => toml_set(&codex_config().1, &command, &args),
        _ => Err(format!("Nieznany klient: {id}")),
    }
}

pub fn disconnect(id: &str) -> Result<(), String> {
    match id {
        "claude-desktop" => json_remove(&claude_desktop_config().1),
        "claude-code" => {
            if let Some(cli) = find_exe("claude") {
                let out = hidden_cmd(&cli).args(["mcp", "remove", "--scope", "user", NAME]).output();
                if out.is_ok_and(|o| o.status.success()) {
                    return Ok(());
                }
            }
            json_remove(&claude_code_config().1)
        }
        "codex" => toml_remove(&codex_config().1),
        _ => Err(format!("Nieznany klient: {id}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keeps_other_entries_and_order() {
        let dir = std::env::temp_dir().join(format!("nw-mcpjson-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("claude_desktop_config.json");
        fs::write(&p, r#"{"mcpServers":{"zeta":{"command":"z"},"alpha":{"command":"a"}},"preferences":{"x":1}}"#).unwrap();
        assert!(!json_has(&p));
        json_set(&p, json!({"command": "C:\\a b\\x.exe", "args": ["--mcp"]})).unwrap();
        assert!(json_has(&p));
        let v: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let keys: Vec<_> = v["mcpServers"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, vec!["zeta", "alpha", NAME]);
        assert_eq!(v["preferences"]["x"], 1);
        assert!(dir.join("claude_desktop_config.json.przed-notes-workbench.bak").exists());
        json_remove(&p).unwrap();
        assert!(!json_has(&p));
        json_set(&dir.join("nowy").join("c.json"), json!({"command": "x"})).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn toml_preserves_rest_of_file() {
        let dir = std::env::temp_dir().join(format!("nw-mcptoml-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        let orig = "model = \"gpt\" # komentarz\n\n[projects.'c:\\work\\x']\ntrust_level = \"trusted\"\n";
        fs::write(&p, orig).unwrap();
        toml_set(&p, "C:\\Users\\D\\.notes-workbench\\bin\\notes-workbench-mcp.exe", &server_args("C:\\Notatki")).unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.starts_with(orig), "{s}");
        assert!(s.contains("[mcp_servers.notes-workbench]"), "{s}");
        assert!(s.contains(r#"args = ["--mcp", "--vault", 'C:\Notatki']"#), "{s}");
        assert!(toml_has(&p));
        toml_remove(&p).unwrap();
        assert!(!toml_has(&p));
        assert!(fs::read_to_string(&p).unwrap().contains("trust_level"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
