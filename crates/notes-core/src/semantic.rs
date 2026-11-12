//! Wyszukiwanie semantyczne: lokalny model wielojęzyczny (multilingual-e5-small, ONNX) zamienia
//! fragmenty Notatek na wektory. Model pobiera się raz do katalogu danych aplikacji; potem wszystko
//! działa bez sieci, a treść Notatek nie opuszcza komputera.
//!
//! Wektory to pamięć podręczna odtwarzalna ze Skarbca, więc trzymamy je poza Skarbcem
//! (w katalogu danych aplikacji, osobno dla każdego Skarbca) i przeliczamy tylko zmienione Notatki.

use crate::index;
use crate::search::plain_text;
use crate::vault::{self, split_frontmatter};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MODEL_NAME: &str = "multilingual-e5-small";
/// Przybliżony rozmiar pobieranego modelu (do komunikatu w interfejsie).
pub const MODEL_MB: u32 = 470;
const CHUNK_CHARS: usize = 900;
const BATCH: usize = 16;

struct Chunk {
    path: String,
    text: String,
    vec: Vec<f32>,
}

struct Store {
    vault: PathBuf,
    conn: Connection,
    mtimes: HashMap<String, i64>,
    chunks: Vec<Chunk>,
}

#[derive(Serialize, Clone, Default)]
pub struct Status {
    pub enabled: bool,
    pub model_ready: bool,
    pub running: bool,
    /// Co robi zadanie w tle (pobieranie modelu, liczenie wektorów).
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub notes: usize,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SemHit {
    pub path: String,
    pub title: String,
    pub folder: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Default)]
pub struct Semantic {
    model: Mutex<Option<TextEmbedding>>,
    store: Mutex<Option<Store>>,
    pub status: Mutex<Status>,
}

fn mtime(p: &Path) -> Option<i64> {
    let m = fs::metadata(p).and_then(|m| m.modified()).ok()?;
    Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Treść Notatki pocięta na fragmenty po akapitach (model widzi najwyżej ~512 tokenów).
pub fn chunks(title: &str, body: &str) -> Vec<String> {
    let text = plain_text(body);
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let mut line = line;
        while line.chars().count() > CHUNK_CHARS {
            let cut = line.char_indices().nth(CHUNK_CHARS).map_or(line.len(), |(i, _)| i);
            // Nie tniemy w środku słowa, jeśli da się tego uniknąć.
            let cut = line[..cut].rfind(' ').filter(|&i| i > CHUNK_CHARS / 2).unwrap_or(cut);
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(line[..cut].to_string());
            line = line[cut..].trim_start();
        }
        if cur.chars().count() + line.chars().count() > CHUNK_CHARS && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    // Tytuł przy każdym fragmencie: bez niego fragment ze środka Notatki traci kontekst.
    out.into_iter().map(|c| if c.is_empty() { title.to_string() } else { format!("{title}\n{c}") }).collect()
}

impl Store {
    /// `readonly` (serwer MCP): plik otwierany tylko do odczytu, bez tworzenia tabel; zapisuje wyłącznie aplikacja.
    fn open(db: &Path, vault: &Path, readonly: bool) -> Result<Store, String> {
        let conn = if readonly {
            Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?
        } else {
            if let Some(d) = db.parent() {
                fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            let conn = Connection::open(db).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE IF NOT EXISTS notes (path TEXT PRIMARY KEY, mtime INTEGER);
                 CREATE TABLE IF NOT EXISTS chunks (path TEXT, idx INTEGER, text TEXT, vec BLOB);
                 CREATE INDEX IF NOT EXISTS chunks_path ON chunks(path);",
            )
            .map_err(|e| e.to_string())?;
            conn
        };
        // Wektory innego modelu są bezużyteczne: zaczynamy od zera.
        let model: Option<String> = conn.query_row("SELECT value FROM meta WHERE key='model'", [], |r| r.get(0)).ok();
        if readonly && model.as_deref() != Some(MODEL_NAME) {
            return Err("Wektory policzono innym modelem: otwórz Skarbiec w aplikacji, żeby je przeliczyła.".into());
        }
        if model.as_deref() != Some(MODEL_NAME) {
            conn.execute_batch("DELETE FROM notes; DELETE FROM chunks;").map_err(|e| e.to_string())?;
            conn.execute("INSERT OR REPLACE INTO meta VALUES ('model', ?1)", params![MODEL_NAME])
                .map_err(|e| e.to_string())?;
        }
        let mut mtimes = HashMap::new();
        {
            let mut st = conn.prepare("SELECT path, mtime FROM notes").map_err(|e| e.to_string())?;
            for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).map_err(|e| e.to_string())? {
                let (p, m) = r.map_err(|e| e.to_string())?;
                mtimes.insert(p, m);
            }
        }
        let mut chunks = Vec::new();
        {
            let mut st = conn.prepare("SELECT path, text, vec FROM chunks ORDER BY path, idx").map_err(|e| e.to_string())?;
            for r in st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))
                .map_err(|e| e.to_string())?
            {
                let (path, text, vec) = r.map_err(|e| e.to_string())?;
                chunks.push(Chunk { path, text, vec: from_blob(&vec) });
            }
        }
        Ok(Store { vault: vault.to_path_buf(), conn, mtimes, chunks })
    }

    fn remove(&mut self, path: &str) {
        let _ = self.conn.execute("DELETE FROM chunks WHERE path=?1", params![path]);
        let _ = self.conn.execute("DELETE FROM notes WHERE path=?1", params![path]);
        self.mtimes.remove(path);
        self.chunks.retain(|c| c.path != path);
    }

    fn put(&mut self, path: &str, m: i64, items: Vec<(String, Vec<f32>)>) -> Result<(), String> {
        self.remove(path);
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        for (i, (text, vec)) in items.iter().enumerate() {
            tx.execute("INSERT INTO chunks VALUES (?1, ?2, ?3, ?4)", params![path, i as i64, text, to_blob(vec)])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT OR REPLACE INTO notes VALUES (?1, ?2)", params![path, m]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.mtimes.insert(path.to_string(), m);
        self.chunks.extend(items.into_iter().map(|(text, vec)| Chunk { path: path.to_string(), text, vec }));
        Ok(())
    }
}

/// Plik z wektorami danego Skarbca w katalogu danych aplikacji.
fn db_path(data_dir: &Path, vault: &Path) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    vault.to_string_lossy().to_lowercase().hash(&mut h);
    data_dir.join("wektory").join(format!("{:016x}.db", h.finish()))
}

impl Semantic {
    pub fn status(&self) -> Status {
        let mut s = self.status.lock().unwrap().clone();
        s.model_ready = self.model.lock().unwrap().is_some();
        s.notes = self.store.lock().unwrap().as_ref().map_or(0, |st| st.mtimes.len());
        s
    }

    fn set(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.status.lock().unwrap());
    }

    /// Zapomina wektory poprzedniego Skarbca (po zmianie Skarbca).
    pub fn reset_store(&self) {
        *self.store.lock().unwrap() = None;
    }

    /// Czy model jest już pobrany (wtedy wczytanie nie wymaga sieci).
    pub fn model_cached(data_dir: &Path) -> bool {
        let cache = data_dir.join("modele");
        fs::read_dir(&cache).map(|mut d| d.next().is_some()).unwrap_or(false)
    }

    /// Tylko do odczytu (serwer MCP): wczytuje pobrany model i zapisane wektory, niczego nie przelicza.
    pub fn load_readonly(&self, data_dir: &Path, vault: &Path) -> Result<(), String> {
        if !Self::model_cached(data_dir) {
            return Err("Wyszukiwanie po znaczeniu nie jest włączone: włącz je w aplikacji Notes Workbench (Szukaj → Znaczenie).".into());
        }
        self.ensure_model(data_dir, &|_| {})?;
        let mut g = self.store.lock().unwrap();
        if g.as_ref().map(|s| s.vault.as_path()) != Some(vault) {
            let db = db_path(data_dir, vault);
            if !db.exists() {
                return Err("Brak wektorów dla tego Skarbca: otwórz go w aplikacji z włączonym wyszukiwaniem po znaczeniu.".into());
            }
            *g = Some(Store::open(&db, vault, true)?);
        }
        Ok(())
    }

    /// Wczytuje zapisane wektory Skarbca bez modelu (wystarczą do Notatek podobnych).
    /// Nie tworzy bazy, gdy jej jeszcze nie ma.
    pub fn open_store(&self, data_dir: &Path, vault: &Path) -> Result<(), String> {
        let mut g = self.store.lock().unwrap();
        if g.as_ref().map(|s| s.vault.as_path()) != Some(vault) {
            let db = db_path(data_dir, vault);
            if db.exists() {
                *g = Some(Store::open(&db, vault, false)?);
            }
        }
        Ok(())
    }

    fn ensure_model(&self, data_dir: &Path, report: &dyn Fn(&Status)) -> Result<(), String> {
        if self.model.lock().unwrap().is_some() {
            return Ok(());
        }
        let cache = data_dir.join("modele");
        let downloaded = cache.exists() && fs::read_dir(&cache).map(|mut d| d.next().is_some()).unwrap_or(false);
        self.set(|s| {
            s.phase = if downloaded {
                "Wczytywanie modelu…".into()
            } else {
                format!("Pobieranie modelu (ok. {MODEL_MB} MB, jednorazowo)…")
            }
        });
        report(&self.status());
        let m = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
                .with_cache_dir(cache)
                .with_show_download_progress(false),
        )
        .map_err(|e| format!("Nie udało się przygotować modelu: {e}"))?;
        *self.model.lock().unwrap() = Some(m);
        Ok(())
    }

    fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        let mut g = self.model.lock().unwrap();
        let m = g.as_mut().ok_or("Model nie jest wczytany")?;
        let v = m.embed(texts, Some(BATCH)).map_err(|e| e.to_string())?;
        Ok(v.into_iter().map(normalize).collect())
    }

    /// Zadanie w tle: model, a potem wektory nowych i zmienionych Notatek. `report` dostaje postęp.
    pub fn run(&self, data_dir: &Path, vault: &Path, report: &dyn Fn(&Status)) -> Result<(), String> {
        self.ensure_model(data_dir, report)?;
        {
            let mut g = self.store.lock().unwrap();
            if g.as_ref().map(|s| s.vault.as_path()) != Some(vault) {
                *g = Some(Store::open(&db_path(data_dir, vault), vault, false)?);
            }
        }
        let mut paths = Vec::new();
        index::collect(vault, vault, &mut paths);
        let todo: Vec<(String, i64)> = {
            let mut g = self.store.lock().unwrap();
            let st = g.as_mut().unwrap();
            let present: HashSet<&String> = paths.iter().collect();
            let gone: Vec<String> = st.mtimes.keys().filter(|p| !present.contains(p)).cloned().collect();
            for p in gone {
                st.remove(&p);
            }
            paths
                .iter()
                .filter_map(|p| {
                    let m = mtime(&vault.join(p))?;
                    (st.mtimes.get(p) != Some(&m)).then(|| (p.clone(), m))
                })
                .collect()
        };
        let total = todo.len();
        self.set(|s| {
            s.phase = "Liczenie wektorów Notatek…".into();
            s.done = 0;
            s.total = total;
        });
        report(&self.status());
        for (i, (path, m)) in todo.into_iter().enumerate() {
            let Ok(raw) = fs::read_to_string(vault.join(&path)) else { continue };
            let (fm, body) = split_frontmatter(&raw);
            let stem = Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let title = vault::fm_value(fm, "title").filter(|t| !t.is_empty()).unwrap_or(stem);
            let parts = chunks(&title, body);
            let vecs = self.embed(parts.iter().map(|c| format!("passage: {c}")).collect())?;
            {
                let mut g = self.store.lock().unwrap();
                // Skarbiec mógł się zmienić w trakcie: wtedy przerywamy.
                match g.as_mut() {
                    Some(st) if st.vault == vault => st.put(&path, m, parts.into_iter().zip(vecs).collect())?,
                    _ => return Ok(()),
                }
            }
            self.set(|s| s.done = i + 1);
            if (i + 1) % 5 == 0 || i + 1 == total {
                report(&self.status());
            }
        }
        self.set(|s| s.phase = String::new());
        Ok(())
    }

    /// Notatki najbliższe znaczeniowo zapytaniu: najlepszy fragment każdej Notatki.
    pub fn search(&self, query: &str, limit: usize, titles: &dyn Fn(&str) -> Option<String>) -> Result<Vec<SemHit>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let qv = self.embed(vec![format!("query: {q}")])?.remove(0);
        let g = self.store.lock().unwrap();
        let Some(st) = g.as_ref() else { return Ok(Vec::new()) };
        let mut best: HashMap<&str, (f32, &str)> = HashMap::new();
        for c in &st.chunks {
            let s = dot(&qv, &c.vec);
            let e = best.entry(c.path.as_str()).or_insert((f32::MIN, ""));
            if s > e.0 {
                *e = (s, c.text.as_str());
            }
        }
        let mut v: Vec<_> = best.into_iter().collect();
        v.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
        Ok(v.into_iter()
            .take(limit)
            .filter_map(|(path, (score, text))| {
                let title = titles(path)?;
                // Fragment bez powtórzonego tytułu na początku.
                let snippet = text.strip_prefix(title.as_str()).unwrap_or(text).trim();
                let snippet: String = snippet.chars().take(220).collect();
                Some(SemHit {
                    folder: path.rfind('/').map_or(String::new(), |i| path[..i].replace('/', " / ")),
                    path: path.to_string(),
                    title,
                    snippet: snippet.replace('\n', " "),
                    score,
                })
            })
            .collect())
    }

    /// Notatki podobne treścią do danej (średni wektor jej fragmentów).
    pub fn similar(&self, path: &str, limit: usize) -> Vec<(String, f32)> {
        let g = self.store.lock().unwrap();
        let Some(st) = g.as_ref() else { return Vec::new() };
        let own: Vec<&Chunk> = st.chunks.iter().filter(|c| c.path == path).collect();
        let Some(first) = own.first() else { return Vec::new() };
        let mut mean = vec![0f32; first.vec.len()];
        for c in &own {
            mean.iter_mut().zip(&c.vec).for_each(|(m, x)| *m += x);
        }
        let mean = normalize(mean);
        let mut best: HashMap<&str, f32> = HashMap::new();
        for c in st.chunks.iter().filter(|c| c.path != path) {
            let s = dot(&mean, &c.vec);
            let e = best.entry(c.path.as_str()).or_insert(f32::MIN);
            if s > *e {
                *e = s;
            }
        }
        let mut v: Vec<_> = best.into_iter().map(|(p, s)| (p.to_string(), s)).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(limit);
        v
    }
}

/// Wyszukiwanie hybrydowe: łączy miejsca w rankingu po znaczeniu i po słowach (Reciprocal Rank Fusion),
/// bo sam model myli się przy nazwach własnych i skrótach, a same słowa nie łapią synonimów.
pub fn fuse(sem: Vec<SemHit>, words: Vec<crate::search::Hit>, limit: usize) -> Vec<SemHit> {
    let mut fused: HashMap<String, (f32, SemHit)> = HashMap::new();
    for (rank, h) in sem.into_iter().enumerate() {
        fused.insert(h.path.clone(), (1.0 / (60.0 + rank as f32), h));
    }
    for (rank, h) in words.into_iter().enumerate() {
        let w = 1.0 / (60.0 + rank as f32);
        match fused.get_mut(&h.path) {
            Some((score, hit)) => {
                *score += w;
                // Fragment z wyszukiwania po słowach ma zaznaczone trafienia.
                hit.snippet = h.snippet;
            }
            None => {
                let hit = SemHit { path: h.path.clone(), title: h.title, folder: h.folder, snippet: h.snippet, score: 0.0 };
                fused.insert(h.path, (w, hit));
            }
        }
    }
    let mut v: Vec<_> = fused.into_values().collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter()
        .take(limit)
        .map(|(s, mut h)| {
            h.score = s;
            h
        })
        .collect()
}

/// Próg dla Notatek podobnych, dobrany na Skarbcu z AMNO: od ~0,92 wyniki są wyraźnie tematyczne,
/// poniżej 0,90 to przypadek (mediana podobieństwa najbliższej Notatki to 0,91).
pub const SIMILAR_MIN: f32 = 0.915;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_by_paragraphs_with_title() {
        let long = "słowo ".repeat(400);
        let c = chunks("Tytuł", &format!("Pierwszy akapit.\n\nDrugi akapit.\n\n{long}"));
        assert!(c[0].starts_with("Tytuł\nPierwszy akapit.\nDrugi akapit."), "{:?}", c[0]);
        assert!(c.len() >= 3);
        assert!(c.iter().all(|x| x.chars().count() <= CHUNK_CHARS + 30));
        assert_eq!(chunks("Pusta", ""), vec!["Pusta"]);
    }

    /// `NW_VAULT=C:\Notatki NW_DATA=%LOCALAPPDATA%\eu.ciesielka.notesworkbench cargo test --release real_semantic -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_semantic() {
        let vault = PathBuf::from(std::env::var("NW_VAULT").unwrap());
        let data = PathBuf::from(std::env::var("NW_DATA").unwrap());
        let s = Semantic::default();
        let t = std::time::Instant::now();
        s.run(&data, &vault, &|st| eprintln!("  {} {}/{}", st.phase, st.done, st.total)).unwrap();
        println!("gotowe w {:?}, Notatek {}", t.elapsed(), s.status().notes);
        let mut idx = crate::index::Index::build(&vault);
        let titles = |p: &str| idx.title(p);
        for q in ["problemy z siecią i VPN", "jak zainstalować usługę na serwerze linux", "spotkanie z dostawcą", "hasło do bazy danych", "dzieci i edukacja"] {
            let t = std::time::Instant::now();
            let r = s.search(q, 4, &titles).unwrap();
            println!("\n{q} ({:?})", t.elapsed());
            for h in r {
                println!("  {:.3} {} | {}", h.score, h.title, h.snippet.chars().take(90).collect::<String>());
            }
        }
        let first = idx.summary().notes.into_iter().find(|n| n.title.contains("ubuntu")).unwrap();
        println!("\npodobne do {}:", first.title);
        for (p, sc) in s.similar(&first.path, 5) {
            println!("  {sc:.3} {}", idx.title(&p).unwrap_or_default());
        }
        idx.remove("");
    }

    /// Rozkład podobieństwa najbliższej Notatki (do doboru progu „Podobne treścią”).
    #[test]
    #[ignore]
    fn real_similarity_distribution() {
        let vault = PathBuf::from(std::env::var("NW_VAULT").unwrap());
        let data = PathBuf::from(std::env::var("NW_DATA").unwrap());
        let s = Semantic::default();
        *s.store.lock().unwrap() = Some(Store::open(&db_path(&data, &vault), &vault, true).unwrap());
        let paths: Vec<String> = s.store.lock().unwrap().as_ref().unwrap().mtimes.keys().cloned().collect();
        let mut tops: Vec<f32> = paths.iter().take(300).filter_map(|p| s.similar(p, 1).first().map(|x| x.1)).collect();
        tops.sort_by(|a, b| a.total_cmp(b));
        for q in [0.1, 0.25, 0.5, 0.75, 0.9] {
            println!("p{:.0}: {:.3}", q * 100.0, tops[(q * tops.len() as f64) as usize]);
        }
        let idx = crate::index::Index::build(&vault);
        for p in paths.iter().take(6) {
            println!("\n{}:", idx.title(p).unwrap_or_default());
            for (q, sc) in s.similar(p, 4) {
                println!("  {sc:.3} {}", idx.title(&q).unwrap_or_default());
            }
        }
    }

    #[test]
    fn blob_roundtrip() {
        let v = vec![0.5f32, -1.25, 3.0];
        assert_eq!(from_blob(&to_blob(&v)), v);
    }
}
