//! Import Eksportu AMNO do nowego Skarbca: Foldery jako katalogi, Notatki jako pliki
//! Markdown z frontmatterem, Załączniki w katalogu `<Notatka>.assets` obok Notatki,
//! Kosz AMNO w `.kosz/`. Każda Notatka jest sprawdzana tekstowo względem źródła,
//! a wynik trafia do Raportu importu (`.notes-workbench/import/`).

pub mod md;
pub mod model;
pub mod names;
pub mod parse;
pub mod verify;

use md::Resolver;
use parse::{Entry, Kind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = ".notes-workbench";
pub const TRASH_DIR: &str = ".kosz";

pub struct Options {
    pub export_html: PathBuf,
    pub vault: PathBuf,
    /// Data Importu (YYYY-MM-DD), zapisywana w Notatkach jako `imported`.
    pub date: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct Issue {
    /// "błąd", "ostrzeżenie" albo "zmiana".
    pub level: &'static str,
    pub kind: &'static str,
    /// Notatka w Skarbcu (ścieżka względna), której dotyczy.
    pub note: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize, Default, Clone)]
pub struct Counts {
    pub notes_in_export: usize,
    pub folders_in_export: usize,
    pub notes_written: usize,
    pub folders_written: usize,
    pub notes_in_trash: usize,
    pub images: usize,
    pub attachments: usize,
    pub files_in_export: usize,
    pub files_copied: usize,
    pub files_unreferenced: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct NoteMap {
    pub amno: String,
    pub path: String,
    pub title: String,
    pub generated_title: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct Report {
    pub ok: bool,
    pub source: String,
    pub vault: String,
    pub date: String,
    pub started: String,
    pub finished: String,
    pub counts: Counts,
    pub errors: usize,
    pub warnings: usize,
    pub issues: Vec<Issue>,
    pub notes: Vec<NoteMap>,
}

struct Ctx<'a> {
    opts: &'a Options,
    entries: Vec<Entry>,
    children: Vec<Vec<usize>>,
    files_dir: PathBuf,
    files_marker: String,
    issues: Vec<Issue>,
    counts: Counts,
    notes: Vec<NoteMap>,
    folder_order: BTreeMap<String, usize>,
    used_files: HashSet<String>,
}

impl Ctx<'_> {
    fn issue(&mut self, level: &'static str, kind: &'static str, note: Option<&str>, message: String) {
        self.issues.push(Issue { level, kind, note: note.map(str::to_string), message });
    }
}

fn rel_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut h = Sha256::new();
    let mut f = fs::File::open(p)?;
    std::io::copy(&mut f, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

/// Tytuł zastępczy: pierwsza niepusta linia treści, skrócona do ~60 znaków.
fn generated_title(blocks: &[model::Block]) -> String {
    fn first_line(blocks: &[model::Block]) -> Option<String> {
        for b in blocks {
            match b {
                model::Block::Line(l) => {
                    let t = l.raw_text();
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
                model::Block::Table(t) => {
                    for c in t.rows.iter().flatten() {
                        if let Some(s) = first_line(&c.blocks) {
                            return Some(s);
                        }
                    }
                }
            }
        }
        None
    }
    let Some(t) = first_line(blocks) else { return "Bez tytułu".into() };
    if t.chars().count() <= 60 {
        return t;
    }
    let cut: String = t.chars().take(60).collect();
    let cut = match cut.rfind(' ') {
        Some(p) if p > 30 => cut[..p].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end())
}

struct NoteResolver<'a> {
    files_dir: &'a Path,
    files_marker: &'a str,
    assets_name: String,
    assets_abs: PathBuf,
    note_rel: String,
    by_src: HashMap<String, String>,
    names: HashSet<String>,
    copies: Vec<(PathBuf, PathBuf, String)>,
    img_n: usize,
    issues: Vec<Issue>,
    images: usize,
    attachments: usize,
}

impl NoteResolver<'_> {
    fn basename(s: &str) -> &str {
        s.rsplit(['\\', '/']).next().unwrap_or(s)
    }

    fn is_export_file(&self, s: &str) -> bool {
        s.to_lowercase().contains(&self.files_marker.to_lowercase())
    }

    /// Kopiuje plik z katalogu eksportu jako Załącznik Notatki; zwraca zakodowany adres względny.
    fn attach(&mut self, src_name: &str, dest_name: impl FnOnce(&mut Self) -> String) -> Option<String> {
        if let Some(d) = self.by_src.get(src_name) {
            return Some(d.clone());
        }
        let src = self.files_dir.join(src_name);
        if !src.is_file() {
            self.issues.push(Issue {
                level: "błąd",
                kind: "brak-pliku",
                note: Some(self.note_rel.clone()),
                message: format!("Brak pliku „{src_name}” w katalogu eksportu"),
            });
            return None;
        }
        let name = dest_name(self);
        let url = names::enc_url_path(&format!("{}/{}", self.assets_name, name));
        self.copies.push((src, self.assets_abs.join(&name), src_name.to_string()));
        self.by_src.insert(src_name.to_string(), url.clone());
        Some(url)
    }
}

impl Resolver for NoteResolver<'_> {
    fn image(&mut self, src: &str) -> (String, Option<(u32, u32)>) {
        if !self.is_export_file(src) {
            return (names::enc_url(src), None);
        }
        let base = Self::basename(src).to_string();
        let path = self.files_dir.join(&base);
        let natural = imagesize::size(&path).ok().map(|d| (d.width as u32, d.height as u32));
        let ext = Path::new(&base)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "png".into());
        let was_new = !self.by_src.contains_key(&base);
        let url = self.attach(&base, |me| {
            loop {
                me.img_n += 1;
                let n = format!("obraz-{}.{ext}", me.img_n);
                if me.names.insert(n.to_lowercase()) {
                    return n;
                }
            }
        });
        if was_new && url.is_some() {
            self.images += 1;
        }
        (url.unwrap_or_else(|| names::enc_url(src)), natural)
    }

    fn link(&mut self, href: &str) -> Option<String> {
        let lower = href.to_ascii_lowercase();
        if lower.starts_with("call:") {
            self.issues.push(Issue {
                level: "zmiana",
                kind: "link-usunięty",
                note: Some(self.note_rel.clone()),
                message: format!("Usunięto błędny odnośnik „{href}” (zostaje sam tekst)"),
            });
            return None;
        }
        if lower.starts_with("mailto:http") {
            let fixed = &href[7..];
            self.issues.push(Issue {
                level: "zmiana",
                kind: "link-naprawiony",
                note: Some(self.note_rel.clone()),
                message: format!("„{href}” → „{fixed}”"),
            });
            return Some(names::enc_url(fixed));
        }
        if self.is_export_file(href) {
            let base = Self::basename(href).to_string();
            let was_new = !self.by_src.contains_key(&base);
            let url = self.attach(&base, |me| {
                let re = regex::Regex::new(r"^_amn_attached_file_\[\d+\]_").unwrap();
                let clean = re.replace(&base, "").to_string();
                let stem = Path::new(&clean).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let ext = Path::new(&clean).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                let mut n = 1;
                loop {
                    let cand = if n == 1 { clean.clone() } else { format!("{stem} ({n}){ext}") };
                    if me.names.insert(cand.to_lowercase()) {
                        return cand;
                    }
                    n += 1;
                }
            });
            if was_new && url.is_some() {
                self.attachments += 1;
            }
            return Some(url.unwrap_or_else(|| names::enc_url(href)));
        }
        if lower.starts_with("file:") {
            let p = href[5..].trim_start_matches('/');
            let win = p.replace('/', "\\");
            if !Path::new(&win).exists() {
                self.issues.push(Issue {
                    level: "ostrzeżenie",
                    kind: "link-zewnętrzny-martwy",
                    note: Some(self.note_rel.clone()),
                    message: format!("Link zewnętrzny wskazuje nieistniejący plik: {win}"),
                });
            }
            return Some(format!("file:///{}", names::enc_url_path(&p.replace('\\', "/"))));
        }
        Some(names::enc_url(href))
    }
}

pub fn run(opts: &Options, progress: &mut dyn FnMut(usize, usize)) -> Result<Report, String> {
    let started = chrono::Local::now().to_rfc3339();
    if opts.vault.exists() {
        let mut rd = fs::read_dir(&opts.vault).map_err(|e| format!("Nie można odczytać katalogu Skarbca: {e}"))?;
        if rd.next().is_some() {
            return Err("Katalog Skarbca nie jest pusty. Wskaż pusty albo nowy katalog.".into());
        }
    }
    fs::create_dir_all(&opts.vault).map_err(|e| format!("Nie można utworzyć Skarbca: {e}"))?;

    let html = fs::read_to_string(&opts.export_html).map_err(|e| format!("Nie można odczytać eksportu: {e}"))?;
    let stem = opts.export_html.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let files_marker = format!("{stem} - files");
    let files_dir = opts.export_html.with_file_name(&files_marker);

    let entries = parse::split_entries(&html);
    if entries.is_empty() {
        return Err("To nie wygląda na Eksport AMNO: nie znaleziono nagłówków notatek.".into());
    }

    let mut children = vec![Vec::new(); entries.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut ctx_issues = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if i == 0 {
            stack.push(0);
            continue;
        }
        let d = e.depth.max(2);
        stack.truncate(d - 1);
        let parent = *stack.last().unwrap_or(&0);
        if stack.len() != d - 1 {
            ctx_issues.push(Issue {
                level: "ostrzeżenie",
                kind: "drzewo",
                note: None,
                message: format!("Nieciągła głębokość przy „{}” (linia {}); dołączono do najbliższego rodzica", e.path, e.line),
            });
        }
        children[parent].push(i);
        stack.push(i);
    }

    let mut ctx = Ctx {
        opts,
        counts: Counts {
            notes_in_export: entries.iter().filter(|e| e.kind == Kind::Note).count(),
            folders_in_export: entries.iter().skip(1).filter(|e| e.kind == Kind::Folder).count(),
            ..Default::default()
        },
        entries,
        children,
        files_dir,
        files_marker,
        issues: ctx_issues,
        notes: Vec::new(),
        folder_order: BTreeMap::new(),
        used_files: HashSet::new(),
    };

    let total = ctx.counts.notes_in_export;
    let mut done = 0usize;
    let root_children = ctx.children[0].clone();
    let mut used_root: HashSet<String> = [APP_DIR, TRASH_DIR].iter().map(|s| s.to_string()).collect();
    let mut order = 0;
    let mut used_trash: HashSet<String> = HashSet::new();
    for c in root_children {
        let e = &ctx.entries[c];
        let is_trash = e.kind == Kind::Folder && (e.title == "Kosz" || e.title == "Recycle Bin");
        if is_trash {
            // Folder kosza AMNO nie staje się Folderem; jego zawartość trafia do Kosza Skarbca.
            ctx.counts.folders_in_export -= 1;
            if ctx.children[c].is_empty() {
                let t = e.title.clone();
                ctx.issue("zmiana", "kosz-pusty", None, format!("Pominięto pusty folder AMNO „{t}”"));
                continue;
            }
            let trash = PathBuf::from(TRASH_DIR);
            if e.title == "Kosz" {
                // Zawartość Kosza AMNO trafia wprost do Kosza Skarbca.
                let kids = ctx.children[c].clone();
                let mut ord = 0;
                for k in kids {
                    ord += 1;
                    place(&mut ctx, k, &trash, &mut used_trash, ord, true, &mut done, total, progress)?;
                }
            } else {
                place(&mut ctx, c, &trash, &mut used_trash, 1, true, &mut done, total, progress)?;
            }
            continue;
        }
        order += 1;
        place(&mut ctx, c, Path::new(""), &mut used_root, order, false, &mut done, total, progress)?;
    }

    // Pliki z katalogu eksportu, do których nie prowadzi żadna Notatka.
    if let Ok(rd) = fs::read_dir(&ctx.files_dir) {
        let orphan_dir = opts.vault.join(APP_DIR).join("import").join("nieprzypisane");
        for f in rd.flatten() {
            if !f.path().is_file() {
                continue;
            }
            ctx.counts.files_in_export += 1;
            let name = f.file_name().to_string_lossy().to_string();
            if !ctx.used_files.contains(&name) {
                ctx.counts.files_unreferenced += 1;
                fs::create_dir_all(&orphan_dir).map_err(|e| e.to_string())?;
                fs::copy(f.path(), orphan_dir.join(&name)).map_err(|e| e.to_string())?;
                ctx.issue(
                    "ostrzeżenie",
                    "plik-nieprzypisany",
                    None,
                    format!("Plik „{name}” nie jest użyty w żadnej Notatce; skopiowano do {APP_DIR}/import/nieprzypisane"),
                );
            }
        }
    } else {
        ctx.issue("ostrzeżenie", "brak-katalogu-plików", None, format!("Nie znaleziono katalogu „{}”", ctx.files_dir.display()));
    }

    if ctx.counts.notes_written != ctx.counts.notes_in_export {
        let (a, b) = (ctx.counts.notes_written, ctx.counts.notes_in_export);
        ctx.issue("błąd", "liczba-notatek", None, format!("Zapisano {a} Notatek, w eksporcie jest {b}"));
    }

    let app = opts.vault.join(APP_DIR);
    fs::create_dir_all(app.join("import")).map_err(|e| e.to_string())?;
    let order_json = serde_json::to_string_pretty(&ctx.folder_order).unwrap();
    fs::write(app.join("kolejnosc-folderow.json"), order_json).map_err(|e| e.to_string())?;

    let errors = ctx.issues.iter().filter(|i| i.level == "błąd").count();
    let warnings = ctx.issues.iter().filter(|i| i.level == "ostrzeżenie").count();
    let report = Report {
        ok: errors == 0,
        source: opts.export_html.to_string_lossy().to_string(),
        vault: opts.vault.to_string_lossy().to_string(),
        date: opts.date.clone(),
        started,
        finished: chrono::Local::now().to_rfc3339(),
        counts: ctx.counts.clone(),
        errors,
        warnings,
        issues: ctx.issues.clone(),
        notes: ctx.notes.clone(),
    };
    fs::write(app.join("import").join("raport.json"), serde_json::to_string_pretty(&report).unwrap())
        .map_err(|e| e.to_string())?;
    fs::write(app.join("import").join("raport.md"), report_md(&report)).map_err(|e| e.to_string())?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn place(
    ctx: &mut Ctx,
    idx: usize,
    dir: &Path,
    used: &mut HashSet<String>,
    order: usize,
    in_trash: bool,
    done: &mut usize,
    total: usize,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    let e = ctx.entries[idx].clone();
    let kids = ctx.children[idx].clone();
    let blocks = if e.kind == Kind::Note { model::parse_body(&e.body_html) } else { Vec::new() };
    let (title, generated) = if e.title.trim().is_empty() {
        (generated_title(&blocks), true)
    } else {
        (e.title.trim().to_string(), false)
    };
    let base = names::sanitize(&title);

    if e.kind == Kind::Folder || !kids.is_empty() {
        let name = names::unique(&base, &[""], used);
        let folder_rel = dir.join(&name);
        fs::create_dir_all(ctx.opts.vault.join(&folder_rel)).map_err(|er| er.to_string())?;
        ctx.counts.folders_written += 1;
        ctx.folder_order.insert(rel_str(&folder_rel), order);
        if name != title {
            ctx.issue("zmiana", "nazwa-pliku", Some(&rel_str(&folder_rel)), format!("Folder „{title}” zapisany jako „{name}”"));
        }
        let mut used_in: HashSet<String> = HashSet::new();
        let mut ord = 0;
        if e.kind == Kind::Note {
            // Notatka z dziećmi: Folder z Notatką folderu o tej samej nazwie.
            ord += 1;
            write_note(ctx, &e, &blocks, &title, generated, &folder_rel, &mut used_in, ord, in_trash, Some(&name))?;
            *done += 1;
            progress(*done, total);
        }
        for k in kids {
            ord += 1;
            place(ctx, k, &folder_rel, &mut used_in, ord, in_trash, done, total, progress)?;
        }
        return Ok(());
    }

    write_note(ctx, &e, &blocks, &title, generated, dir, used, order, in_trash, None)?;
    *done += 1;
    progress(*done, total);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_note(
    ctx: &mut Ctx,
    e: &Entry,
    blocks: &[model::Block],
    title: &str,
    generated: bool,
    dir: &Path,
    used: &mut HashSet<String>,
    order: usize,
    in_trash: bool,
    forced_name: Option<&str>,
) -> Result<(), String> {
    let base = forced_name.map(str::to_string).unwrap_or_else(|| names::sanitize(title));
    let stem = names::unique(&base, &[".md", ".assets"], used);
    let rel = dir.join(format!("{stem}.md"));
    let rel_s = rel_str(&rel);
    let assets_name = format!("{stem}.assets");

    let mut r = NoteResolver {
        files_dir: &ctx.files_dir,
        files_marker: &ctx.files_marker,
        assets_name: assets_name.clone(),
        assets_abs: ctx.opts.vault.join(dir).join(&assets_name),
        note_rel: rel_s.clone(),
        by_src: HashMap::new(),
        names: HashSet::new(),
        copies: Vec::new(),
        img_n: 0,
        issues: Vec::new(),
        images: 0,
        attachments: 0,
    };
    let body = md::blocks_to_md(blocks, &mut r);
    let mut fm = String::from("---\n");
    fm.push_str(&format!("title: {}\n", serde_json::to_string(title).unwrap()));
    fm.push_str(&format!("order: {order}\n"));
    fm.push_str(&format!("imported: {}\n", ctx.opts.date));
    fm.push_str("---\n\n");
    let abs = ctx.opts.vault.join(&rel);
    if let Some(p) = abs.parent() {
        fs::create_dir_all(p).map_err(|er| er.to_string())?;
    }
    fs::write(&abs, format!("{fm}{body}")).map_err(|er| format!("{}: {er}", abs.display()))?;

    let NoteResolver { copies, issues, images, attachments, .. } = r;
    ctx.issues.extend(issues);
    ctx.counts.images += images;
    ctx.counts.attachments += attachments;
    for (src, dest, src_name) in copies {
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(|er| er.to_string())?;
        }
        fs::copy(&src, &dest).map_err(|er| format!("{}: {er}", dest.display()))?;
        let same = sha256_file(&src).ok() == sha256_file(&dest).ok();
        if !same {
            ctx.issue("błąd", "suma-kontrolna", Some(&rel_s), format!("Kopia „{src_name}” różni się od oryginału"));
        }
        ctx.counts.files_copied += 1;
        ctx.used_files.insert(src_name);
    }

    // Weryfikacja: ten sam tekst, te same obrazy i odnośniki.
    let expected = verify::html_extract(&e.body_html, true);
    let actual = verify::md_extract(&body);
    if let Some((exp, act)) = verify::first_difference(&expected.text, &actual.text) {
        ctx.issue("błąd", "tekst-niezgodny", Some(&rel_s), format!("Tekst różni się od eksportu.\n  oczekiwano: …{exp}…\n  jest:       …{act}…"));
    }
    if expected.images != actual.images {
        ctx.issue("błąd", "obrazy", Some(&rel_s), format!("Obrazów w eksporcie: {}, w Notatce: {}", expected.images, actual.images));
    }
    if expected.links != actual.links {
        ctx.issue("ostrzeżenie", "odnośniki", Some(&rel_s), format!("Odnośników w eksporcie: {}, w Notatce: {}", expected.links, actual.links));
    }

    if generated {
        ctx.issue("zmiana", "tytuł-zastępczy", Some(&rel_s), format!("Notatka bez tytułu; nadano Tytuł zastępczy „{title}”"));
    } else if stem != title {
        ctx.issue("zmiana", "nazwa-pliku", Some(&rel_s), format!("Notatka „{title}” zapisana jako „{stem}.md”"));
    }
    ctx.counts.notes_written += 1;
    if in_trash {
        ctx.counts.notes_in_trash += 1;
    }
    ctx.notes.push(NoteMap { amno: e.path.clone(), path: rel_s, title: title.to_string(), generated_title: generated });
    Ok(())
}

fn report_md(r: &Report) -> String {
    let c = &r.counts;
    let mut s = format!(
        "# Raport importu\n\n- Źródło: {}\n- Skarbiec: {}\n- Data: {}\n- Wynik: **{}**\n\n## Liczby\n\n| | |\n| --- | --- |\n\
| Notatki w eksporcie | {} |\n| Notatki zapisane | {} |\n| w tym w Koszu | {} |\n| Foldery w eksporcie | {} |\n| Foldery zapisane | {} |\n\
| Obrazy | {} |\n| Załączniki | {} |\n| Pliki w eksporcie | {} |\n| Pliki skopiowane | {} |\n| Pliki nieprzypisane | {} |\n\n",
        r.source,
        r.vault,
        r.date,
        if r.ok { "bez błędów" } else { "są błędy" },
        c.notes_in_export,
        c.notes_written,
        c.notes_in_trash,
        c.folders_in_export,
        c.folders_written,
        c.images,
        c.attachments,
        c.files_in_export,
        c.files_copied,
        c.files_unreferenced,
    );
    for (level, head) in [("błąd", "Błędy"), ("ostrzeżenie", "Ostrzeżenia"), ("zmiana", "Zmiany")] {
        let items: Vec<&Issue> = r.issues.iter().filter(|i| i.level == level).collect();
        s.push_str(&format!("## {head} ({})\n\n", items.len()));
        for i in items {
            let note = i.note.as_deref().map(|n| format!("`{n}`: ")).unwrap_or_default();
            s.push_str(&format!("- [{}] {note}{}\n", i.kind, i.message.replace('\n', "\n  ")));
        }
        s.push('\n');
    }
    s
}
