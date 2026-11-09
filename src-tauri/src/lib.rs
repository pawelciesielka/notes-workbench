use notes_core::index::{self, Index};
use notes_core::proposals;
use notes_core::search::{self, Search};
use notes_core::semantic::{self, Semantic};
use notes_core::vault;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Default, Serialize, Deserialize, Clone)]
struct Settings {
    vault: Option<String>,
    /// Użytkownik włączył wyszukiwanie semantyczne (i zgodził się na pobranie modelu).
    #[serde(default)]
    semantic: bool,
}

#[derive(Default)]
struct AppState {
    settings: Mutex<Settings>,
    index: Mutex<Option<Index>>,
    search: Mutex<Option<Search>>,
    semantic: Semantic,
    sem_running: AtomicBool,
    sem_again: AtomicBool,
}

/// Uruchamia w tle liczenie wektorów (jeśli włączone); gdy już trwa, zleca ponowny przebieg po nim.
fn start_semantic(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.settings.lock().unwrap().semantic {
        return;
    }
    if state.sem_running.swap(true, Ordering::SeqCst) {
        state.sem_again.store(true, Ordering::SeqCst);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let Ok(data_dir) = app.path().app_local_data_dir() else {
            state.sem_running.store(false, Ordering::SeqCst);
            return;
        };
        loop {
            state.sem_again.store(false, Ordering::SeqCst);
            state.semantic.status.lock().unwrap().running = true;
            state.semantic.status.lock().unwrap().error = None;
            let result = match state.vault() {
                Ok(v) => state.semantic.run(&data_dir, &v, &|s| {
                    let _ = app.emit("semantic-status", s);
                }),
                Err(e) => Err(e),
            };
            {
                let mut s = state.semantic.status.lock().unwrap();
                s.running = false;
                s.phase = String::new();
                s.error = result.err();
            }
            let _ = app.emit("semantic-status", state.semantic.status());
            if !state.sem_again.load(Ordering::SeqCst) {
                break;
            }
        }
        state.sem_running.store(false, Ordering::SeqCst);
    });
}

impl AppState {
    fn with_search<T>(&self, f: impl FnOnce(&mut Search, &PathBuf) -> T) -> Result<T, String> {
        let vault = self.vault()?;
        let mut g = self.search.lock().unwrap();
        let s = g.get_or_insert_with(|| {
            let mut s = Search::new();
            s.sync(&vault);
            s
        });
        Ok(f(s, &vault))
    }

    /// Po zmianie plików: `removed` zniknęła ze swojego miejsca, `updated` ma nową treść.
    fn changed(&self, removed: Option<&str>, updated: Option<&str>) -> Result<(), String> {
        self.with_index(|idx, v| {
            if let Some(r) = removed {
                idx.remove(r);
            }
            if let Some(u) = updated {
                idx.update(v, u);
            }
        })?;
        self.with_search(|s, v| {
            if let Some(r) = removed {
                s.remove(r);
            }
            if let Some(u) = updated {
                s.update(v, u);
            }
        })
    }

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
    *state.search.lock().unwrap() = None;
    state.semantic.reset_store();
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
async fn vault_index(app: AppHandle, state: State<'_, AppState>, rebuild: bool) -> Result<index::Summary, String> {
    if rebuild {
        let vault = state.vault()?;
        let idx = tauri::async_runtime::spawn_blocking(move || Index::build(&vault)).await.map_err(|e| e.to_string())?;
        *state.index.lock().unwrap() = Some(idx);
        state.with_search(|s, v| s.sync(v))?;
    }
    // Wektory dogonią zmiany w tle (liczy tylko zmienione Notatki).
    start_semantic(&app);
    state.with_index(|idx, _| idx.summary())
}

#[tauri::command]
fn search_notes(state: State<AppState>, query: String, limit: Option<usize>) -> Result<search::Results, String> {
    state.with_search(|s, _| s.search(&query, limit.unwrap_or(100)))?
}

#[derive(Serialize)]
struct SemanticInfo {
    #[serde(flatten)]
    status: semantic::Status,
    model: &'static str,
    model_mb: u32,
}

#[tauri::command]
fn semantic_status(state: State<AppState>) -> SemanticInfo {
    let mut status = state.semantic.status();
    status.enabled = state.settings.lock().unwrap().semantic;
    SemanticInfo { status, model: semantic::MODEL_NAME, model_mb: semantic::MODEL_MB }
}

/// Włącza wyszukiwanie semantyczne: przy pierwszym razie pobiera model.
#[tauri::command]
fn semantic_enable(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.semantic = true;
        s.clone()
    };
    save_settings(&app, &s)?;
    start_semantic(&app);
    Ok(())
}

#[tauri::command]
async fn semantic_search(state: State<'_, AppState>, query: String) -> Result<Vec<semantic::SemHit>, String> {
    let titles = |p: &str| state.with_index(|idx, _| idx.title(p)).ok().flatten();
    let sem = state.semantic.search(&query, 60, &titles)?;
    let words = state.with_search(|s, _| s.search(&query, 60))??;
    Ok(semantic::fuse(sem, words.hits, 50))
}

#[tauri::command]
fn similar_notes(state: State<AppState>, path: String) -> Result<Vec<index::NoteRef>, String> {
    let sims = state.semantic.similar(&path, 6);
    state.with_index(|idx, _| {
        sims.into_iter()
            .filter(|(_, s)| *s > semantic::SIMILAR_MIN)
            .filter_map(|(p, _)| idx.title(&p).map(|title| index::NoteRef { stem: String::new(), path: p, title }))
            .collect()
    })
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
    state.changed(None, Some(&path))?;
    Ok(n)
}

#[tauri::command]
fn set_note_title(state: State<AppState>, path: String, title: String) -> Result<vault::NoteData, String> {
    let n = vault::set_title(&state.vault()?, &path, &title)?;
    state.changed(None, Some(&path))?;
    Ok(n)
}

#[derive(Serialize)]
struct ProposalView {
    #[serde(flatten)]
    p: proposals::Proposal,
    note_title: String,
    target_title: Option<String>,
}

/// Oczekujące Propozycje z tytułami Notatek (najpierw sprząta te o nieistniejących Notatkach).
#[tauri::command]
fn list_proposals(state: State<AppState>) -> Result<Vec<ProposalView>, String> {
    let vault = state.vault()?;
    state.with_index(|idx, v| {
        proposals::prune(v, idx)?;
        Ok(proposals::pending(&vault)?
            .into_iter()
            .map(|p| ProposalView {
                note_title: idx.title(&p.note).unwrap_or_else(|| p.note.clone()),
                target_title: p.target.as_ref().map(|t| idx.title(t).unwrap_or_else(|| t.clone())),
                p,
            })
            .collect())
    })?
}

/// Akceptuje Propozycje (zapis we frontmatterze Notatek); zwraca liczbę zaakceptowanych.
#[tauri::command]
fn accept_proposals(state: State<AppState>, ids: Vec<u64>) -> Result<usize, String> {
    let vault = state.vault()?;
    let mut done = 0;
    for id in ids {
        let note = state.with_index(|idx, _| proposals::accept(&vault, idx, id))??;
        state.changed(None, Some(&note))?;
        done += 1;
    }
    Ok(done)
}

#[tauri::command]
fn reject_proposals(state: State<AppState>, ids: Vec<u64>) -> Result<(), String> {
    let vault = state.vault()?;
    for id in ids {
        proposals::reject(&vault, id)?;
    }
    Ok(())
}

/// Oznacza Notatkę jako niedostępną dla AI (Tag `noai` we frontmatterze) albo zdejmuje oznaczenie.
#[tauri::command]
fn set_noai(state: State<AppState>, path: String, on: bool) -> Result<vault::NoteData, String> {
    let n = vault::set_fm_tag(&state.vault()?, &path, index::NOAI_TAG, on)?;
    state.changed(None, Some(&path))?;
    Ok(n)
}

#[tauri::command]
fn create_note(state: State<AppState>, folder: String, title: String) -> Result<String, String> {
    let p = vault::create_note(&state.vault()?, &folder, &title)?;
    state.changed(None, Some(&p))?;
    Ok(p)
}

#[tauri::command]
fn trash_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::trash_note(&state.vault()?, &path)?;
    state.changed(Some(&path), None)?;
    Ok(p)
}

#[tauri::command]
fn restore_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::restore_note(&state.vault()?, &path)?;
    state.changed(None, Some(&p))?;
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
            search_notes,
            semantic_status,
            semantic_enable,
            semantic_search,
            similar_notes,
            note_links,
            resolve_link,
            tag_notes,
            graph,
            save_note,
            set_note_title,
            set_noai,
            list_proposals,
            accept_proposals,
            reject_proposals,
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
