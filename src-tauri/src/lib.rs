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
    let v = state.vault()?;
    let label = if trash { "drzewo Kosza" } else { "drzewo Notatek" };
    Ok(startlog::timed(label, || vault::tree(&v, trash)))
}

#[tauri::command]
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

/// Załącznik Notatki: treść w surowym ciele żądania, Notatka i nazwa w nagłówkach (zakodowane URI).
#[tauri::command]
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
#[tauri::command]
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
    startlog::mark("raport importu: wczytywanie");
    let p = state.vault()?.join(vault::APP_DIR).join("import").join("raport.json");
    match fs::read_to_string(p) {
        Ok(s) => serde_json::from_str(&s).map(Some).map_err(|e| e.to_string()),
        Err(_) => Ok(None),
    }
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

#[tauri::command]
fn create_folder(app: AppHandle, state: State<AppState>, parent: String, name: String) -> Result<String, String> {
    let p = notes_core::ops::create_folder(&state.vault()?, &parent, &name)?;
    start_semantic(&app);
    Ok(p)
}

#[tauri::command]
fn rename_folder(app: AppHandle, state: State<AppState>, path: String, name: String) -> Result<OpResult, String> {
    let vault = state.vault()?;
    let (p, fixed) = state.with_index(|idx, _| notes_core::ops::rename_folder(&vault, idx, &path, &name))??;
    state.reindex()?;
    start_semantic(&app);
    Ok(OpResult { path: p, fixed })
}

#[tauri::command]
fn move_note(app: AppHandle, state: State<AppState>, path: String, folder: String) -> Result<OpResult, String> {
    let vault = state.vault()?;
    let (p, fixed) = state.with_index(|idx, _| notes_core::ops::move_note(&vault, idx, &path, &folder))??;
    state.reindex()?;
    start_semantic(&app);
    Ok(OpResult { path: p, fixed })
}

#[tauri::command]
fn trash_folder(state: State<AppState>, path: String) -> Result<String, String> {
    let p = notes_core::ops::trash_folder(&state.vault()?, &path)?;
    state.reindex()?;
    Ok(p)
}

#[tauri::command]
fn restore_folder(app: AppHandle, state: State<AppState>, path: String) -> Result<String, String> {
    let p = notes_core::ops::restore_folder(&state.vault()?, &path)?;
    state.reindex()?;
    start_semantic(&app);
    Ok(p)
}

#[tauri::command]
fn set_folder_noai(state: State<AppState>, path: String, on: bool) -> Result<String, String> {
    let p = notes_core::ops::set_folder_noai(&state.vault()?, &path, on)?;
    state.reindex()?;
    Ok(p)
}

/// Pokazuje plik albo katalog ze Skarbca zaznaczony w Eksploratorze.
#[tauri::command]
fn reveal_path(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let p = vault::resolve(&state.vault()?, &path)?;
    app.opener().reveal_item_in_dir(&p).map_err(|e| e.to_string())
}

#[tauri::command]
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
#[tauri::command]
fn reveal_vault(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let v = state.vault()?;
    app.opener().open_path(v.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
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

/// Dopasowuje okno główne do obszaru roboczego monitora (bez paska zadań) i ustawia je na środku.
/// Domyślny rozmiar z konfiguracji (1400×900) na ekranie ze skalowaniem bywa wyższy niż ekran,
/// a wtedy dół okna chowa się pod paskiem zadań.
fn fit_main_window(app: &tauri::App) {
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

/// Znacznik z interfejsu (np. „drzewo wyświetlone”) do dziennika startu.
#[tauri::command]
fn start_mark(label: String) {
    startlog::mark(&format!("interfejs: {label}"));
}

pub fn run() {
    startlog::begin();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Ok(d) = app.path().app_local_data_dir() {
                startlog::init(d);
            }
            startlog::mark("setup: początek");
            fit_main_window(app);
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
            reveal_vault,
            reveal_path,
            mcp_status,
            mcp_connect,
            mcp_disconnect,
            create_folder,
            rename_folder,
            move_note,
            trash_folder,
            restore_folder,
            set_folder_noai,
            open_link
        ])
        .run(tauri::generate_context!())
        .expect("błąd uruchomienia aplikacji");
}
