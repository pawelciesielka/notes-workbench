//! Indeks Grafu wiedzy: Linki `[[…]]`, Backlinki i Tagi wszystkich Notatek poza Koszem.
//! Indeks da się zawsze odtworzyć ze Skarbca; trzymamy go tylko w pamięci.

use crate::vault::{self, split_frontmatter, APP_DIR, TRASH_DIR};
use pulldown_cmark::{Event, Options, Parser, Tag};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Wystąpienie Linku albo Tagu w treści Notatki.
#[derive(Serialize, Clone, Debug)]
pub struct Span {
    /// Pozycja w treści (bez frontmattera) w jednostkach UTF-16, jak w JavaScripcie.
    pub start: usize,
    pub end: usize,
    /// Dla Linku: cel bez aliasu i nagłówka; dla Tagu: nazwa bez `#`.
    pub target: String,
    /// Tekst do wyświetlenia (alias albo cel).
    pub label: String,
    pub kind: &'static str,
    /// Ścieżka Notatki docelowej, gdy Link da się rozwiązać.
    pub path: Option<String>,
}

#[derive(Clone, Debug)]
struct RawLink {
    target: String,
    line: String,
    from_frontmatter: bool,
}

#[derive(Clone, Debug)]
pub struct NoteInfo {
    pub path: String,
    pub stem: String,
    pub title: String,
    tags: Vec<String>,
    links: Vec<RawLink>,
}

#[derive(Default)]
pub struct Index {
    notes: BTreeMap<String, NoteInfo>,
    by_stem: HashMap<String, Vec<String>>,
}

#[derive(Serialize)]
pub struct NoteRef {
    pub path: String,
    pub title: String,
    pub stem: String,
}

#[derive(Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct Summary {
    pub notes: Vec<NoteRef>,
    pub tags: Vec<TagCount>,
}

#[derive(Serialize)]
pub struct Backlink {
    pub path: String,
    pub title: String,
    /// Linie z Linkiem (kontekst); pusta lista, gdy to Powiązanie z frontmattera.
    pub lines: Vec<String>,
    pub related: bool,
}

#[derive(Serialize)]
pub struct NoteLinks {
    pub spans: Vec<Span>,
    pub tags: Vec<String>,
    pub related: Vec<NoteRef>,
    pub backlinks: Vec<Backlink>,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: &'static str,
}

#[derive(Serialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<(String, String)>,
}

static WIKI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\[\]\n]+?)\]\]").unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([\p{L}\p{N}_/-]+)").unwrap());

/// Klucz Tagu: bez wielkości liter, jak w Obsidianie.
pub fn tag_key(t: &str) -> String {
    t.to_lowercase()
}

/// Treść z wymazanymi (spacjami) fragmentami, w których nie szukamy Linków i Tagów:
/// kod, bloki HTML (tabele z Importu) i znaczniki HTML.
fn masked(body: &str) -> String {
    let mut bytes = body.as_bytes().to_vec();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS);
    for (ev, range) in Parser::new_ext(body, opts).into_offset_iter() {
        let hide = matches!(
            ev,
            Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) | Event::Start(Tag::CodeBlock(_))
        );
        if hide {
            for b in &mut bytes[range] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
        }
    }
    // Zastępujemy tylko całe znaki (zakresy parsera są na granicach znaków), więc to wciąż UTF-8.
    String::from_utf8(bytes).unwrap_or_else(|_| body.to_string())
}

fn escaped(s: &str, pos: usize) -> bool {
    s[..pos].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

fn line_at(body: &str, pos: usize) -> String {
    let start = body[..pos].rfind('\n').map_or(0, |i| i + 1);
    let end = body[pos..].find('\n').map_or(body.len(), |i| pos + i);
    let line = body[start..end].trim();
    if line.chars().count() > 240 {
        line.chars().take(240).collect::<String>() + "…"
    } else {
        line.to_string()
    }
}

/// `cel#nagłówek|alias` → (cel, etykieta).
fn split_wiki(inner: &str) -> (String, String) {
    let (t, alias) = match inner.split_once('|') {
        Some((t, a)) => (t, Some(a.trim())),
        None => (inner, None),
    };
    let target = t.split('#').next().unwrap_or("").trim().to_string();
    let label = alias.filter(|a| !a.is_empty()).unwrap_or(t.trim()).to_string();
    (target, label)
}

struct Found {
    start: usize,
    end: usize,
    target: String,
    label: String,
    kind: &'static str,
}

/// Linki i Tagi w treści (pozycje w bajtach).
fn scan(body: &str) -> Vec<Found> {
    let m = masked(body);
    let mut out = Vec::new();
    for c in WIKI.captures_iter(&m) {
        let all = c.get(0).unwrap();
        if escaped(&m, all.start()) {
            continue;
        }
        let (target, label) = split_wiki(&c[1]);
        if target.is_empty() {
            continue;
        }
        out.push(Found { start: all.start(), end: all.end(), target, label, kind: "link" });
    }
    for c in TAG.captures_iter(&m) {
        let all = c.get(0).unwrap();
        let prev = m[..all.start()].chars().next_back();
        if prev.is_some_and(|p| !p.is_whitespace() && !"([{,;".contains(p)) {
            continue;
        }
        let name = c[1].trim_end_matches(['/', '-']);
        // Sam numer (#12) to nie Tag.
        if name.is_empty() || name.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if out.iter().any(|f| f.start <= all.start() && all.start() < f.end) {
            continue;
        }
        let end = all.start() + 1 + name.len();
        out.push(Found { start: all.start(), end, target: name.to_string(), label: format!("#{name}"), kind: "tag" });
    }
    out.sort_by_key(|f| f.start);
    out
}

/// Wartości listy z frontmattera: `klucz: [a, "b"]`, `klucz: a, b` albo linie `- a` pod kluczem.
fn fm_list(fm: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    let clean = |s: &str| {
        let s = s.trim();
        let s = if s.starts_with('"') { serde_json::from_str::<String>(s).unwrap_or_else(|_| s.to_string()) } else { s.to_string() };
        s.trim_matches(|c| c == '"' || c == '\'').trim().to_string()
    };
    for line in fm.lines() {
        if in_block {
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                out.push(clean(item));
                continue;
            }
            if line.starts_with(' ') || line.trim().is_empty() {
                continue;
            }
            in_block = false;
        }
        if let Some(v) = line.strip_prefix(&format!("{key}:")) {
            let v = v.trim();
            if v.is_empty() {
                in_block = true;
            } else if let Some(inner) = v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                // Elementy mogą zawierać przecinki w cudzysłowach: dzielimy z poszanowaniem cudzysłowów.
                let mut cur = String::new();
                let mut q = false;
                for ch in inner.chars() {
                    match ch {
                        '"' => {
                            q = !q;
                            cur.push(ch)
                        }
                        ',' if !q => out.push(clean(&std::mem::take(&mut cur))),
                        _ => cur.push(ch),
                    }
                }
                out.push(clean(&cur));
            } else {
                out.extend(v.split(',').map(clean));
            }
        }
    }
    out.retain(|s| !s.is_empty());
    out
}

fn parse_note(path: String, raw: &str) -> NoteInfo {
    let (fm, body) = split_frontmatter(raw);
    let stem = Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let title = vault::fm_value(fm, "title").filter(|t| !t.is_empty()).unwrap_or_else(|| stem.clone());
    let mut tags: Vec<String> = fm_list(fm, "tags").into_iter().map(|t| t.trim_start_matches('#').to_string()).collect();
    let mut links = Vec::new();
    for f in scan(body) {
        match f.kind {
            "link" => links.push(RawLink { target: f.target, line: line_at(body, f.start), from_frontmatter: false }),
            _ => tags.push(f.target),
        }
    }
    for p in fm_list(fm, "powiazane") {
        let inner = p.trim().trim_start_matches("[[").trim_end_matches("]]");
        let (target, _) = split_wiki(inner);
        if !target.is_empty() {
            links.push(RawLink { target, line: String::new(), from_frontmatter: true });
        }
    }
    let mut seen = HashSet::new();
    tags.retain(|t| !t.is_empty() && seen.insert(tag_key(t)));
    NoteInfo { path, stem, title, tags, links }
}

fn collect(vault: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            if !name.ends_with(".assets") {
                collect(vault, &p, out);
            }
        } else if name.to_lowercase().ends_with(".md") {
            out.push(p.strip_prefix(vault).unwrap_or(&p).to_string_lossy().replace('\\', "/"));
        }
    }
}

fn parent(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

impl Index {
    pub fn build(vault: &Path) -> Index {
        let mut paths = Vec::new();
        collect(vault, vault, &mut paths);
        let mut idx = Index::default();
        for p in paths {
            if let Ok(raw) = fs::read_to_string(vault.join(&p)) {
                idx.insert(parse_note(p, &raw));
            }
        }
        idx
    }

    fn insert(&mut self, n: NoteInfo) {
        self.remove(&n.path.clone());
        self.by_stem.entry(n.stem.to_lowercase()).or_default().push(n.path.clone());
        self.notes.insert(n.path.clone(), n);
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(old) = self.notes.remove(path) {
            if let Some(v) = self.by_stem.get_mut(&old.stem.to_lowercase()) {
                v.retain(|p| p != path);
            }
        }
    }

    /// Ponownie wczytuje jedną Notatkę (po zapisie albo zmianie z zewnątrz).
    pub fn update(&mut self, vault: &Path, path: &str) {
        if path.starts_with(&format!("{TRASH_DIR}/")) || path.starts_with(&format!("{APP_DIR}/")) {
            self.remove(path);
            return;
        }
        match fs::read_to_string(vault.join(path)) {
            Ok(raw) => self.insert(parse_note(path.to_string(), &raw)),
            Err(_) => self.remove(path),
        }
    }

    /// Rozwiązuje cel Linku jak Obsidian: po nazwie pliku, a przy kilku kandydatach
    /// najpierw ten z Folderu Notatki źródłowej, potem o najkrótszej ścieżce.
    pub fn resolve(&self, from: &str, target: &str) -> Option<String> {
        let t = target.trim().trim_start_matches('/').replace('\\', "/");
        let t = t.strip_suffix(".md").unwrap_or(&t).to_string();
        let tl = t.to_lowercase();
        let mut cands: Vec<&String> = if tl.contains('/') {
            let want = format!("{tl}.md");
            self.notes.keys().filter(|p| {
                let pl = p.to_lowercase();
                pl == want || pl.ends_with(&format!("/{want}"))
            }).collect()
        } else {
            self.by_stem.get(&tl).map(|v| v.iter().collect()).unwrap_or_default()
        };
        let here = parent(from);
        cands.sort_by_key(|p| (parent(p) != here, p.matches('/').count(), p.to_lowercase()));
        cands.first().map(|p| p.to_string())
    }

    pub fn summary(&self) -> Summary {
        let notes = self
            .notes
            .values()
            .map(|n| NoteRef { path: n.path.clone(), title: n.title.clone(), stem: n.stem.clone() })
            .collect();
        let mut tags: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for n in self.notes.values() {
            for t in &n.tags {
                tags.entry(tag_key(t)).or_insert_with(|| (t.clone(), 0)).1 += 1;
            }
        }
        Summary { notes, tags: tags.into_values().map(|(tag, count)| TagCount { tag, count }).collect() }
    }

    pub fn tag_notes(&self, tag: &str) -> Vec<NoteRef> {
        let k = tag_key(tag);
        let mut v: Vec<NoteRef> = self
            .notes
            .values()
            .filter(|n| n.tags.iter().any(|t| tag_key(t) == k))
            .map(|n| NoteRef { path: n.path.clone(), title: n.title.clone(), stem: n.stem.clone() })
            .collect();
        v.sort_by_key(|n| n.title.to_lowercase());
        v
    }

    fn note_ref(&self, path: &str) -> Option<NoteRef> {
        self.notes.get(path).map(|n| NoteRef { path: n.path.clone(), title: n.title.clone(), stem: n.stem.clone() })
    }

    /// Linki, Tagi i Backlinki Notatki. `body` to aktualna treść (także dla Notatki w Koszu).
    pub fn note_links(&self, path: &str, raw: &str) -> NoteLinks {
        let (fm, body) = split_frontmatter(raw);
        let info = parse_note(path.to_string(), raw);
        let spans = scan(body)
            .into_iter()
            .map(|f| Span {
                start: body[..f.start].encode_utf16().count(),
                end: body[..f.end].encode_utf16().count(),
                path: if f.kind == "link" { self.resolve(path, &f.target) } else { None },
                target: f.target,
                label: f.label,
                kind: f.kind,
            })
            .collect();
        let related = info
            .links
            .iter()
            .filter(|l| l.from_frontmatter)
            .filter_map(|l| self.resolve(path, &l.target))
            .filter_map(|p| self.note_ref(&p))
            .collect();
        let _ = fm;
        let mut backlinks = Vec::new();
        for n in self.notes.values() {
            if n.path == path {
                continue;
            }
            let mut lines = Vec::new();
            let mut related = false;
            for l in &n.links {
                if self.resolve(&n.path, &l.target).as_deref() == Some(path) {
                    if l.from_frontmatter {
                        related = true;
                    } else if !lines.contains(&l.line) {
                        lines.push(l.line.clone());
                    }
                }
            }
            if !lines.is_empty() || related {
                backlinks.push(Backlink { path: n.path.clone(), title: n.title.clone(), lines, related });
            }
        }
        backlinks.sort_by_key(|b| b.title.to_lowercase());
        NoteLinks { spans, tags: info.tags, related, backlinks }
    }

    /// Graf wiedzy: węzły to Notatki i Tagi (`#tag`), krawędzie to Linki, Powiązania i przypisania Tagów.
    pub fn graph(&self) -> Graph {
        let mut nodes: Vec<GraphNode> = self
            .notes
            .values()
            .map(|n| GraphNode { id: n.path.clone(), label: n.title.clone(), kind: "note" })
            .collect();
        let mut edges = HashSet::new();
        let mut tags: BTreeMap<String, String> = BTreeMap::new();
        for n in self.notes.values() {
            for l in &n.links {
                if let Some(p) = self.resolve(&n.path, &l.target) {
                    if p != n.path {
                        let e = if n.path < p { (n.path.clone(), p) } else { (p, n.path.clone()) };
                        edges.insert(e);
                    }
                }
            }
            for t in &n.tags {
                let id = format!("#{}", tag_key(t));
                tags.entry(id.clone()).or_insert_with(|| format!("#{t}"));
                edges.insert((n.path.clone(), id));
            }
        }
        nodes.extend(tags.into_iter().map(|(id, label)| GraphNode { id, label, kind: "tag" }));
        let mut edges: Vec<_> = edges.into_iter().collect();
        edges.sort();
        Graph { nodes, edges }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links_and_tags_outside_code() {
        let body = "Zobacz [[Inna notatka|tę]] i [[Folder/X#Nagłówek]] #projekt #12\n`[[nie]] #nie`\n```\n#nie [[nie]]\n```\n\\[[nie]] \\#nie a#nie (#tak)\n<span style=\"color:#F02828\">x</span>";
        let f = scan(body);
        let got: Vec<(&str, &str, &str)> = f.iter().map(|f| (f.kind, f.target.as_str(), f.label.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("link", "Inna notatka", "tę"),
                ("link", "Folder/X", "Folder/X#Nagłówek"),
                ("tag", "projekt", "#projekt"),
                ("tag", "tak", "#tak"),
            ]
        );
    }

    /// Ręczne sprawdzenie na prawdziwym Skarbcu: `NW_VAULT=C:\Notatki cargo test real_vault -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_vault() {
        let v = std::env::var("NW_VAULT").unwrap();
        let t = std::time::Instant::now();
        let idx = Index::build(Path::new(&v));
        let s = idx.summary();
        println!("notatek {}, tagów {}, czas {:?}", s.notes.len(), s.tags.len(), t.elapsed());
        for tc in s.tags.iter().take(60) {
            let n = idx.tag_notes(&tc.tag);
            println!("  #{} ({}) np. {}", tc.tag, tc.count, n[0].path);
        }
        let g = idx.graph();
        println!("krawędzi {}", g.edges.len());
    }

    #[test]
    fn imported_escaped_link_is_not_wiki() {
        let body = r"\[[1-1.docx](N.assets/1-1.docx)\]";
        assert!(scan(body).is_empty());
    }

    #[test]
    fn frontmatter_lists() {
        let fm = "title: \"A\"\ntags: [a, \"b, c\"]\npowiazane: [\"[[X]]\", \"[[Y|y]]\"]\ninne:\n  - p\n  - q\n";
        assert_eq!(fm_list(fm, "tags"), vec!["a", "b, c"]);
        assert_eq!(fm_list(fm, "powiazane"), vec!["[[X]]", "[[Y|y]]"]);
        assert_eq!(fm_list(fm, "inne"), vec!["p", "q"]);
    }

    #[test]
    fn resolves_prefer_same_folder() {
        let mut idx = Index::default();
        for p in ["A/Cel.md", "B/Cel.md", "Cel.md", "A/Zrodlo.md"] {
            idx.insert(parse_note(p.into(), "x"));
        }
        assert_eq!(idx.resolve("A/Zrodlo.md", "Cel").as_deref(), Some("A/Cel.md"));
        assert_eq!(idx.resolve("C/Z.md", "cel").as_deref(), Some("Cel.md"));
        assert_eq!(idx.resolve("A/Zrodlo.md", "B/Cel").as_deref(), Some("B/Cel.md"));
        assert_eq!(idx.resolve("A/Zrodlo.md", "Brak"), None);
    }

    #[test]
    fn backlinks_and_graph() {
        let mut idx = Index::default();
        idx.insert(parse_note("A.md".into(), "---\ntitle: \"Aa\"\npowiazane: [\"[[C]]\"]\n---\nIdź do [[B]] #t"));
        idx.insert(parse_note("B.md".into(), "nic"));
        idx.insert(parse_note("C.md".into(), "#T"));
        let b = idx.note_links("B.md", "nic");
        assert_eq!(b.backlinks.len(), 1);
        assert_eq!(b.backlinks[0].lines, vec!["Idź do [[B]] #t"]);
        let c = idx.note_links("C.md", "#T");
        assert!(c.backlinks[0].related);
        let g = idx.graph();
        assert_eq!(g.nodes.len(), 4);
        assert!(g.edges.contains(&("A.md".into(), "#t".into())));
        assert!(g.edges.contains(&("C.md".into(), "#t".into())));
        assert!(g.edges.contains(&("A.md".into(), "C.md".into())));
        assert_eq!(idx.summary().tags[0].count, 2);
    }
}
