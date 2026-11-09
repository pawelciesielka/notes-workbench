//! Dzieli Eksport AMNO na wpisy (Folder albo Notatka) z surowym HTML treści.
//!
//! Eksport to jeden plik, w którym drzewo jest spłaszczone: każdy element zaczyna się
//! linią nagłówka `[Note] tytuł, path: My Data\A\tytuł (depth: N)` (Foldery bez `[Note]`),
//! a po niej idą linie treści aż do następnego nagłówka. HTML bywa niepoprawny
//! (niedomknięte komórki tabel), dlatego treść każdej Notatki jest parsowana osobno.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Note,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: Kind,
    /// Tytuł z nagłówka (bez przycinania).
    pub title: String,
    /// Ścieżka AMNO, np. `My Data\Projekty\Strona WWW`.
    pub path: String,
    pub depth: usize,
    /// Surowy HTML treści (linie między nagłówkami).
    pub body_html: String,
    /// Numer linii nagłówka w eksporcie (od 1), do raportu.
    pub line: usize,
}

const CORBEL: &str = "<span style=\"font-family:'Corbel';font-size:12pt;\">";
const EMPTY_CORBEL: &str = "<span style=\"font-family:'Corbel';font-size:12pt;\"></span><br />";

fn header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)^(\[Note\] ?)?(.*), path: (My Data.*) \(depth: (\d+)\)$").unwrap()
    })
}

fn tags_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]*>").unwrap())
}

/// Tekst linii HTML: bez znaczników, z rozkodowanymi encjami.
pub fn line_text(line: &str) -> String {
    let stripped = tags_re().replace_all(line, "");
    html_escape::decode_html_entities(&stripped).into_owned()
}

struct Header {
    note: bool,
    title: String,
    path: String,
    depth: usize,
}

fn parse_header(line: &str) -> Option<Header> {
    let trimmed = line.trim_start();
    // Nagłówek zawsze zaczyna się spanem Corbel i kończy liczbą głębokości.
    if !trimmed.starts_with(CORBEL) || !line.contains("depth&#58;") {
        return None;
    }
    let text = line_text(line);
    let text = text.trim();
    let caps = header_re().captures(text)?;
    Some(Header {
        note: caps.get(1).is_some(),
        title: caps[2].to_string(),
        path: caps[3].to_string(),
        depth: caps[4].parse().ok()?,
    })
}

pub fn split_entries(html: &str) -> Vec<Entry> {
    let lines: Vec<&str> = html.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut headers: Vec<(usize, Header)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(h) = parse_header(line) {
            headers.push((i, h));
        }
    }
    let mut entries = Vec::with_capacity(headers.len());
    let mut stack: Vec<String> = Vec::new(); // ścieżki AMNO wg głębokości
    for (n, (i, h)) in headers.iter().enumerate() {
        let end = headers.get(n + 1).map(|(j, _)| *j).unwrap_or(lines.len());
        let mut body: &[&str] = &lines[i + 1..end];
        while body.first().is_some_and(|l| l.trim() == EMPTY_CORBEL) {
            body = &body[1..];
        }
        while body.last().is_some_and(|l| {
            let t = l.trim();
            t == EMPTY_CORBEL || t.is_empty() || t == "</html>"
        }) {
            body = &body[..body.len() - 1];
        }
        // Tytuł bierzemy z ścieżki (ścieżka rodzica + "\" + tytuł), bo tytuł może zawierać ", path: ".
        stack.truncate(h.depth.saturating_sub(1));
        let title = match stack.last() {
            Some(parent) => h
                .path
                .strip_prefix(&format!("{parent}\\"))
                .map(str::to_string)
                .unwrap_or_else(|| h.title.clone()),
            None => h.title.clone(),
        };
        stack.push(h.path.clone());
        entries.push(Entry {
            kind: if h.note { Kind::Note } else { Kind::Folder },
            title,
            path: h.path.clone(),
            depth: h.depth,
            body_html: body.join("\n"),
            line: i + 1,
        });
    }
    entries
}
