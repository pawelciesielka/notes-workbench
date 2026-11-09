//! Serwer MCP dla Claude: dostęp tylko do odczytu do Skarbca Notes Workbench.
//!
//! Protokół: JSON-RPC 2.0 przez stdin/stdout, jedna wiadomość w linii (transport stdio MCP).
//! Na stdout piszemy wyłącznie odpowiedzi protokołu; diagnostyka idzie na stderr.
//!
//! Skarbiec: `--vault <katalog>` albo ten, który jest otwarty w aplikacji (jej settings.json).
//! Kosz i katalog aplikacji są niewidoczne, tak jak w wyszukiwaniu w aplikacji.

use notes_core::index::{self, Index};
use notes_core::proposals;
use notes_core::search::{Search, HL_END, HL_START};
use notes_core::semantic::{self, Semantic};
use notes_core::vault::{self, TreeNode};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const APP_ID: &str = "eu.ciesielka.notesworkbench";
/// Źródło Propozycji pokazywane użytkownikowi w aplikacji.
const SOURCE: &str = "Claude (MCP)";
const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// Limit tekstu jednej odpowiedzi, żeby wielka Notatka nie zajęła całego kontekstu Claude.
const MAX_TEXT: usize = 60_000;

struct Server {
    fixed_vault: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    vault: Option<PathBuf>,
    index: Option<Index>,
    /// Notatki oznaczone `#noai` (same albo przez Notatkę folderu): niewidoczne w żadnym narzędziu.
    noai: HashSet<String>,
    search: Option<Search>,
    synced: Option<Instant>,
    semantic: Semantic,
    semantic_error: Option<String>,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// Skarbiec otwarty w aplikacji (zapisany w jej ustawieniach).
fn app_vault() -> Option<PathBuf> {
    let s = std::fs::read_to_string(env_dir("APPDATA")?.join(APP_ID).join("settings.json")).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    v["vault"].as_str().map(PathBuf::from)
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n\n[… obcięto: Notatka ma {} znaków, pokazano pierwsze {}]", &s[..cut], s.chars().count(), s[..cut].chars().count())
}

fn hl(s: &str) -> String {
    s.replace(HL_START, "**").replace(HL_END, "**").replace('\n', " ")
}

fn hidden(path: &str) -> bool {
    path.starts_with(&format!("{}/", vault::TRASH_DIR)) || path.starts_with(&format!("{}/", vault::APP_DIR))
}

impl Server {
    /// Aktualny Skarbiec i świeże indeksy (pliki mogły zmienić się w aplikacji albo poza nią).
    fn sync(&mut self) -> Result<PathBuf, String> {
        let v = self
            .fixed_vault
            .clone()
            .or_else(app_vault)
            .ok_or("Nie wiadomo, który Skarbiec otworzyć: podaj --vault albo otwórz Skarbiec w aplikacji Notes Workbench.")?;
        if !v.is_dir() {
            return Err(format!("Skarbiec {} nie istnieje.", v.display()));
        }
        let fresh = self.synced.is_some_and(|t| t.elapsed() < Duration::from_secs(3));
        if self.vault.as_ref() != Some(&v) {
            self.index = None;
            self.search = None;
            self.semantic.reset_store();
            self.semantic_error = None;
            self.vault = Some(v.clone());
        } else if fresh {
            return Ok(v);
        }
        let idx = Index::build(&v);
        self.noai = idx.noai();
        self.index = Some(idx);
        self.search.get_or_insert_with(Search::new).sync(&v);
        self.synced = Some(Instant::now());
        Ok(v)
    }

    fn idx(&self) -> &Index {
        self.index.as_ref().unwrap()
    }

    /// Model i wektory wczytywane przy pierwszym użyciu; błąd zapamiętujemy, żeby nie próbować co chwilę.
    fn semantic_ready(&mut self, vault: &Path) -> Result<(), String> {
        if let Some(e) = &self.semantic_error {
            return Err(e.clone());
        }
        let data = self.data_dir.clone().ok_or("Brak katalogu danych aplikacji")?;
        self.semantic.load_readonly(&data, vault).inspect_err(|e| self.semantic_error = Some(e.clone()))
    }

    /// Ścieżka Notatki z argumentu: ścieżka w Skarbcu, nazwa pliku albo tytuł.
    /// Notatki oznaczone `#noai` są dla serwera jak nieistniejące: nie zdradzamy nawet, że są.
    fn find_note(&self, arg: &str) -> Result<String, String> {
        let a = arg.trim().replace('\\', "/");
        let a = a.trim_start_matches('/');
        let not_found = || format!("Nie ma Notatki „{arg}”. Użyj search_notes albo list_notes, żeby znaleźć ścieżkę.");
        if hidden(a) {
            return Err("Notatki z Kosza i pliki aplikacji nie są dostępne.".into());
        }
        let with_md = if a.to_lowercase().ends_with(".md") { a.to_string() } else { format!("{a}.md") };
        let s = self.idx().summary();
        let visible: Vec<_> = s.notes.iter().filter(|n| !self.noai.contains(&n.path)).collect();
        if visible.iter().any(|n| n.path == with_md) {
            return Ok(with_md);
        }
        if let Some(p) = self.idx().resolve("", a) {
            return if self.noai.contains(&p) { Err(not_found()) } else { Ok(p) };
        }
        let low = a.to_lowercase();
        let by_title: Vec<_> = visible.into_iter().filter(|n| n.title.to_lowercase() == low).collect();
        match by_title.len() {
            1 => Ok(by_title[0].path.clone()),
            0 => Err(not_found()),
            _ => Err(format!(
                "Kilka Notatek ma tytuł „{arg}”: {}. Podaj ścieżkę.",
                by_title.iter().map(|n| n.path.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    fn call(&mut self, name: &str, args: &Value) -> Result<String, String> {
        let vault = self.sync()?;
        let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match name {
            "search_notes" => {
                let q = s("query");
                if q.trim().is_empty() {
                    return Err("Podaj query.".into());
                }
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(15).clamp(1, 50) as usize;
                let mode = args.get("mode").and_then(Value::as_str).unwrap_or("hybrid");
                // Pobieramy z zapasem, bo Notatki #noai odpadają dopiero po wyszukaniu.
                let pool = 100;
                let words = self.search.as_ref().unwrap().search(&q, pool)?;
                let mut note = String::new();
                let hits: Vec<(String, String, String, String)> = if mode == "words" {
                    if words.partial {
                        note = "Żadna Notatka nie zawiera wszystkich słów; wyniki z częścią słów.\n".into();
                    }
                    words.hits.into_iter().map(|h| (h.path, h.title, h.folder, h.snippet)).collect()
                } else {
                    match self.semantic_ready(&vault) {
                        Ok(()) => {
                            let idx = self.index.as_ref().unwrap();
                            let titles = |p: &str| idx.title(p);
                            let sem = self.semantic.search(&q, pool, &titles)?;
                            let list = if mode == "meaning" { sem } else { semantic::fuse(sem, words.hits, pool) };
                            list.into_iter().map(|h| (h.path, h.title, h.folder, h.snippet)).collect()
                        }
                        Err(e) => {
                            note = format!("Uwaga: {e} Wyniki tylko po słowach.\n");
                            words.hits.into_iter().map(|h| (h.path, h.title, h.folder, h.snippet)).collect()
                        }
                    }
                };
                let hits: Vec<_> = hits.into_iter().filter(|h| !self.noai.contains(&h.0)).take(limit).collect();
                if hits.is_empty() {
                    return Ok(format!("{note}Nic nie znaleziono dla: {q}"));
                }
                let mut out = note;
                for (i, (path, title, folder, snippet)) in hits.iter().enumerate() {
                    let f = if folder.is_empty() { String::new() } else { format!(" (Folder: {folder})") };
                    out += &format!("{}. {title}{f}\n   path: {path}\n   {}\n", i + 1, hl(snippet).trim());
                }
                Ok(out)
            }
            "read_note" => {
                let path = self.find_note(&s("path"))?;
                let n = vault::read_note(&vault, &path)?;
                let mut links = self.idx().note_links(&path, &n.raw);
                links.backlinks.retain(|b| !self.noai.contains(&b.path));
                let mut head = format!("# {}\npath: {path}\n", n.title);
                if !links.tags.is_empty() {
                    head += &format!("tags: {}\n", links.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "));
                }
                if !links.backlinks.is_empty() {
                    head += &format!(
                        "backlinks: {}\n",
                        links.backlinks.iter().map(|b| format!("{} ({})", b.title, b.path)).collect::<Vec<_>>().join("; ")
                    );
                }
                let assets = Path::new(&path).with_extension("assets");
                let assets_dir = vault.join(&assets);
                if assets_dir.is_dir() {
                    let files: Vec<String> = std::fs::read_dir(&assets_dir)
                        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
                        .unwrap_or_default();
                    head += &format!(
                        "attachments ({}): {}\n",
                        files.len(),
                        files.iter().take(30).cloned().collect::<Vec<_>>().join(", ")
                    );
                }
                head += "\n---\n\n";
                let fm = if n.frontmatter.trim().is_empty() { String::new() } else { format!("frontmatter:\n{}\n\n", n.frontmatter.trim_end()) };
                Ok(clip(&format!("{head}{fm}{}", n.body), MAX_TEXT))
            }
            "list_notes" => {
                let folder = s("folder").replace('\\', "/").trim_matches('/').to_string();
                if hidden(&format!("{folder}/")) {
                    return Err("Ten Folder nie jest dostępny.".into());
                }
                let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(1).clamp(1, 10) as usize;
                // Drzewo bez Notatek #noai; Folder, którego Notatka folderu ma #noai, znika w całości.
                fn prune(list: Vec<TreeNode>, noai: &HashSet<String>, hidden: &mut usize) -> Vec<TreeNode> {
                    fn all(n: &TreeNode) -> usize {
                        usize::from(n.kind == "note") + usize::from(n.folder_note.is_some()) + n.children.iter().map(all).sum::<usize>()
                    }
                    let mut out = Vec::new();
                    for mut n in list {
                        if n.kind == "note" {
                            if noai.contains(&n.path) {
                                *hidden += 1;
                            } else {
                                out.push(n);
                            }
                            continue;
                        }
                        if n.folder_note.as_ref().is_some_and(|f| noai.contains(f)) {
                            *hidden += all(&n);
                            continue;
                        }
                        n.children = prune(std::mem::take(&mut n.children), noai, hidden);
                        out.push(n);
                    }
                    out
                }
                let mut hidden_count = 0;
                let tree = prune(vault::tree(&vault, false), &self.noai, &mut hidden_count);
                let nodes: Vec<TreeNode> = if folder.is_empty() {
                    tree
                } else {
                    fn find(list: Vec<TreeNode>, f: &str) -> Option<TreeNode> {
                        for n in list {
                            if n.kind == "folder" && n.path == f {
                                return Some(n);
                            }
                            if f.starts_with(&format!("{}/", n.path)) {
                                return find(n.children, f);
                            }
                        }
                        None
                    }
                    let n = find(tree, &folder).ok_or(format!("Nie ma Folderu {folder}"))?;
                    let mut v = n.children;
                    if let Some(fnote) = n.folder_note {
                        v.insert(0, TreeNode { name: String::new(), title: format!("(Notatka folderu) {}", n.title), path: fnote, kind: "note", order: None, folder_note: None, children: vec![] });
                    }
                    v
                };
                fn count(n: &[TreeNode]) -> usize {
                    n.iter().map(|x| usize::from(x.kind == "note") + usize::from(x.folder_note.is_some()) + count(&x.children)).sum()
                }
                fn render(n: &[TreeNode], d: usize, max: usize, out: &mut String) {
                    for x in n {
                        let pad = "  ".repeat(d);
                        if x.kind == "folder" {
                            *out += &format!("{pad}📁 {} ({} Notatek) — folder: {}\n", x.title, count(&x.children) + usize::from(x.folder_note.is_some()), x.path);
                            if d + 1 < max {
                                render(&x.children, d + 1, max, out);
                            }
                        } else {
                            *out += &format!("{pad}- {} — path: {}\n", x.title, x.path);
                        }
                    }
                }
                let mut out = format!("{} (Notatek łącznie: {})\n", if folder.is_empty() { "Skarbiec" } else { &folder }, count(&nodes));
                render(&nodes, 0, depth, &mut out);
                if hidden_count > 0 {
                    out += &format!("\n(W Skarbcu jest {hidden_count} Notatek oznaczonych przez użytkownika jako niedostępne dla AI; nie są tu pokazane.)\n");
                }
                Ok(clip(&out, MAX_TEXT))
            }
            "note_links" => {
                let path = self.find_note(&s("path"))?;
                let n = vault::read_note(&vault, &path)?;
                let mut l = self.idx().note_links(&path, &n.raw);
                l.backlinks.retain(|b| !self.noai.contains(&b.path));
                l.related.retain(|r| !self.noai.contains(&r.path));
                let mut out = format!("# {}\npath: {path}\n", n.title);
                let outgoing: Vec<String> = l
                    .spans
                    .iter()
                    .filter(|x| x.kind == "link")
                    .map(|x| match &x.path {
                        Some(p) if self.noai.contains(p) => format!("- [[{}]] → (niedostępna dla AI)", x.target),
                        Some(p) => format!("- [[{}]] → {p}", x.target),
                        None => format!("- [[{}]] → (Notatka nie istnieje)", x.target),
                    })
                    .collect();
                out += &format!("\nLinki wychodzące ({}):\n{}\n", outgoing.len(), outgoing.join("\n"));
                out += &format!("\nTagi: {}\n", if l.tags.is_empty() { "brak".into() } else { l.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ") });
                if !l.related.is_empty() {
                    out += &format!("\nPowiązania (zaakceptowane, z frontmattera): {}\n", l.related.iter().map(|r| format!("{} ({})", r.title, r.path)).collect::<Vec<_>>().join("; "));
                }
                out += &format!("\nBacklinki ({}):\n", l.backlinks.len());
                for b in &l.backlinks {
                    out += &format!("- {} — path: {}{}\n", b.title, b.path, if b.related { " (Powiązanie)" } else { "" });
                    for line in &b.lines {
                        out += &format!("    > {line}\n");
                    }
                }
                if self.semantic_ready(&vault).is_ok() {
                    let sims: Vec<String> = self
                        .semantic
                        .similar(&path, 6)
                        .into_iter()
                        .filter(|(p, s)| *s > semantic::SIMILAR_MIN && !self.noai.contains(p))
                        .filter_map(|(p, s)| self.idx().title(&p).map(|t| format!("- {t} — path: {p} (podobieństwo {s:.2})")))
                        .collect();
                    if !sims.is_empty() {
                        out += &format!("\nNotatki podobne treścią (model, to nie są Linki):\n{}\n", sims.join("\n"));
                    }
                }
                Ok(out)
            }
            "list_tags" => {
                let tag = s("tag").trim_start_matches('#').to_string();
                // Liczymy tylko Notatki dostępne dla AI; sam Tag noai pomijamy.
                let visible = |t: &str| -> Vec<index::NoteRef> {
                    self.idx().tag_notes(t).into_iter().filter(|n| !self.noai.contains(&n.path)).collect()
                };
                if tag.is_empty() {
                    let tags: Vec<String> = self
                        .idx()
                        .summary()
                        .tags
                        .iter()
                        .filter(|t| index::tag_key(&t.tag) != index::NOAI_TAG)
                        .filter_map(|t| {
                            let n = visible(&t.tag).len();
                            (n > 0).then(|| format!("#{} ({n})", t.tag))
                        })
                        .collect();
                    if tags.is_empty() {
                        return Ok("Skarbiec nie ma jeszcze Tagów.".into());
                    }
                    Ok(tags.join("\n"))
                } else {
                    let notes = visible(&tag);
                    if notes.is_empty() {
                        return Ok(format!("Żadna Notatka nie ma Tagu #{tag}."));
                    }
                    Ok(notes.iter().map(|n| format!("- {} — path: {}", n.title, n.path)).collect::<Vec<_>>().join("\n"))
                }
            }
            "propose_relations" | "propose_tags" => {
                let items = args.get("items").and_then(Value::as_array).ok_or("Podaj items (lista).")?;
                if items.len() > 50 {
                    return Err("Najwyżej 50 Propozycji naraz.".into());
                }
                let mut out = String::new();
                let (mut added, mut skipped) = (0, 0);
                for (i, it) in items.iter().enumerate() {
                    let g = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    let reason = g("reason");
                    let res = if name == "propose_relations" {
                        let (a, b) = (g("from"), g("to"));
                        match (self.find_note(&a), self.find_note(&b)) {
                            (Ok(pa), Ok(pb)) => proposals::add_relation(&vault, self.idx(), &pa, &pb, &reason, SOURCE)
                                .map(|r| (format!("{} ↔ {}", self.idx().title(&pa).unwrap_or(pa.clone()), self.idx().title(&pb).unwrap_or(pb.clone())), r)),
                            (Err(e), _) | (_, Err(e)) => Ok((format!("{a} ↔ {b}"), proposals::Added::NotFound(e))),
                        }
                    } else {
                        let (p, tag) = (g("path"), g("tag"));
                        match self.find_note(&p) {
                            Ok(pp) => proposals::add_tag(&vault, self.idx(), &pp, &tag, &reason, SOURCE)
                                .map(|r| (format!("#{} → {}", tag.trim_start_matches('#'), self.idx().title(&pp).unwrap_or(pp.clone())), r)),
                            Err(e) => Ok((format!("#{tag} → {p}"), proposals::Added::NotFound(e))),
                        }
                    }?;
                    let (label, r) = res;
                    let msg = match r {
                        proposals::Added::Added(id) => {
                            added += 1;
                            format!("dodano (id {id})")
                        }
                        other => {
                            skipped += 1;
                            match other {
                                proposals::Added::AlreadyPending => "pominięto: już czeka na decyzję".into(),
                                proposals::Added::RejectedBefore => "pominięto: użytkownik wcześniej to odrzucił, nie proponuj ponownie".into(),
                                proposals::Added::AlreadyLinked => "pominięto: już połączone / już ma ten Tag".into(),
                                proposals::Added::NotFound(e) | proposals::Added::Invalid(e) => format!("błąd: {e}"),
                                proposals::Added::Added(_) => unreachable!(),
                            }
                        }
                    };
                    out += &format!("{}. {label}: {msg}\n", i + 1);
                }
                Ok(format!(
                    "Dodano {added}, pominięto {skipped}.\n{out}\nPropozycje czekają na akceptację użytkownika w aplikacji Notes Workbench (widok Propozycje); dopiero wtedy trafiają do Notatek.",
                ))
            }
            "list_proposals" => {
                let status = args.get("status").and_then(Value::as_str).unwrap_or("pending");
                let filter = match args.get("path").and_then(Value::as_str) {
                    Some(p) if !p.trim().is_empty() => Some(self.find_note(p)?),
                    _ => None,
                };
                let all = proposals::list(&vault)?;
                let shown: Vec<_> = all
                    .iter()
                    .filter(|p| match status {
                        "rejected" => p.status == proposals::Status::Rejected,
                        "all" => true,
                        _ => p.status == proposals::Status::Pending,
                    })
                    // Propozycje dotyczące Notatek ukrytych przed AI też są ukryte.
                    .filter(|p| !self.noai.contains(&p.note) && p.target.as_ref().is_none_or(|t| !self.noai.contains(t)))
                    .filter(|p| filter.as_ref().is_none_or(|f| &p.note == f || p.target.as_ref() == Some(f)))
                    .collect();
                if shown.is_empty() {
                    return Ok("Brak Propozycji.".into());
                }
                let t = |p: &str| self.idx().title(p).unwrap_or_else(|| p.to_string());
                let mut out = format!("Propozycje ({}):\n", shown.len());
                for p in shown {
                    let what = match p.kind {
                        proposals::Kind::Relation => format!("{} ↔ {}", t(&p.note), t(p.target.as_deref().unwrap_or(""))),
                        proposals::Kind::Tag => format!("#{} → {}", p.tag.as_deref().unwrap_or(""), t(&p.note)),
                    };
                    let st = if p.status == proposals::Status::Rejected { " [odrzucona]" } else { "" };
                    out += &format!("- id {}: {what}{st} — {}\n", p.id, p.reason);
                }
                Ok(clip(&out, MAX_TEXT))
            }
            _ => Err(format!("Nieznane narzędzie: {name}")),
        }
    }
}

fn tools() -> Value {
    let ro = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "search_notes",
            "title": "Szukaj w notatkach",
            "description": "Search the user's personal notes (Polish, Markdown, imported from All My Notes Organizer). \
mode 'hybrid' (default) combines keyword and semantic (meaning) ranking; 'words' is exact full-text (prefix matching, \
case and Polish diacritics ignored, \"phrase\" in quotes, #tag filters by tag); 'meaning' is semantic only. \
Write queries in Polish for best results. Returns titles, paths and snippets; use read_note to get the full text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for" },
                    "mode": { "type": "string", "enum": ["hybrid", "words", "meaning"], "default": "hybrid" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 15 }
                },
                "required": ["query"]
            },
            "annotations": ro
        },
        {
            "name": "read_note",
            "title": "Czytaj Notatkę",
            "description": "Read one note: title, tags, backlinks, attachment names, frontmatter and the full Markdown body. \
'path' may be the vault path from search/list results, the file name or the exact title.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            },
            "annotations": ro
        },
        {
            "name": "list_notes",
            "title": "Lista Notatek",
            "description": "Browse the folder tree of the notes vault (in the user's own order). Without 'folder' lists the top level. \
'depth' controls how many folder levels to expand (default 1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string", "description": "Folder path, e.g. 'Projects/Monitoring'" },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 10, "default": 1 }
                }
            },
            "annotations": ro
        },
        {
            "name": "note_links",
            "title": "Linki Notatki",
            "description": "Knowledge-graph neighbourhood of a note: outgoing [[links]], backlinks with the line that links, \
tags, accepted AI relations (Powiązania) and semantically similar notes.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            },
            "annotations": ro
        },
        {
            "name": "propose_relations",
            "title": "Zaproponuj powiązania",
            "description": "Propose links (Powiązania) between pairs of notes that are genuinely related in content (same project, \
topic, problem, person or decision) — not merely sharing a word. Each item needs a one-sentence 'reason' in Polish that \
the user will read when deciding. Nothing is written to the notes: proposals wait in a queue until the user accepts them \
in the Notes Workbench app; accepted ones become `powiazane: [[...]]` in the note's frontmatter. Previously rejected pairs \
are refused — do not re-propose them. Use search_notes/note_links (it shows similar notes) to find candidates and \
read_note to verify before proposing. Up to 50 items per call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "from": { "type": "string", "description": "Note path (the relation is stored in this note)" },
                                "to": { "type": "string", "description": "Related note path" },
                                "reason": { "type": "string", "description": "Why they are related, one sentence in Polish" }
                            },
                            "required": ["from", "to", "reason"]
                        }
                    }
                },
                "required": ["items"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "propose_tags",
            "title": "Zaproponuj Tagi",
            "description": "Propose tags for notes. Prefer existing tags (see list_tags); create a new tag only when none fits. \
Tags: lowercase, letters/digits/_-/ without spaces, Polish words are fine. Each item needs a short 'reason' in Polish. \
Proposals wait for the user's acceptance in the app; accepted tags go to `tags:` in the frontmatter. Up to 50 items per call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "tag": { "type": "string" },
                                "reason": { "type": "string" }
                            },
                            "required": ["path", "tag", "reason"]
                        }
                    }
                },
                "required": ["items"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "list_proposals",
            "title": "Propozycje",
            "description": "List proposals in the queue: 'pending' (default, waiting for the user), 'rejected' (do not propose again) or 'all'. \
Optional 'path' limits to proposals involving one note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["pending", "rejected", "all"], "default": "pending" },
                    "path": { "type": "string" }
                }
            },
            "annotations": ro
        },
        {
            "name": "list_tags",
            "title": "Tagi",
            "description": "Without 'tag': all tags with note counts. With 'tag': the notes that have this tag.",
            "inputSchema": {
                "type": "object",
                "properties": { "tag": { "type": "string" } }
            },
            "annotations": ro
        }
    ])
}

fn handle(srv: &mut Server, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let ok = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOLS[0]);
            let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "notes-workbench", "title": "Notes Workbench", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Access to the user's personal knowledge base. Notes themselves are read-only; you can propose \
relations and tags (propose_relations, propose_tags), which the user accepts or rejects in the Notes Workbench app. \
Personal knowledge base (Markdown notes, mostly in Polish). \
Start with search_notes, then read_note for full text. Notes link to each other with [[links]] and #tags; \
use note_links to follow the knowledge graph. Cite note titles when you use their content. \
The user has marked some notes as unavailable to AI (#noai); they are hidden from every tool. \
Do not try to infer or reach their content in other ways."
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, is_error) = match srv.call(name, &args) {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        _ => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("Method not found: {method}") } }),
    })
}

/// Uruchamia serwer MCP na stdin/stdout aż do zamknięcia wejścia.
/// `args`: argumenty bez nazwy programu (`--mcp` jest pomijany, gdy serwer działa w aplikacji).
pub fn serve(args: impl IntoIterator<Item = String>) {
    let mut args = args.into_iter();
    let mut fixed_vault = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--mcp" => {}
            "--vault" => fixed_vault = args.next().map(PathBuf::from),
            "--version" => {
                println!("notes-mcp {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            _ => {
                eprintln!("Użycie: notes-mcp [--vault <katalog Skarbca>]");
                std::process::exit(2);
            }
        }
    }
    let mut srv = Server {
        fixed_vault,
        data_dir: env_dir("LOCALAPPDATA").map(|d| d.join(APP_ID)),
        vault: None,
        index: None,
        noai: HashSet::new(),
        search: None,
        synced: None,
        semantic: Semantic::default(),
        semantic_error: None,
    };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let r: Vec<Value> = batch.iter().filter_map(|m| handle(&mut srv, m)).collect();
                (!r.is_empty()).then(|| Value::Array(r))
            }
            Ok(msg) => handle(&mut srv, &msg),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("Parse error: {e}") } })),
        };
        if let Some(r) = reply {
            let _ = writeln!(stdout, "{r}");
            let _ = stdout.flush();
        }
    }
}
