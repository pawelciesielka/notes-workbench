//! Wyszukiwanie pełnotekstowe (SQLite FTS5 w pamięci) po tytułach, Folderach, Tagach i treści Notatek.
//! Indeks da się zawsze odtworzyć ze Skarbca, więc nie zapisujemy go na dysku; po zmianach
//! wczytujemy ponownie tylko pliki o nowej dacie modyfikacji.

use crate::index;
use crate::vault::{self, split_frontmatter};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Znaczniki początku i końca trafienia we fragmentach (frontend zamienia je na `<mark>`).
pub const HL_START: char = '\u{1}';
pub const HL_END: char = '\u{2}';

pub struct Search {
    conn: Connection,
    mtimes: HashMap<String, SystemTime>,
}

#[derive(Serialize)]
pub struct Hit {
    pub path: String,
    pub title: String,
    pub folder: String,
    pub snippet: String,
}

#[derive(Serialize)]
pub struct Results {
    pub hits: Vec<Hit>,
    /// `true`, gdy żadna Notatka nie zawiera wszystkich słów i pokazujemy te z częścią słów.
    pub partial: bool,
}

/// Tekst Notatki tak, jak widzi go czytelnik: bez składni Markdown i znaczników HTML.
pub fn plain_text(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS);
    let mut out = String::with_capacity(md.len());
    let mut html = String::new();
    let mut in_html = false;
    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::HtmlBlock) => {
                in_html = true;
                html.clear();
            }
            Event::End(TagEnd::HtmlBlock) => {
                in_html = false;
                // Komórki tabel HTML rozdzielamy, żeby słowa się nie sklejały.
                let spaced = html.replace("</td>", " </td>").replace("<br", " <br").replace("</p>", " </p>");
                out.push_str(&amno_import::verify::html_extract(&spaced, false).text);
                out.push('\n');
            }
            Event::Html(h) if in_html => html.push_str(&h),
            Event::Text(t) | Event::Code(t) => {
                if in_html {
                    html.push_str(&t)
                } else {
                    out.push_str(&t)
                }
            }
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock | TagEnd::TableCell) => {
                out.push('\n')
            }
            _ => {}
        }
    }
    out
}

/// Zapytanie użytkownika → wyrażenie FTS5. Słowa szukamy jako przedrostków (odmiana w polskim),
/// „fraza w cudzysłowie” dosłownie, `#tag` tylko w Tagach. `any` łączy słowa przez OR.
fn fts_query(q: &str, any: bool) -> Option<String> {
    let mut parts = Vec::new();
    let mut rest = q.trim();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('"') {
            let (phrase, after) = r.split_once('"').unwrap_or((r, ""));
            let words: Vec<_> = phrase.split_whitespace().collect();
            if !words.is_empty() {
                parts.push(format!("\"{}\"", words.join(" ").replace('"', "")));
            }
            rest = after.trim_start();
            continue;
        }
        let (tok, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        rest = after.trim_start();
        if let Some(tag) = tok.strip_prefix('#') {
            let t = tag.replace('"', "");
            if !t.is_empty() {
                parts.push(format!("tags : \"{t}\""));
            }
            continue;
        }
        // Znaki interpunkcji na brzegach słowa nie pomagają w szukaniu.
        let t: String = tok.trim_matches(|c: char| !c.is_alphanumeric()).replace('"', "");
        if !t.is_empty() {
            parts.push(format!("\"{t}\"*"));
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(if any { " OR " } else { " " }))
}

fn folder_of(path: &str) -> String {
    path.rfind('/').map_or(String::new(), |i| path[..i].replace('/', " / "))
}

impl Search {
    pub fn new() -> Search {
        let conn = Connection::open_in_memory().expect("SQLite w pamięci");
        conn.execute_batch(
            "CREATE VIRTUAL TABLE notes USING fts5(
                path UNINDEXED, title, folder, tags, body,
                tokenize = 'unicode61 remove_diacritics 2'
            );",
        )
        .expect("FTS5");
        Search { conn, mtimes: HashMap::new() }
    }

    fn put(&mut self, vault: &Path, path: &str) {
        let _ = self.conn.execute("DELETE FROM notes WHERE path = ?1", params![path]);
        let Ok(raw) = fs::read_to_string(vault.join(path)) else {
            self.mtimes.remove(path);
            return;
        };
        let (fm, body) = split_frontmatter(&raw);
        let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let title = vault::fm_value(fm, "title").filter(|t| !t.is_empty()).unwrap_or(stem);
        let tags = index::note_tags(path, &raw).join(" ");
        let _ = self.conn.execute(
            "INSERT INTO notes (path, title, folder, tags, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![path, title, folder_of(path), tags, plain_text(body)],
        );
        if let Ok(m) = fs::metadata(vault.join(path)).and_then(|m| m.modified()) {
            self.mtimes.insert(path.to_string(), m);
        }
    }

    pub fn remove(&mut self, path: &str) {
        let _ = self.conn.execute("DELETE FROM notes WHERE path = ?1", params![path]);
        self.mtimes.remove(path);
    }

    /// Jedna Notatka po zapisie, utworzeniu albo przeniesieniu (ścieżki w Koszu pomijamy).
    pub fn update(&mut self, vault: &Path, path: &str) {
        if path.starts_with(&format!("{}/", vault::TRASH_DIR)) {
            self.remove(path);
        } else {
            self.put(vault, path);
        }
    }

    /// Dopasowuje indeks do Skarbca: wczytuje nowe i zmienione pliki, usuwa zniknięte.
    pub fn sync(&mut self, vault: &Path) -> usize {
        let mut paths = Vec::new();
        index::collect(vault, vault, &mut paths);
        let mut changed = 0;
        let tx = self.conn.execute("BEGIN", []);
        let present: std::collections::HashSet<&String> = paths.iter().collect();
        let gone: Vec<String> = self.mtimes.keys().filter(|p| !present.contains(p)).cloned().collect();
        for p in gone {
            self.remove(&p);
            changed += 1;
        }
        for p in &paths {
            let m = fs::metadata(vault.join(p)).and_then(|m| m.modified()).ok();
            if m.is_none() || self.mtimes.get(p) != m.as_ref() {
                self.put(vault, p);
                changed += 1;
            }
        }
        if tx.is_ok() {
            let _ = self.conn.execute("COMMIT", []);
        }
        changed
    }

    fn run(&self, fts: &str, limit: usize) -> Result<Vec<Hit>, String> {
        let hl = |s: char| s.to_string();
        let sql = format!(
            "SELECT path, title, folder,
                    snippet(notes, 4, '{a}', '{b}', '…', 18)
             FROM notes WHERE notes MATCH ?1
             ORDER BY bm25(notes, 0.0, 10.0, 3.0, 5.0, 1.0)
             LIMIT ?2",
            a = hl(HL_START),
            b = hl(HL_END)
        );
        let mut st = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![fts, limit as i64], |r| {
                Ok(Hit { path: r.get(0)?, title: r.get(1)?, folder: r.get(2)?, snippet: r.get(3)? })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn search(&self, q: &str, limit: usize) -> Result<Results, String> {
        let Some(all) = fts_query(q, false) else { return Ok(Results { hits: Vec::new(), partial: false }) };
        let hits = self.run(&all, limit)?;
        if !hits.is_empty() {
            return Ok(Results { hits, partial: false });
        }
        match fts_query(q, true) {
            Some(any) if any != all => Ok(Results { hits: self.run(&any, limit)?, partial: true }),
            _ => Ok(Results { hits, partial: false }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_queries() {
        assert_eq!(fts_query("serwis ubuntu", false).unwrap(), "\"serwis\"* \"ubuntu\"*");
        assert_eq!(fts_query("\"stały IP\" #linux, x\"y", true).unwrap(), "\"stały IP\" OR tags : \"linux,\" OR \"xy\"*");
        assert!(fts_query("  ,, ", false).is_none());
    }

    /// `NW_VAULT=C:\Notatki cargo test --release real_search -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_search() {
        let v = std::env::var("NW_VAULT").unwrap();
        let t = std::time::Instant::now();
        let mut s = Search::new();
        let n = s.sync(Path::new(&v));
        println!("zaindeksowano {n} w {:?}; ponowna synchronizacja: {} zmian", t.elapsed(), s.sync(Path::new(&v)));
        for q in ["usługa systemd", "certyfikat", "docker compose", "\"stały adres\"", "kopia zapasowa"] {
            let t = std::time::Instant::now();
            let r = s.search(q, 5).unwrap();
            println!("\n{q}: {} trafień{} ({:?})", r.hits.len(), if r.partial { " (częściowe)" } else { "" }, t.elapsed());
            for h in r.hits.iter().take(3) {
                println!("  {} | {}", h.title, h.snippet.replace(['\u{1}', '\u{2}'], "*").replace('\n', " "));
            }
        }
    }

    #[test]
    fn plain_text_keeps_words_apart() {
        let t = plain_text("**Akapit** pierwszy\n\nDrugi `kod`\n\n<table><tr><td>komórka</td><td>druga</td></tr></table>\n");
        assert!(t.contains("Akapit pierwszy\nDrugi kod\n"), "{t}");
        assert!(t.contains("komórka") && !t.contains("komórkadruga"), "{t}");
    }

    #[test]
    fn finds_with_inflection_and_diacritics() {
        let dir = std::env::temp_dir().join(format!("nw-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Linux")).unwrap();
        fs::write(dir.join("Linux/Serwis.md"), "---\ntitle: \"Instalacja serwisu\"\n---\n\nUruchomienie usługi w systemd. #admin\n").unwrap();
        fs::write(dir.join("Inna.md"), "Zupełnie inna treść o kotach.\n").unwrap();
        let mut s = Search::new();
        assert_eq!(s.sync(&dir), 2);
        assert_eq!(s.sync(&dir), 0);
        let r = s.search("uslug systemd", 10).unwrap();
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].title, "Instalacja serwisu");
        assert!(r.hits[0].snippet.contains('\u{1}'));
        assert_eq!(s.search("serwis", 10).unwrap().hits[0].path, "Linux/Serwis.md");
        assert_eq!(s.search("#admin", 10).unwrap().hits.len(), 1);
        let p = s.search("kot systemd", 10).unwrap();
        assert!(p.partial && p.hits.len() == 2);
        fs::remove_file(dir.join("Inna.md")).unwrap();
        assert_eq!(s.sync(&dir), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
