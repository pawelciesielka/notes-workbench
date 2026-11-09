//! Odczyt Skarbca: drzewo Folderów i Notatek, treść Notatki, Kosz.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const APP_DIR: &str = amno_import::APP_DIR;
pub const TRASH_DIR: &str = amno_import::TRASH_DIR;

#[derive(Serialize, Clone)]
pub struct TreeNode {
    pub name: String,
    /// Ścieżka względem Skarbca, z ukośnikami `/`.
    pub path: String,
    pub kind: &'static str,
    pub title: String,
    pub order: Option<i64>,
    /// Notatka folderu (ścieżka), otwierana po wybraniu Folderu.
    pub folder_note: Option<String>,
    pub children: Vec<TreeNode>,
}

#[derive(Serialize)]
pub struct NoteData {
    pub path: String,
    pub title: String,
    pub frontmatter: String,
    pub body: String,
    pub raw: String,
}

/// Rozdziela frontmatter (bez ograniczników `---`) i treść.
pub fn split_frontmatter(raw: &str) -> (&str, &str) {
    let r = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let Some(rest) = r.strip_prefix("---\n").or_else(|| r.strip_prefix("---\r\n")) else {
        return ("", r);
    };
    let mut pos = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let fm = &rest[..pos];
            let body = &rest[pos + line.len()..];
            return (fm, body.trim_start_matches(['\r', '\n']));
        }
        pos += line.len();
    }
    ("", r)
}

pub fn fm_value(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        if let Some(v) = line.strip_prefix(&format!("{key}:")) {
            let v = v.trim();
            if v.starts_with('"') {
                if let Ok(s) = serde_json::from_str::<String>(v) {
                    return Some(s);
                }
            }
            let v = v.trim_matches(|c| c == '\'' || c == '"');
            return Some(v.to_string());
        }
    }
    None
}

fn read_head(path: &Path) -> String {
    // Frontmatter jest na początku pliku; 4 KB wystarcza.
    use std::io::Read;
    let mut buf = vec![0u8; 4096];
    let n = fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn folder_orders(vault: &Path) -> HashMap<String, i64> {
    fs::read_to_string(vault.join(APP_DIR).join("kolejnosc-folderow.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn read_dir_tree(vault: &Path, dir: &Path, orders: &HashMap<String, i64>) -> Vec<TreeNode> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            if name.ends_with(".assets") {
                continue;
            }
            let r = rel(vault, &p);
            let mut children = read_dir_tree(vault, &p, orders);
            let fnote = children.iter().position(|c| c.kind == "note" && c.name == name);
            let folder_note = fnote.map(|i| children.remove(i));
            out.push(TreeNode {
                title: name.clone(),
                name,
                order: orders.get(&r).copied(),
                path: r,
                kind: "folder",
                folder_note: folder_note.map(|n| n.path),
                children,
            });
        } else if name.to_lowercase().ends_with(".md") {
            let stem = name[..name.len() - 3].to_string();
            let (fm, _) = {
                let head = read_head(&p);
                let (fm, b) = split_frontmatter(&head);
                (fm.to_string(), b.len())
            };
            out.push(TreeNode {
                title: fm_value(&fm, "title").filter(|t| !t.is_empty()).unwrap_or_else(|| stem.clone()),
                order: fm_value(&fm, "order").and_then(|o| o.parse().ok()),
                name: stem,
                path: rel(vault, &p),
                kind: "note",
                folder_note: None,
                children: Vec::new(),
            });
        }
    }
    out.sort_by(|a, b| {
        let ka = (a.order.is_none(), a.order.unwrap_or(0), a.title.to_lowercase());
        let kb = (b.order.is_none(), b.order.unwrap_or(0), b.title.to_lowercase());
        ka.cmp(&kb)
    });
    out
}

pub fn tree(vault: &Path, trash: bool) -> Vec<TreeNode> {
    let orders = folder_orders(vault);
    let root = if trash { vault.join(TRASH_DIR) } else { vault.to_path_buf() };
    read_dir_tree(vault, &root, &orders)
}

/// Ścieżka w Skarbcu z adresu względnego; odrzuca wyjście poza Skarbiec.
pub fn resolve(vault: &Path, rel_path: &str) -> Result<PathBuf, String> {
    let rel_path = rel_path.trim_start_matches(['/', '\\']);
    let mut p = vault.to_path_buf();
    for c in Path::new(rel_path).components() {
        match c {
            Component::Normal(s) => p.push(s),
            Component::CurDir => {}
            _ => return Err("Ścieżka poza Skarbcem".into()),
        }
    }
    Ok(p)
}

pub fn read_note(vault: &Path, rel_path: &str) -> Result<NoteData, String> {
    let p = resolve(vault, rel_path)?;
    let raw = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Ok(NoteData {
        path: rel_path.to_string(),
        title: fm_value(fm, "title").filter(|t| !t.is_empty()).unwrap_or(stem),
        frontmatter: fm.to_string(),
        body: body.to_string(),
        raw: raw.clone(),
    })
}

/// Składa plik Notatki z frontmattera (bez ograniczników) i treści.
fn compose(fm: &str, body: &str) -> String {
    if fm.trim().is_empty() {
        return body.to_string();
    }
    let fm = fm.trim_end_matches(['\r', '\n']);
    format!("---\n{fm}\n---\n\n{body}")
}

/// Ustawia (albo dodaje na początku) pole frontmattera; `None` usuwa pole.
fn set_fm(fm: &str, key: &str, value: Option<&str>) -> String {
    let mut lines: Vec<String> = fm.lines().map(str::to_string).collect();
    let prefix = format!("{key}:");
    let pos = lines.iter().position(|l| l.starts_with(&prefix));
    // Usuwamy też ewentualne elementy listy `- …` pod kluczem.
    if let Some(i) = pos {
        let mut j = i + 1;
        while j < lines.len() && (lines[j].starts_with(' ') || lines[j].starts_with('-')) {
            j += 1;
        }
        lines.drain(i..j);
    }
    if let Some(v) = value {
        lines.insert(pos.unwrap_or(0), format!("{key}: {v}"));
    }
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// Zapis atomowy: najpierw plik tymczasowy obok, potem podmiana.
fn write_atomic(p: &Path, content: &str) -> Result<(), String> {
    let tmp = p.with_extension("md.tmp");
    fs::write(&tmp, content).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, p).map_err(|e| format!("{}: {e}", p.display()))
}

fn note_file(vault: &Path, rel_path: &str) -> Result<PathBuf, String> {
    if !rel_path.to_lowercase().ends_with(".md") {
        return Err("To nie jest Notatka".into());
    }
    resolve(vault, rel_path)
}

/// Zapisuje treść Notatki, zachowując jej frontmatter. `expected` to treść pliku, którą
/// edytor wczytał; jeśli plik zmienił się w międzyczasie z zewnątrz, nie nadpisujemy go.
pub fn save_body(vault: &Path, rel_path: &str, body: &str, expected: &str) -> Result<NoteData, String> {
    let p = note_file(vault, rel_path)?;
    let raw = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    if raw != expected {
        return Err("Notatka zmieniła się na dysku poza edytorem. Wczytaj ją ponownie, zanim zapiszesz.".into());
    }
    let (fm, _) = split_frontmatter(&raw);
    write_atomic(&p, &compose(fm, body))?;
    read_note(vault, rel_path)
}

pub fn set_title(vault: &Path, rel_path: &str, title: &str) -> Result<NoteData, String> {
    let p = note_file(vault, rel_path)?;
    let raw = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    let t = title.trim();
    let v = (!t.is_empty()).then(|| serde_json::to_string(t).unwrap());
    write_atomic(&p, &compose(&set_fm(fm, "title", v.as_deref()), body))?;
    read_note(vault, rel_path)
}

/// Dopisuje wartość do listy we frontmatterze (np. `powiazane: ["[[X]]"]`), jeśli jej tam nie ma.
pub fn add_fm_list_item(vault: &Path, rel_path: &str, key: &str, item: &str) -> Result<NoteData, String> {
    let p = note_file(vault, rel_path)?;
    let raw = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    let mut items = crate::index::fm_list(fm, key);
    if !items.iter().any(|x| x.eq_ignore_ascii_case(item)) {
        items.push(item.to_string());
        let fm = set_fm(fm, key, Some(&serde_json::to_string(&items).unwrap()));
        write_atomic(&p, &compose(&fm, body))?;
    }
    read_note(vault, rel_path)
}

/// Dodaje albo usuwa Tag we frontmatterze (`tags: [...]`), nie ruszając treści.
pub fn set_fm_tag(vault: &Path, rel_path: &str, tag: &str, on: bool) -> Result<NoteData, String> {
    let p = note_file(vault, rel_path)?;
    let raw = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    let mut tags: Vec<String> = crate::index::fm_list(fm, "tags").into_iter().map(|t| t.trim_start_matches('#').to_string()).collect();
    let key = tag.to_lowercase();
    tags.retain(|t| t.to_lowercase() != key);
    if on {
        tags.push(tag.to_string());
    }
    let v = (!tags.is_empty()).then(|| serde_json::to_string(&tags).unwrap());
    let fm = set_fm(fm, "tags", v.as_deref());
    // Notatka bez frontmattera dostaje go tylko wtedy, gdy jest co zapisać.
    write_atomic(&p, &compose(&fm, body))?;
    read_note(vault, rel_path)
}

fn used_names(dir: &Path) -> std::collections::HashSet<String> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
        .unwrap_or_default()
}

/// Tworzy Notatkę w Folderze (`""` to korzeń Skarbca); nazwa pliku z tytułu.
pub fn create_note(vault: &Path, folder: &str, title: &str) -> Result<String, String> {
    let dir = resolve(vault, folder)?;
    if folder.starts_with(TRASH_DIR) || folder.starts_with(APP_DIR) {
        return Err("Nie można tworzyć Notatek w tym miejscu".into());
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = title.trim();
    let base = amno_import::names::sanitize(if t.is_empty() { "Nowa Notatka" } else { t });
    let stem = amno_import::names::unique(&base, &[".md", ".assets"], &mut used_names(&dir));
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let fm = format!("title: {}\ncreated: {date}\n", serde_json::to_string(if t.is_empty() { stem.as_str() } else { t }).unwrap());
    let rel = if folder.is_empty() { format!("{stem}.md") } else { format!("{}/{stem}.md", folder.trim_matches('/')) };
    fs::write(dir.join(format!("{stem}.md")), compose(&fm, "")).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Przenosi Notatkę razem z jej Załącznikami (`<nazwa>.assets`) pod nową ścieżkę katalogu.
fn move_note(vault: &Path, rel_path: &str, to_dir_rel: &str) -> Result<String, String> {
    let src = note_file(vault, rel_path)?;
    if !src.exists() {
        return Err(format!("Nie ma Notatki {rel_path}"));
    }
    let stem = src.file_stem().unwrap().to_string_lossy().to_string();
    let dest_dir = resolve(vault, to_dir_rel)?;
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let new_stem = amno_import::names::unique(&stem, &[".md", ".assets"], &mut used_names(&dest_dir));
    let assets = src.with_file_name(format!("{stem}.assets"));
    fs::rename(&src, dest_dir.join(format!("{new_stem}.md"))).map_err(|e| e.to_string())?;
    if assets.is_dir() {
        fs::rename(&assets, dest_dir.join(format!("{new_stem}.assets"))).map_err(|e| e.to_string())?;
    }
    let dir = to_dir_rel.trim_matches('/');
    Ok(if dir.is_empty() { format!("{new_stem}.md") } else { format!("{dir}/{new_stem}.md") })
}

fn parent_rel(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

/// Do Kosza: ta sama ścieżka pod `.kosz/`, żeby dało się przywrócić na miejsce.
pub fn trash_note(vault: &Path, rel_path: &str) -> Result<String, String> {
    if rel_path.starts_with(&format!("{TRASH_DIR}/")) {
        return Err("Notatka jest już w Koszu".into());
    }
    let to = format!("{TRASH_DIR}/{}", parent_rel(rel_path));
    move_note(vault, rel_path, to.trim_end_matches('/'))
}

pub fn restore_note(vault: &Path, rel_path: &str) -> Result<String, String> {
    let inner = rel_path
        .strip_prefix(&format!("{TRASH_DIR}/"))
        .ok_or("Notatka nie jest w Koszu")?;
    move_note(vault, rel_path, parent_rel(inner))
}

pub fn mime(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("txt" | "md" | "sh" | "py" | "xml" | "wsdl" | "json" | "csv") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let (fm, body) = split_frontmatter("---\ntitle: \"A\"\norder: 2\n---\n\nTreść\n");
        assert_eq!(fm, "title: \"A\"\norder: 2\n");
        assert_eq!(body, "Treść\n");
        assert_eq!(fm_value(fm, "title").as_deref(), Some("A"));
        assert_eq!(split_frontmatter("bez").1, "bez");
    }

    #[test]
    fn edits_files() {
        let dir = std::env::temp_dir().join(format!("nw-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("F")).unwrap();
        let p = create_note(&dir, "F", "Moja: notatka").unwrap();
        assert_eq!(p, "F/Moja_ notatka.md");
        let n = read_note(&dir, &p).unwrap();
        assert_eq!(n.title, "Moja: notatka");
        let n = save_body(&dir, &p, "Treść [[X]]\n", &n.raw).unwrap();
        assert!(n.raw.starts_with("---\ntitle: \"Moja: notatka\"\ncreated: "));
        assert_eq!(n.body, "Treść [[X]]\n");
        assert!(save_body(&dir, &p, "inna", "stara").is_err());
        let n = set_title(&dir, &p, "Nowy").unwrap();
        assert_eq!(n.title, "Nowy");
        assert_eq!(n.body, "Treść [[X]]\n");
        let n = set_fm_tag(&dir, &p, "noai", true).unwrap();
        assert!(n.frontmatter.contains("tags: [\"noai\"]"), "{}", n.frontmatter);
        assert_eq!(n.body, "Treść [[X]]\n");
        let n = set_fm_tag(&dir, &p, "noai", false).unwrap();
        assert!(!n.frontmatter.contains("tags"), "{}", n.frontmatter);
        assert_eq!(n.title, "Nowy");
        fs::create_dir_all(dir.join("F/Moja_ notatka.assets")).unwrap();
        let t = trash_note(&dir, &p).unwrap();
        assert_eq!(t, ".kosz/F/Moja_ notatka.md");
        assert!(dir.join(".kosz/F/Moja_ notatka.assets").is_dir());
        assert_eq!(restore_note(&dir, &t).unwrap(), p);
        assert!(dir.join("F/Moja_ notatka.assets").is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_escape() {
        let v = Path::new("C:/v");
        assert!(resolve(v, "../x").is_err());
        assert!(resolve(v, "a/b.md").is_ok());
    }
}
