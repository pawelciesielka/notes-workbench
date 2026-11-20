mod brief;
mod jarvis;
mod mcp_setup;
mod startlog;

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
    /// Folder z plikami Daily brief (Markdown z datą w nazwie).
    #[serde(default)]
    brief_dir: Option<String>,
    /// Katalog Jarvisa (z `jarvis.py`); brak = `C:\WORK\jarvis`.
    #[serde(default)]
    jarvis_dir: Option<String>,
    /// Interpreter Pythona dla Jarvisa; brak = wykryty automatycznie.
    #[serde(default)]
    jarvis_python: Option<String>,
}

#[derive(Default)]
struct AppState {
    settings: Mutex<Settings>,
    index: Mutex<Option<Index>>,
    search: Mutex<Option<Search>>,
    semantic: Semantic,
    sem_running: AtomicBool,
    sem_again: AtomicBool,
    /// Użytkownik wszedł w Szukaj w tej sesji: dopiero wtedy wczytujemy model (kilkaset MB pamięci).
    sem_wanted: AtomicBool,
    /// Uruchomienie z Harmonogramu (`--daily-brief`): okno pokazuje sam brief, dopóki użytkownik nie otworzy aplikacji.
    brief_mode: AtomicBool,
}

/// Uruchamia w tle liczenie wektorów (jeśli włączone); gdy już trwa, zleca ponowny przebieg po nim.
fn start_semantic(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.settings.lock().unwrap().semantic || !state.sem_wanted.load(Ordering::SeqCst) {
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
            startlog::timed("indeks wyszukiwania: budowa od zera", || {
                let mut s = Search::new();
                s.sync(&vault);
                s
            })
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
        let idx = g.get_or_insert_with(|| startlog::timed("indeks Notatek: budowa", || Index::build(&vault)));
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

#[tauri::command(async)]
fn get_vault(state: State<AppState>) -> Option<String> {
    let v = state.settings.lock().unwrap().vault.clone();
    v.filter(|p| PathBuf::from(p).is_dir())
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn vault_tree(state: State<AppState>, trash: bool) -> Result<Vec<vault::TreeNode>, String> {
    let v = state.vault()?;
    let label = if trash { "drzewo Kosza" } else { "drzewo Notatek" };
    Ok(startlog::timed(label, || vault::tree(&v, trash)))
}

#[tauri::command(async)]
fn read_note(state: State<AppState>, path: String) -> Result<vault::NoteData, String> {
    vault::read_note(&state.vault()?, &path)
}

/// Buduje indeks od nowa (np. po powrocie do okna, gdy pliki mogły zmienić się z zewnątrz).
#[tauri::command]
async fn vault_index(app: AppHandle, state: State<'_, AppState>, rebuild: bool) -> Result<index::Summary, String> {
    if rebuild {
        startlog::mark("indeks: przebudowa zaczęta");
        let vault = state.vault()?;
        let idx = tauri::async_runtime::spawn_blocking(move || startlog::timed("indeks Notatek: przebudowa", || Index::build(&vault)))
            .await
            .map_err(|e| e.to_string())?;
        *state.index.lock().unwrap() = Some(idx);
        state.with_search(|s, v| startlog::timed("indeks wyszukiwania: dogonienie zmian", || s.sync(v)))?;
    }
    // Wektory dogonią zmiany w tle (liczy tylko zmienione Notatki).
    start_semantic(&app);
    state.with_index(|idx, _| idx.summary())
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn semantic_status(state: State<AppState>) -> SemanticInfo {
    let mut status = state.semantic.status();
    status.enabled = state.settings.lock().unwrap().semantic;
    SemanticInfo { status, model: semantic::MODEL_NAME, model_mb: semantic::MODEL_MB }
}

/// Włącza wyszukiwanie semantyczne: przy pierwszym razie pobiera model.
#[tauri::command(async)]
fn semantic_enable(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.semantic = true;
        s.clone()
    };
    save_settings(&app, &s)?;
    state.sem_wanted.store(true, Ordering::SeqCst);
    start_semantic(&app);
    Ok(())
}

/// Wejście w Szukaj: wczytuje model i dolicza wektory zmienionych Notatek (gdy wyszukiwanie po znaczeniu jest włączone).
#[tauri::command(async)]
fn semantic_activate(app: AppHandle, state: State<AppState>) {
    if !state.sem_wanted.swap(true, Ordering::SeqCst) {
        startlog::mark("wyszukiwanie po znaczeniu: wczytywanie modelu");
    }
    start_semantic(&app);
}

#[tauri::command]
async fn semantic_search(state: State<'_, AppState>, query: String) -> Result<Vec<semantic::SemHit>, String> {
    let titles = |p: &str| state.with_index(|idx, _| idx.title(p)).ok().flatten();
    let sem = state.semantic.search(&query, 60, &titles)?;
    let words = state.with_search(|s, _| s.search(&query, 60))??;
    Ok(semantic::fuse(sem, words.hits, 50))
}

#[tauri::command(async)]
fn similar_notes(app: AppHandle, state: State<AppState>, path: String) -> Result<Vec<index::NoteRef>, String> {
    if state.settings.lock().unwrap().semantic {
        if let Ok(d) = app.path().app_local_data_dir() {
            // Same zapisane wektory, bez modelu: Notatki podobne działają, zanim ktoś wejdzie w Szukaj.
            state.semantic.open_store(&d, &state.vault()?)?;
        }
    }
    let sims = state.semantic.similar(&path, 6);
    state.with_index(|idx, _| {
        sims.into_iter()
            .filter(|(_, s)| *s > semantic::SIMILAR_MIN)
            .filter_map(|(p, _)| idx.title(&p).map(|title| index::NoteRef { stem: String::new(), path: p, title }))
            .collect()
    })
}

#[tauri::command(async)]
fn note_links(state: State<AppState>, path: String) -> Result<index::NoteLinks, String> {
    let note = vault::read_note(&state.vault()?, &path)?;
    state.with_index(|idx, _| idx.note_links(&path, &note.raw))
}

#[tauri::command(async)]
fn resolve_link(state: State<AppState>, from: String, target: String) -> Result<Option<String>, String> {
    state.with_index(|idx, _| idx.resolve(&from, &target))
}

#[tauri::command(async)]
fn tag_notes(state: State<AppState>, tag: String) -> Result<Vec<index::NoteRef>, String> {
    state.with_index(|idx, _| idx.tag_notes(&tag))
}

#[tauri::command(async)]
fn graph(state: State<AppState>) -> Result<index::Graph, String> {
    state.with_index(|idx, _| idx.graph())
}

#[tauri::command(async)]
fn save_note(state: State<AppState>, path: String, body: String, expected: String) -> Result<vault::NoteData, String> {
    let n = vault::save_body(&state.vault()?, &path, &body, &expected)?;
    state.changed(None, Some(&path))?;
    Ok(n)
}

/// Załącznik Notatki: treść w surowym ciele żądania, Notatka i nazwa w nagłówkach (zakodowane URI).
#[tauri::command(async)]
fn add_attachment(state: State<AppState>, request: tauri::ipc::Request<'_>) -> Result<vault::Attachment, String> {
    let header = |k: &str| -> Result<String, String> {
        let v = request.headers().get(k).and_then(|v| v.to_str().ok()).ok_or(format!("Brak nagłówka {k}"))?;
        urlencoding::decode(v).map(|s| s.into_owned()).map_err(|e| e.to_string())
    };
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("Oczekiwano danych pliku".into());
    };
    vault::add_attachment(&state.vault()?, &header("x-note")?, &header("x-name")?, data)
}

#[tauri::command(async)]
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
#[tauri::command(async)]
fn list_proposals(state: State<AppState>) -> Result<Vec<ProposalView>, String> {
    let vault = state.vault()?;
    startlog::mark("Propozycje: wczytywanie");
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

#[derive(Serialize)]
struct AcceptResult {
    done: usize,
    /// Notatki utworzone z Propozycji (drzewo do odświeżenia).
    created: Vec<String>,
    /// Propozycje, których nie dało się zastosować (np. Notatka zmieniona od czasu Propozycji): id i powód.
    failed: Vec<(u64, String)>,
}

/// Akceptuje Propozycje (zapis w Notatkach). Błąd jednej nie zatrzymuje pozostałych.
#[tauri::command(async)]
fn accept_proposals(app: AppHandle, state: State<AppState>, ids: Vec<u64>) -> Result<AcceptResult, String> {
    let vault = state.vault()?;
    let kinds: std::collections::HashMap<u64, proposals::Kind> =
        proposals::pending(&vault)?.into_iter().map(|p| (p.id, p.kind)).collect();
    let mut res = AcceptResult { done: 0, created: Vec::new(), failed: Vec::new() };
    let mut folders = false;
    for id in ids {
        match state.with_index(|idx, _| proposals::accept(&vault, idx, id)).and_then(|r| r) {
            Ok(path) => {
                match kinds.get(&id) {
                    // Nowy Folder i zmiana nazwy zmieniają ścieżki: indeks budowany od nowa po wszystkich.
                    Some(proposals::Kind::Folder | proposals::Kind::Rename) => folders = true,
                    Some(proposals::Kind::Note) => {
                        state.changed(None, Some(&path))?;
                        res.created.push(path);
                    }
                    _ => state.changed(None, Some(&path))?,
                }
                res.done += 1;
            }
            Err(e) => res.failed.push((id, e)),
        }
    }
    if folders {
        state.reindex()?;
        start_semantic(&app);
    }
    Ok(res)
}

#[tauri::command(async)]
fn reject_proposals(state: State<AppState>, ids: Vec<u64>) -> Result<(), String> {
    let vault = state.vault()?;
    for id in ids {
        proposals::reject(&vault, id)?;
    }
    Ok(())
}

/// Oznacza Notatkę jako niedostępną dla AI (Tag `noai` we frontmatterze) albo zdejmuje oznaczenie.
#[tauri::command(async)]
fn set_noai(state: State<AppState>, path: String, on: bool) -> Result<vault::NoteData, String> {
    let n = vault::set_fm_tag(&state.vault()?, &path, index::NOAI_TAG, on)?;
    state.changed(None, Some(&path))?;
    Ok(n)
}

#[tauri::command(async)]
fn create_note(state: State<AppState>, folder: String, title: String) -> Result<String, String> {
    let p = vault::create_note(&state.vault()?, &folder, &title)?;
    state.changed(None, Some(&p))?;
    Ok(p)
}

#[tauri::command(async)]
fn trash_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::trash_note(&state.vault()?, &path)?;
    state.changed(Some(&path), None)?;
    Ok(p)
}

#[tauri::command(async)]
fn restore_note(state: State<AppState>, path: String) -> Result<String, String> {
    let p = vault::restore_note(&state.vault()?, &path)?;
    state.changed(None, Some(&p))?;
    Ok(p)
}

impl AppState {
    /// Po operacjach zmieniających wiele ścieżek naraz: indeks od nowa, wyszukiwanie dogania zmiany.
    fn reindex(&self) -> Result<(), String> {
        let v = self.vault()?;
        *self.index.lock().unwrap() = Some(Index::build(&v));
        self.with_search(|s, v| s.sync(v))?;
        Ok(())
    }
}

#[derive(Serialize)]
struct OpResult {
    path: String,
    /// Ile Notatek miało poprawione Linki.
    fixed: usize,
}

#[tauri::command(async)]
fn create_folder(app: AppHandle, state: State<AppState>, parent: String, name: String) -> Result<String, String> {
    let p = notes_core::ops::create_folder(&state.vault()?, &parent, &name)?;
    start_semantic(&app);
    Ok(p)
}

#[tauri::command(async)]
fn rename_folder(app: AppHandle, state: State<AppState>, path: String, name: String) -> Result<OpResult, String> {
    let vault = state.vault()?;
    let (p, fixed) = state.with_index(|idx, _| notes_core::ops::rename_folder(&vault, idx, &path, &name))??;
    state.reindex()?;
    start_semantic(&app);
    Ok(OpResult { path: p, fixed })
}

#[tauri::command(async)]
fn move_note(app: AppHandle, state: State<AppState>, path: String, folder: String) -> Result<OpResult, String> {
    let vault = state.vault()?;
    let (p, fixed) = state.with_index(|idx, _| notes_core::ops::move_note(&vault, idx, &path, &folder))??;
    state.reindex()?;
    start_semantic(&app);
    Ok(OpResult { path: p, fixed })
}

#[tauri::command(async)]
fn move_folder(app: AppHandle, state: State<AppState>, path: String, parent: String) -> Result<OpResult, String> {
    let vault = state.vault()?;
    let (p, fixed) = state.with_index(|idx, _| notes_core::ops::move_folder(&vault, idx, &path, &parent))??;
    state.reindex()?;
    start_semantic(&app);
    Ok(OpResult { path: p, fixed })
}

/// Miejsce Notatki albo Folderu w Kolejności jego Folderu: przed `before` (`None`: na końcu).
#[tauri::command(async)]
fn set_position(state: State<AppState>, path: String, before: Option<String>) -> Result<(), String> {
    notes_core::ops::set_position(&state.vault()?, &path, before.as_deref())
}

#[tauri::command(async)]
fn trash_folder(state: State<AppState>, path: String) -> Result<String, String> {
    let p = notes_core::ops::trash_folder(&state.vault()?, &path)?;
    state.reindex()?;
    Ok(p)
}

#[tauri::command(async)]
fn restore_folder(app: AppHandle, state: State<AppState>, path: String) -> Result<String, String> {
    let p = notes_core::ops::restore_folder(&state.vault()?, &path)?;
    state.reindex()?;
    start_semantic(&app);
    Ok(p)
}

#[tauri::command(async)]
fn set_folder_noai(state: State<AppState>, path: String, on: bool) -> Result<String, String> {
    let p = notes_core::ops::set_folder_noai(&state.vault()?, &path, on)?;
    state.reindex()?;
    Ok(p)
}

/// Pokazuje plik albo katalog ze Skarbca zaznaczony w Eksploratorze.
#[tauri::command(async)]
fn reveal_path(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let p = vault::resolve(&state.vault()?, &path)?;
    app.opener().reveal_item_in_dir(&p).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn mcp_status(state: State<AppState>) -> Result<mcp_setup::Status, String> {
    Ok(mcp_setup::status(&state.vault()?.to_string_lossy()))
}

/// Podłącza serwer MCP (ten Skarbiec) do klienta AI: `claude-desktop`, `claude-code` albo `codex`.
#[tauri::command]
async fn mcp_connect(state: State<'_, AppState>, client: String) -> Result<mcp_setup::Status, String> {
    let vault = state.vault()?.to_string_lossy().to_string();
    let v = vault.clone();
    tauri::async_runtime::spawn_blocking(move || mcp_setup::connect(&client, &v)).await.map_err(|e| e.to_string())??;
    Ok(mcp_setup::status(&vault))
}

#[tauri::command]
async fn mcp_disconnect(state: State<'_, AppState>, client: String) -> Result<mcp_setup::Status, String> {
    let vault = state.vault()?.to_string_lossy().to_string();
    tauri::async_runtime::spawn_blocking(move || mcp_setup::disconnect(&client)).await.map_err(|e| e.to_string())??;
    Ok(mcp_setup::status(&vault))
}

/// Otwiera katalog Skarbca w Eksploratorze Windows.
#[tauri::command(async)]
fn reveal_vault(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let v = state.vault()?;
    app.opener().open_path(v.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Otwiera plik ze Skarbca (Załącznik) w domyślnym programie Windows.
#[tauri::command(async)]
fn open_vault_file(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let p = vault::resolve(&state.vault()?, &path)?;
    if !p.exists() {
        return Err(format!("Nie ma pliku {}", p.display()));
    }
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Otwiera odnośnik spoza Skarbca: stronę w przeglądarce albo Link zewnętrzny do pliku.
#[tauri::command(async)]
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

/// Dopasowuje okno główne do obszaru roboczego monitora (bez paska zadań) i ustawia je na środku.
/// Domyślny rozmiar z konfiguracji (1400×900) na ekranie ze skalowaniem bywa wyższy niż ekran,
/// a wtedy dół okna chowa się pod paskiem zadań.
fn fit_main_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let monitor = win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let area = m.work_area();
        let scale = m.scale_factor();
        let (aw, ah) = (area.size.width as f64, area.size.height as f64);
        // Najmniejszy rozmiar okna z konfiguracji (900×600) w pikselach fizycznych.
        let (min_w, min_h) = (900.0 * scale, 600.0 * scale);
        if aw < min_w || ah < min_h {
            let _ = win.maximize();
        } else {
            // Rozmiar zewnętrzny (z paskiem tytułu i ramką); `set_size` ustawia wnętrze, więc ramkę odejmujemy.
            let w = (1400.0 * scale).min(aw * 0.94).max(min_w);
            let h = (900.0 * scale).min(ah * 0.94).max(min_h);
            let (fw, fh) = match (win.outer_size(), win.inner_size()) {
                (Ok(o), Ok(i)) => (o.width.saturating_sub(i.width) as f64, o.height.saturating_sub(i.height) as f64),
                _ => (0.0, 0.0),
            };
            let _ = win.set_size(tauri::PhysicalSize::new((w - fw).round() as u32, (h - fh).round() as u32));
            let _ = win.set_position(tauri::PhysicalPosition::new(
                area.position.x + ((aw - w) / 2.0).round() as i32,
                area.position.y + ((ah - h) / 2.0).round() as i32,
            ));
        }
    }
    // Okno startuje ukryte (bez mignięcia w złym miejscu); pokazujemy je zawsze, także gdy nie znamy monitora.
    let _ = win.show();
}


// ===== Daily brief =====

/// Okno briefu: mniejsze niż główne, na środku ekranu, na wierzchu przy starcie (żeby nie zginęło pod innymi).
fn show_brief_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let _ = win.set_title("Daily brief – Notes Workbench");
    if let Some(m) = win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten()) {
        let area = m.work_area();
        let scale = m.scale_factor();
        let (aw, ah) = (area.size.width as f64, area.size.height as f64);
        let w = (900.0 * scale).min(aw * 0.94);
        let h = (820.0 * scale).min(ah * 0.94);
        let _ = win.set_size(tauri::PhysicalSize::new(w.round() as u32, h.round() as u32));
        let _ = win.set_position(tauri::PhysicalPosition::new(
            area.position.x + ((aw - w) / 2.0).round() as i32,
            area.position.y + ((ah - h) / 2.0).round() as i32,
        ));
    }
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    let _ = win.set_focus();
    // Na wierzchu tylko na chwilę: potem okno zachowuje się zwyczajnie.
    let w2 = win.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = w2.set_always_on_top(false);
    });
}

/// `brief`: okno uruchomione z Harmonogramu pokazuje sam brief; `main`: zwykła aplikacja.
#[tauri::command(async)]
fn app_mode(state: State<AppState>) -> &'static str {
    if state.brief_mode.load(Ordering::SeqCst) {
        "brief"
    } else {
        "main"
    }
}

#[derive(Serialize)]
struct BriefConfig {
    dir: Option<String>,
    autostart: bool,
}

#[tauri::command(async)]
fn brief_config(state: State<AppState>) -> BriefConfig {
    BriefConfig { dir: state.settings.lock().unwrap().brief_dir.clone(), autostart: brief::autostart_enabled() }
}

#[tauri::command(async)]
fn set_brief_dir(app: AppHandle, state: State<AppState>, dir: Option<String>) -> Result<(), String> {
    if let Some(d) = &dir {
        if !PathBuf::from(d).is_dir() {
            return Err("Wskazany katalog nie istnieje".into());
        }
    }
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.brief_dir = dir;
        s.clone()
    };
    save_settings(&app, &s)
}

#[tauri::command(async)]
fn set_brief_autostart(on: bool) -> Result<bool, String> {
    brief::set_autostart(on)?;
    Ok(brief::autostart_enabled())
}

/// Dzisiejszy brief (`None`, gdy nie ma pliku albo nie ustawiono Folderu).
#[tauri::command(async)]
fn brief_today(state: State<AppState>) -> Result<Option<brief::Brief>, String> {
    let s = state.settings.lock().unwrap().clone();
    let Some(dir) = s.brief_dir else { return Ok(None) };
    brief::find_today(&PathBuf::from(dir), s.vault.as_deref().map(std::path::Path::new))
}

#[tauri::command(async)]
fn brief_seen() -> bool {
    brief::seen_today()
}

#[tauri::command(async)]
fn brief_mark_seen() -> Result<(), String> {
    brief::mark_seen()
}

/// Z okna briefu do pełnej aplikacji (w tym samym oknie).
#[tauri::command(async)]
fn brief_open_main(app: AppHandle, state: State<AppState>) {
    state.brief_mode.store(false, Ordering::SeqCst);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_title("Notes Workbench");
    }
    fit_main_window(&app);
}

/// Otwiera plik briefu w domyślnym programie Windows.
#[tauri::command(async)]
fn brief_open_file(app: AppHandle, file: String) -> Result<(), String> {
    app.opener().open_path(file, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn brief_close(app: AppHandle) {
    app.exit(0);
}

// ===== Jarvis (docs/jarvis.md) =====

#[derive(Serialize)]
struct JarvisInfo {
    /// Katalog z `jarvis.py` (`None`: nie znaleziono).
    dir: Option<String>,
    dir_setting: Option<String>,
    python: Option<String>,
    python_setting: Option<String>,
    running: bool,
    autostart: bool,
}

fn jarvis_dir(state: &State<AppState>) -> Result<PathBuf, String> {
    let d = state.settings.lock().unwrap().jarvis_dir.clone();
    jarvis::resolve_dir(d.as_deref())
        .ok_or_else(|| "Nie znaleziono Jarvisa (jarvis.py). Wskaż jego katalog w zakładce „Konfiguruj Jarvisa”.".into())
}

fn jarvis_env(state: &State<AppState>) -> Result<(PathBuf, PathBuf), String> {
    let dir = jarvis_dir(state)?;
    let p = state.settings.lock().unwrap().jarvis_python.clone();
    let py = jarvis::resolve_python(p.as_deref())
        .ok_or("Nie znaleziono Pythona (python.exe). Wskaż go w zakładce „Konfiguruj Jarvisa”.")?;
    Ok((py, dir))
}

fn jarvis_tasks_file(dir: &std::path::Path) -> Result<PathBuf, String> {
    Ok(PathBuf::from(jarvis::read_config(dir)?.tasks_file))
}

#[tauri::command(async)]
fn jarvis_info(state: State<AppState>) -> JarvisInfo {
    let s = state.settings.lock().unwrap().clone();
    let dir = jarvis::resolve_dir(s.jarvis_dir.as_deref());
    JarvisInfo {
        running: dir.as_deref().is_some_and(jarvis::tracker_running),
        autostart: jarvis::autostart_enabled(),
        dir: dir.map(|d| d.to_string_lossy().to_string()),
        dir_setting: s.jarvis_dir,
        python: jarvis::resolve_python(s.jarvis_python.as_deref()).map(|p| p.to_string_lossy().to_string()),
        python_setting: s.jarvis_python,
    }
}

#[tauri::command(async)]
fn set_jarvis_paths(app: AppHandle, state: State<AppState>, dir: Option<String>, python: Option<String>) -> Result<(), String> {
    if let Some(d) = &dir {
        if jarvis::resolve_dir(Some(d)).is_none() {
            return Err("W tym katalogu nie ma pliku jarvis.py".into());
        }
    }
    if let Some(p) = &python {
        if !PathBuf::from(p).is_file() {
            return Err("Wskazany plik Pythona nie istnieje".into());
        }
    }
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.jarvis_dir = dir;
        s.jarvis_python = python;
        s.clone()
    };
    save_settings(&app, &s)
}

#[derive(Serialize)]
struct JarvisTasks {
    file: String,
    tasks: Vec<jarvis::Task>,
    focus: Option<String>,
    today: String,
}

#[tauri::command(async)]
fn jarvis_tasks(state: State<AppState>) -> Result<JarvisTasks, String> {
    let dir = jarvis_dir(&state)?;
    let file = jarvis_tasks_file(&dir)?;
    let today = brief::today();
    Ok(JarvisTasks {
        tasks: jarvis::read_tasks(&file)?,
        file: file.to_string_lossy().to_string(),
        focus: jarvis::focus(&dir, &today),
        today,
    })
}

/// Pełny stan dnia od Jarvisa (`status --json`): czas, kategorie, zadania z czasem, strażnik.
#[tauri::command(async)]
fn jarvis_status(state: State<AppState>, day: Option<String>) -> Result<serde_json::Value, String> {
    let (py, dir) = jarvis_env(&state)?;
    let mut args = vec!["status", "--json"];
    if let Some(d) = day.as_deref() {
        args.push(d);
    }
    let out = jarvis::run(&py, &dir, &args)?;
    serde_json::from_str(out.trim())
        .map_err(|e| format!("Nieczytelna odpowiedź Jarvisa ({e}). Czy jarvis.py ma komendę status --json?"))
}

#[tauri::command(async)]
fn jarvis_add_task(state: State<AppState>, text: String) -> Result<jarvis::Task, String> {
    jarvis::add_task(&jarvis_tasks_file(&jarvis_dir(&state)?)?, &text)
}

#[tauri::command(async)]
fn jarvis_update_task(state: State<AppState>, raw: String, task: jarvis::Task) -> Result<jarvis::Task, String> {
    jarvis::update_task(&jarvis_tasks_file(&jarvis_dir(&state)?)?, &raw, &task)
}

/// Fokus przez CLI, żeby Jarvis zapisał też wpis w `guard.jsonl`.
#[tauri::command(async)]
fn jarvis_set_focus(state: State<AppState>, name: String) -> Result<(), String> {
    let (py, dir) = jarvis_env(&state)?;
    let out = jarvis::run(&py, &dir, &["focus", &name])?;
    if out.contains("Nie znaleziono") {
        return Err(format!(
            "Jarvis nie ustawił fokusu: „{name}” nie jest otwartym zadaniem na dziś (fokus może być tylko zadanie z dzisiejszą datą, zaległe albo bez terminu)."
        ));
    }
    Ok(())
}

fn jarvis_wait_running(dir: &std::path::Path) {
    // Tracker zapisuje jarvis.pid zaraz po starcie; czekamy chwilę, żeby stan w oknie od razu był prawdziwy.
    for _ in 0..20 {
        if jarvis::tracker_running(dir) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

/// `start`, `stop`, `restart`, `install` (autostart), `uninstall`.
#[tauri::command(async)]
fn jarvis_control(state: State<AppState>, action: String) -> Result<String, String> {
    let (py, dir) = jarvis_env(&state)?;
    let steps: &[&str] = match action.as_str() {
        "start" => &["start"],
        "stop" => &["stop"],
        "restart" => &["stop", "start"],
        "install" => &["install"],
        "uninstall" => &["uninstall"],
        _ => return Err(format!("Nieznana akcja: {action}")),
    };
    let mut out = String::new();
    for s in steps {
        out += &jarvis::run(&py, &dir, &[s])?;
        if *s == "stop" {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    if steps.last() == Some(&"start") {
        jarvis_wait_running(&dir);
    }
    Ok(out.trim().to_string())
}

#[tauri::command(async)]
fn jarvis_config(state: State<AppState>) -> Result<jarvis::Config, String> {
    jarvis::read_config(&jarvis_dir(&state)?)
}

/// Zapisuje konfigurację; działający tracker restartuje, żeby ją wczytał. Zwraca, czy był restart.
#[tauri::command(async)]
fn jarvis_set_config(state: State<AppState>, config: jarvis::Config) -> Result<bool, String> {
    let (py, dir) = jarvis_env(&state)?;
    jarvis::write_config(&dir, &config)?;
    if !jarvis::tracker_running(&dir) {
        return Ok(false);
    }
    jarvis::run(&py, &dir, &["stop"])?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    jarvis::run(&py, &dir, &["start"])?;
    jarvis_wait_running(&dir);
    Ok(true)
}

/// Raport HTML dnia (`jarvis report <dzień> --open`) w przeglądarce.
#[tauri::command(async)]
fn jarvis_report(state: State<AppState>, day: Option<String>) -> Result<(), String> {
    let (py, dir) = jarvis_env(&state)?;
    let d = day.unwrap_or_else(brief::today);
    jarvis::run(&py, &dir, &["report", &d, "--open"]).map(|_| ())
}

/// Znacznik z interfejsu (np. „drzewo wyświetlone”) do dziennika startu.
#[tauri::command(async)]
fn start_mark(label: String) {
    startlog::mark(&format!("interfejs: {label}"));
}

pub fn run(brief_mode: bool) {
    startlog::begin();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .setup(move |app| {
            if let Ok(d) = app.path().app_local_data_dir() {
                startlog::init(d);
            }
            startlog::mark("setup: początek");
            if brief_mode {
                app.state::<AppState>().brief_mode.store(true, Ordering::SeqCst);
                show_brief_window(app.handle());
            } else {
                fit_main_window(app.handle());
            }
            startlog::mark("okno pokazane");
            if let Some(p) = settings_path(app.handle()) {
                if let Ok(s) = fs::read_to_string(p) {
                    if let Ok(s) = serde_json::from_str::<Settings>(&s) {
                        startlog::mark(&format!(
                            "ustawienia: Skarbiec {}, wyszukiwanie semantyczne {}",
                            s.vault.as_deref().unwrap_or("-"),
                            if s.semantic { "włączone" } else { "wyłączone" }
                        ));
                        *app.state::<AppState>().settings.lock().unwrap() = s;
                    }
                }
            }
            // Serwer MCP podłączony wcześniej: po aktualizacji aplikacji kopia serwera dostaje nową wersję
            // (gdy Claude akurat go używa, kopia zostaje i odświeży się przy następnym uruchomieniu).
            if mcp_setup::server_path().exists() {
                std::thread::spawn(|| {
                    let _ = startlog::timed("kopia serwera MCP: sprawdzenie", mcp_setup::install_server);
                });
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
            add_attachment,
            start_mark,
            get_vault,
            set_vault,
            import_amno,
            vault_tree,
            read_note,
            vault_index,
            search_notes,
            semantic_status,
            semantic_enable,
            semantic_activate,
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
            open_vault_file,
            reveal_vault,
            reveal_path,
            mcp_status,
            mcp_connect,
            mcp_disconnect,
            create_folder,
            rename_folder,
            move_note,
            move_folder,
            set_position,
            trash_folder,
            restore_folder,
            set_folder_noai,
            open_link,
            app_mode,
            brief_config,
            set_brief_dir,
            set_brief_autostart,
            brief_today,
            brief_seen,
            brief_mark_seen,
            brief_open_main,
            brief_open_file,
            brief_close,
            jarvis_info,
            set_jarvis_paths,
            jarvis_tasks,
            jarvis_status,
            jarvis_add_task,
            jarvis_update_task,
            jarvis_set_focus,
            jarvis_control,
            jarvis_config,
            jarvis_set_config,
            jarvis_report
        ])
        .run(tauri::generate_context!())
        .expect("błąd uruchomienia aplikacji");
}

pub const BRIEF_ARG: &str = brief::ARG;

/// Czy start z Harmonogramu ma pokazać okno briefu (sprawdzane przed uruchomieniem Tauri).
pub fn brief_should_show() -> bool {
    brief::should_show()
}
