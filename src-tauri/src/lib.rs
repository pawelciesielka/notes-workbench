mod index;
mod vault;

use index::Index;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Default, Serialize, Deserialize, Clone)]
struct Settings {
    vault: Option<String>,
}

#[derive(Default)]
struct AppState {
    settings: Mutex<Settings>,
    index: Mutex<Option<Index>>,
}

impl AppState {
    /// Działa na indeksie Grafu wiedzy, budując go przy pierwszym użyciu.
    fn with_index<T>(&self, f: impl FnOnce(&mut Index, &PathBuf) -> T) -> Result<T, String> {
        let vault = self.vault()?;
        let mut g = self.index.lock().unwrap();
        let idx = g.get_or_insert_with(|| Index::build(&vault));
        Ok(f(idx, &vault))
    }

    fn vault(&self) -> Result<PathBuf, String> {
        self.settings
            .lock()
            .unwrap()
            .vault
            .clone()
            .map(PathBuf::from)
            .ok_or_else(|| "Nie wybrano Skarbca".to_string())
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn save_settings(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let p = settings_path(app).ok_or("Brak katalogu ustawień")?;
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    fs::write(p, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_vault(state: State<AppState>) -> Option<String> {
    let v = state.settings.lock().unwrap().vault.clone();
    v.filter(|p| PathBuf::from(p).is_dir())
}

#[tauri::command]
fn set_vault(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    if !PathBuf::from(&path).is_dir() {
        return Err("Wskazany katalog nie istnieje".into());
    }
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.vault = Some(path);
        s.clone()
    };
    *state.index.lock().unwrap() = None;
    save_settings(&app, &s)
}

#[derive(Serialize, Clone)]
struct Progress {
    done: usize,
    total: usize,
}

#[tauri::command]
async fn import_amno(app: AppHandle, export: String, vault: String) -> Result<amno_import::Report, String> {
    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let opts = amno_import::Options {
            export_html: PathBuf::from(export),
            vault: PathBuf::from(vault),
            date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        };
        let mut last = 0usize;
        amno_import::run(&opts, &mut |done, total| {
            if done == total || done - last >= 10 {
                last = done;
                let _ = app2.emit("import-progress", Progress { done, total });
            }
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let state = app.state::<AppState>();
    set_vault(app.clone(), state, report.vault.clone())?;
    Ok(report)
}

#[tauri::command]
fn vault_tree(state: State<AppState>, trash: bool) -> Result<Vec<vault::TreeNode>, String> {
    Ok(vault::tree(&state.vault()?, trash))
}

#[tauri::command]
fn read_note(state: State<AppState>, path: String) -> Result<vault::NoteData, String> {
    vault::read_note(&state.vault()?, &path)
}

/// Buduje indeks od nowa (np. po powrocie do okna, gdy pliki mogły zmienić się z zewnątrz).
#[tauri::command]
async fn vault_index(state: State<'_, AppState>, rebuild: bool) -> Result<index::Summary, String> {
    if rebuild {
        let vault = state.vault()?;
        let idx = tauri::async_runtime::spawn_blocking(move || Index::build(&vault)).await.map_err(|e| e.to_string())?;
        *state.index.lock().unwrap() = Some(idx);
    }
    state.with_index(|idx, _| idx.summary())
}

#[tauri::command]
fn note_links(state: State<AppState>, path: String) -> Result<index::NoteLinks, String> {
    let note = vault::read_note(&state.vault()?, &path)?;
    state.with_index(|idx, _| idx.note_links(&path, &note.raw))
}

#[tauri::command]
fn resolve_link(state: State<AppState>, from: String, target: String) -> Result<Option<String>, String> {
    state.with_index(|idx, _| idx.resolve(&from, &target))
}

#[tauri::command]
fn tag_notes(state: State<AppState>, tag: String) -> Result<Vec<index::NoteRef>, String> {
    state.with_index(|idx, _| idx.tag_notes(&tag))
}

#[tauri::command]
fn graph(state: State<AppState>) -> Result<index::Graph, String> {
    state.with_index(|idx, _| idx.graph())
}

#[tauri::command]
fn save_note(state: State<AppState>, path: String, body: String, expected: String) -> Result<vault::NoteData, String> {
    let n = vault::save_body(&state.vault()?, &path, &body, &expected)?;
    state.with_index(|idx, v| idx.update(v, &path))?;
    Ok(n)
}

#[tauri::command]
fn set_note_title(state: State<AppState>, path: String, title: String) -> Result<vault::NoteData, String> {
    let n = vault::set_title(&state.vault()?, &path, &title)?;
    state.with_index(|idx, v| idx.update(v, &path))?;
    Ok(n)
}

#[tauri::command]
fn create_note(state: State<AppState>, folder: String, title: String) -> Result<String, String> {
    let p = vault::create_note(&state.vault()?, &folder, &title)?;
    state.with_index(|idx, v| idx.update(v, &p))?;
    Ok(p)
}

#[tauri::command]
fn trash_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::trash_note(&state.vault()?, &path)?;
    state.with_index(|idx, _| idx.remove(&path))?;
    Ok(p)
}

#[tauri::command]
fn restore_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::restore_note(&state.vault()?, &path)?;
    state.with_index(|idx, v| idx.update(v, &p))?;
    Ok(p)
}

#[tauri::command]
fn read_report(state: State<AppState>) -> Result<Option<serde_json::Value>, String> {
    let p = state.vault()?.join(vault::APP_DIR).join("import").join("raport.json");
    match fs::read_to_string(p) {
        Ok(s) => serde_json::from_str(&s).map(Some).map_err(|e| e.to_string()),
        Err(_) => Ok(None),
    }
}

/// Otwiera plik ze Skarbca (Załącznik) w domyślnym programie Windows.
#[tauri::command]
fn open_vault_file(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let p = vault::resolve(&state.vault()?, &path)?;
    if !p.exists() {
        return Err(format!("Nie ma pliku {}", p.display()));
    }
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Otwiera odnośnik spoza Skarbca: stronę w przeglądarce albo Link zewnętrzny do pliku.
#[tauri::command]
fn open_link(app: AppHandle, url: String) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("file:") {
        let p = urlencoding::decode(url[5..].trim_start_matches('/')).map_err(|e| e.to_string())?;
        let win = p.replace('/', "\\");
        if !PathBuf::from(&win).exists() {
            return Err(format!("Plik nie istnieje: {win}"));
        }
        return app.opener().open_path(win, None::<&str>).map_err(|e| e.to_string());
    }
    if lower.starts_with("http:") || lower.starts_with("https:") || lower.starts_with("mailto:") {
        return app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string());
    }
    Err(format!("Nieobsługiwany odnośnik: {url}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Some(p) = settings_path(app.handle()) {
                if let Ok(s) = fs::read_to_string(p) {
                    if let Ok(s) = serde_json::from_str::<Settings>(&s) {
                        *app.state::<AppState>().settings.lock().unwrap() = s;
                    }
                }
            }
            Ok(())
        })
        // Pliki Skarbca (obrazy, PDF) dla widoku Notatki: http://vault.localhost/<ścieżka>.
        .register_uri_scheme_protocol("vault", |ctx, req| {
            let respond = |status: u16, mime: &str, body: Vec<u8>| {
                tauri::http::Response::builder()
                    .status(status)
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(body)
                    .unwrap()
            };
            let state = ctx.app_handle().state::<AppState>();
            let Ok(root) = state.vault() else { return respond(404, "text/plain", Vec::new()) };
            let raw = req.uri().path();
            let decoded = urlencoding::decode(raw).map(|s| s.into_owned()).unwrap_or_else(|_| raw.to_string());
            match vault::resolve(&root, &decoded).and_then(|p| fs::read(&p).map(|b| (p, b)).map_err(|e| e.to_string())) {
                Ok((p, bytes)) => respond(200, vault::mime(&p), bytes),
                Err(_) => respond(404, "text/plain", Vec::new()),
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_vault,
            set_vault,
            import_amno,
            vault_tree,
            read_note,
            vault_index,
            note_links,
            resolve_link,
            tag_notes,
            graph,
            save_note,
            set_note_title,
            create_note,
            trash_note,
            restore_note,
            read_report,
            open_vault_file,
            open_link
        ])
        .run(tauri::generate_context!())
        .expect("błąd uruchomienia aplikacji");
}
