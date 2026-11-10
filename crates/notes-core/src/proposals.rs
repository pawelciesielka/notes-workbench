//! Propozycje powiązań i Tagów od AI (np. Claude przez serwer MCP), czekające na decyzję użytkownika.
//!
//! Kolejka leży w Skarbcu (`.notes-workbench/propozycje.json`), bo decyzje użytkownika, zwłaszcza
//! odrzucenia, są częścią jego danych: odrzucona Propozycja nie wraca, także po przeinstalowaniu
//! aplikacji. Zaakceptowana znika z kolejki i trafia do frontmattera Notatki (ADR 0002):
//! powiązanie jako `powiazane: ["[[…]]"]`, Tag jako `tags: [...]`.
//! Nowa Notatka i zmiana treści Notatki też czekają tu na akceptację; zmiana treści zapamiętuje treść,
//! od której wyszła, i nie nadpisze Notatki zmienionej w międzyczasie.

use crate::index::{self, Index};
use crate::vault::{self, APP_DIR};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Relation,
    Tag,
    /// Nowa Notatka: `note` to Folder, `title` i `body` jej treść.
    Note,
    /// Nowa treść istniejącej Notatki (frontmatter zostaje): `body`, a `base` to treść w chwili Propozycji.
    Edit,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Rejected,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub kind: Kind,
    /// Notatka, do której frontmattera trafi wpis po akceptacji.
    pub note: String,
    /// Dla powiązania: Notatka docelowa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Dla Tagu: nazwa bez `#`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Dla nowej Notatki: tytuł.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Dla nowej Notatki i zmiany: proponowana treść (Markdown, bez frontmattera).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Dla zmiany: treść Notatki, od której wyszła Propozycja (do porównania i wykrycia konfliktu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub reason: String,
    /// Kto zaproponował (np. „Claude (MCP)”).
    pub source: String,
    pub created: String,
    pub status: Status,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    proposals: Vec<Proposal>,
}

/// Wynik dodania jednej Propozycji (dla AI, żeby wiedziało, co się stało).
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Added {
    Added(u64),
    AlreadyPending,
    RejectedBefore,
    AlreadyLinked,
    NotFound(String),
    Invalid(String),
}

fn file_path(vault: &Path) -> PathBuf {
    vault.join(APP_DIR).join("propozycje.json")
}

fn load(vault: &Path) -> Result<File, String> {
    match fs::read_to_string(file_path(vault)) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("propozycje.json: {e}")),
        Err(_) => Ok(File::default()),
    }
}

fn save(vault: &Path, f: &File) -> Result<(), String> {
    let p = file_path(vault);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(f).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Powiązanie nie ma kierunku: para (A, B) to to samo co (B, A).
fn same_pair(p: &Proposal, a: &str, b: &str) -> bool {
    p.kind == Kind::Relation
        && ((p.note == a && p.target.as_deref() == Some(b)) || (p.note == b && p.target.as_deref() == Some(a)))
}

pub fn list(vault: &Path) -> Result<Vec<Proposal>, String> {
    Ok(load(vault)?.proposals)
}

pub fn pending(vault: &Path) -> Result<Vec<Proposal>, String> {
    Ok(load(vault)?.proposals.into_iter().filter(|p| p.status == Status::Pending).collect())
}

/// Dodaje Propozycję powiązania. `hidden` to Notatki niedostępne dla AI.
pub fn add_relation(vault: &Path, idx: &Index, from: &str, to: &str, reason: &str, source: &str) -> Result<Added, String> {
    if from == to {
        return Ok(Added::Invalid("Notatka nie może być powiązana sama ze sobą.".into()));
    }
    if reason.trim().is_empty() {
        return Ok(Added::Invalid("Podaj uzasadnienie (reason).".into()));
    }
    if idx.linked(from, to) {
        return Ok(Added::AlreadyLinked);
    }
    let mut f = load(vault)?;
    if let Some(p) = f.proposals.iter().find(|p| same_pair(p, from, to)) {
        return Ok(if p.status == Status::Rejected { Added::RejectedBefore } else { Added::AlreadyPending });
    }
    f.next_id += 1;
    let id = f.next_id;
    f.proposals.push(Proposal {
        id,
        kind: Kind::Relation,
        note: from.to_string(),
        target: Some(to.to_string()),
        tag: None,
        title: None,
        body: None,
        base: None,
        reason: reason.trim().to_string(),
        source: source.to_string(),
        created: now(),
        status: Status::Pending,
    });
    save(vault, &f)?;
    Ok(Added::Added(id))
}

/// Dodaje Propozycję Tagu.
pub fn add_tag(vault: &Path, idx: &Index, note: &str, tag: &str, reason: &str, source: &str) -> Result<Added, String> {
    let t = tag.trim().trim_start_matches('#').to_string();
    let valid = !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || "_-/".contains(c)) && !t.chars().all(|c| c.is_ascii_digit());
    if !valid {
        return Ok(Added::Invalid(format!("„{tag}” nie jest poprawnym Tagiem (litery, cyfry, _ - /, bez spacji).")));
    }
    if index::tag_key(&t) == index::NOAI_TAG {
        return Ok(Added::Invalid("Tagu noai może używać tylko użytkownik.".into()));
    }
    if idx.has_tag(note, &t) {
        return Ok(Added::AlreadyLinked);
    }
    let mut f = load(vault)?;
    let key = index::tag_key(&t);
    if let Some(p) = f
        .proposals
        .iter()
        .find(|p| p.kind == Kind::Tag && p.note == note && p.tag.as_deref().map(index::tag_key).as_deref() == Some(&key))
    {
        return Ok(if p.status == Status::Rejected { Added::RejectedBefore } else { Added::AlreadyPending });
    }
    f.next_id += 1;
    let id = f.next_id;
    f.proposals.push(Proposal {
        id,
        kind: Kind::Tag,
        note: note.to_string(),
        target: None,
        tag: Some(t),
        title: None,
        body: None,
        base: None,
        reason: reason.trim().to_string(),
        source: source.to_string(),
        created: now(),
        status: Status::Pending,
    });
    save(vault, &f)?;
    Ok(Added::Added(id))
}

/// Największa proponowana treść (znaki), żeby kolejka nie puchła.
pub const MAX_BODY: usize = 200_000;

fn push(vault: &Path, mut p: Proposal) -> Result<Added, String> {
    let mut f = load(vault)?;
    f.next_id += 1;
    p.id = f.next_id;
    let id = p.id;
    f.proposals.push(p);
    save(vault, &f)?;
    Ok(Added::Added(id))
}

fn text_proposal(kind: Kind, note: &str, title: Option<String>, body: &str, base: Option<String>, reason: &str, source: &str) -> Proposal {
    Proposal {
        id: 0,
        kind,
        note: note.to_string(),
        target: None,
        tag: None,
        title,
        body: Some(body.to_string()),
        base,
        reason: reason.trim().to_string(),
        source: source.to_string(),
        created: now(),
        status: Status::Pending,
    }
}

/// Dodaje Propozycję nowej Notatki w Folderze `folder` (pusty: główny poziom Skarbca).
pub fn add_note(vault: &Path, folder: &str, title: &str, body: &str, reason: &str, source: &str) -> Result<Added, String> {
    let folder = folder.trim().trim_matches('/');
    if title.trim().is_empty() {
        return Ok(Added::Invalid("Podaj tytuł (title).".into()));
    }
    if reason.trim().is_empty() {
        return Ok(Added::Invalid("Podaj uzasadnienie (reason).".into()));
    }
    if body.chars().count() > MAX_BODY {
        return Ok(Added::Invalid(format!("Treść jest za długa (najwyżej {MAX_BODY} znaków).")));
    }
    let bad = folder.split('/').any(|s| s.is_empty() || s.starts_with('.') || s.ends_with(".assets"));
    if !folder.is_empty() && (bad || !vault.join(folder).is_dir()) {
        return Ok(Added::NotFound(format!("Nie ma Folderu „{folder}”. Użyj list_notes, żeby znaleźć Folder.")));
    }
    let f = load(vault)?;
    let dup = f
        .proposals
        .iter()
        .find(|p| p.kind == Kind::Note && p.note == folder && p.title.as_deref().map(str::trim) == Some(title.trim()));
    if let Some(p) = dup {
        return Ok(if p.status == Status::Rejected { Added::RejectedBefore } else { Added::AlreadyPending });
    }
    push(vault, text_proposal(Kind::Note, folder, Some(title.trim().to_string()), body, None, reason, source))
}

/// Dodaje Propozycję nowej treści Notatki `note`. Czekająca zmiana tej samej Notatki jest zastępowana nową.
pub fn add_edit(vault: &Path, note: &str, body: &str, reason: &str, source: &str) -> Result<Added, String> {
    if reason.trim().is_empty() {
        return Ok(Added::Invalid("Podaj uzasadnienie (reason).".into()));
    }
    if body.chars().count() > MAX_BODY {
        return Ok(Added::Invalid(format!("Treść jest za długa (najwyżej {MAX_BODY} znaków).")));
    }
    let current = vault::read_note(vault, note)?.body;
    if current.trim_end() == body.trim_end() {
        return Ok(Added::Invalid("Proponowana treść jest taka sama jak obecna.".into()));
    }
    let mut f = load(vault)?;
    let before = f.proposals.len();
    f.proposals.retain(|p| !(p.kind == Kind::Edit && p.note == note && p.status == Status::Pending));
    if f.proposals.len() != before {
        save(vault, &f)?;
    }
    push(vault, text_proposal(Kind::Edit, note, None, body, Some(current), reason, source))
}

/// Akceptuje: zapisuje w Notatce i usuwa z kolejki. Zwraca zmienioną albo utworzoną Notatkę.
pub fn accept(vault: &Path, idx: &Index, id: u64) -> Result<String, String> {
    let mut f = load(vault)?;
    let pos = f.proposals.iter().position(|p| p.id == id && p.status == Status::Pending).ok_or("Nie ma takiej Propozycji")?;
    let p = f.proposals[pos].clone();
    match p.kind {
        Kind::Relation => {
            let target = p.target.as_deref().ok_or("Brak Notatki docelowej")?;
            // Najkrótszy cel Linku, który jednoznacznie wskazuje Notatkę (jak w Obsidianie).
            let stem = Path::new(target).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let link = if idx.resolve(&p.note, &stem).as_deref() == Some(target) {
                stem
            } else {
                target.trim_end_matches(".md").to_string()
            };
            vault::add_fm_list_item(vault, &p.note, "powiazane", &format!("[[{link}]]"))?;
        }
        Kind::Tag => {
            vault::set_fm_tag(vault, &p.note, p.tag.as_deref().unwrap_or_default(), true)?;
        }
        Kind::Note => {
            let path = vault::create_note(vault, &p.note, p.title.as_deref().unwrap_or_default())?;
            let raw = vault::read_note(vault, &path)?.raw;
            vault::save_body(vault, &path, p.body.as_deref().unwrap_or_default(), &raw)?;
            f.proposals.remove(pos);
            save(vault, &f)?;
            return Ok(path);
        }
        Kind::Edit => {
            let n = vault::read_note(vault, &p.note)?;
            if Some(n.body.as_str()) != p.base.as_deref() {
                return Err(format!(
                    "Notatka „{}” zmieniła się od czasu Propozycji, więc jej nie nadpisuję. Odrzuć Propozycję i poproś AI o nową.",
                    n.title
                ));
            }
            vault::save_body(vault, &p.note, p.body.as_deref().unwrap_or_default(), &n.raw)?;
        }
    }
    f.proposals.remove(pos);
    save(vault, &f)?;
    Ok(p.note)
}

/// Odrzuca: zostaje w pliku jako odrzucona, żeby AI nie zaproponowało jej ponownie.
pub fn reject(vault: &Path, id: u64) -> Result<(), String> {
    let mut f = load(vault)?;
    let p = f.proposals.iter_mut().find(|p| p.id == id).ok_or("Nie ma takiej Propozycji")?;
    p.status = Status::Rejected;
    save(vault, &f)
}

/// Aktualizuje ścieżki Notatek w kolejce po ich przeniesieniu (stara → nowa).
pub fn rename_paths(vault: &Path, moved: &std::collections::HashMap<String, String>) -> Result<(), String> {
    if moved.is_empty() || !file_path(vault).exists() {
        return Ok(());
    }
    let mut f = load(vault)?;
    let mut changed = false;
    for p in &mut f.proposals {
        if p.kind == Kind::Note {
            continue;
        }
        if let Some(n) = moved.get(&p.note) {
            p.note = n.clone();
            changed = true;
        }
        if let Some(n) = p.target.as_ref().and_then(|t| moved.get(t)) {
            p.target = Some(n.clone());
            changed = true;
        }
    }
    if changed {
        save(vault, &f)?;
    }
    Ok(())
}

/// Usuwa z kolejki Propozycje dotyczące Notatek, których już nie ma (np. przeniesionych do Kosza).
pub fn prune(vault: &Path, idx: &Index) -> Result<usize, String> {
    let mut f = load(vault)?;
    let before = f.proposals.len();
    f.proposals.retain(|p| {
        p.status == Status::Rejected
            || p.kind == Kind::Note
            || (idx.title(&p.note).is_some() && p.target.as_ref().is_none_or(|t| idx.title(t).is_some()))
    });
    let removed = before - f.proposals.len();
    if removed > 0 {
        save(vault, &f)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_accept_reject() {
        let dir = std::env::temp_dir().join(format!("nw-prop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("A")).unwrap();
        fs::write(dir.join("A/Router.md"), "---\ntitle: \"Router\"\n---\n\nKonfiguracja\n").unwrap();
        fs::write(dir.join("VPN.md"), "Tunel\n").unwrap();
        fs::write(dir.join("Sieć.md"), "Ogólnie [[VPN]]\n").unwrap();
        let idx = Index::build(&dir);

        let r = add_relation(&dir, &idx, "A/Router.md", "VPN.md", "Oba o dostępie do sieci firmowej.", "Claude (MCP)").unwrap();
        let Added::Added(id) = r else { panic!("{r:?}") };
        assert_eq!(add_relation(&dir, &idx, "VPN.md", "A/Router.md", "x", "c").unwrap(), Added::AlreadyPending);
        assert_eq!(add_relation(&dir, &idx, "Sieć.md", "VPN.md", "x", "c").unwrap(), Added::AlreadyLinked);
        assert!(matches!(add_tag(&dir, &idx, "VPN.md", "noai", "x", "c").unwrap(), Added::Invalid(_)));
        let Added::Added(tid) = add_tag(&dir, &idx, "VPN.md", "#sieć", "Notatka o sieci.", "c").unwrap() else { panic!() };
        let Added::Added(t2) = add_tag(&dir, &idx, "VPN.md", "praca", "x", "c").unwrap() else { panic!() };
        assert_eq!(pending(&dir).unwrap().len(), 3);

        assert_eq!(accept(&dir, &idx, id).unwrap(), "A/Router.md");
        let n = vault::read_note(&dir, "A/Router.md").unwrap();
        assert!(n.frontmatter.contains("powiazane: [\"[[VPN]]\"]"), "{}", n.frontmatter);
        assert_eq!(n.body, "Konfiguracja\n");
        accept(&dir, &idx, tid).unwrap();
        assert!(vault::read_note(&dir, "VPN.md").unwrap().frontmatter.contains("tags: [\"sieć\"]"));
        reject(&dir, t2).unwrap();
        assert!(pending(&dir).unwrap().is_empty());
        let idx = Index::build(&dir);
        assert_eq!(add_tag(&dir, &idx, "VPN.md", "Praca", "x", "c").unwrap(), Added::RejectedBefore);
        assert_eq!(add_relation(&dir, &idx, "VPN.md", "A/Router.md", "x", "c").unwrap(), Added::AlreadyLinked);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn note_and_edit_proposals() {
        let dir = std::env::temp_dir().join(format!("nw-prop-note-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Projekty")).unwrap();
        fs::write(dir.join("Projekty/VPN.md"), "---\ntitle: \"VPN\"\norder: 2\n---\n\nTunel do biura\n").unwrap();

        assert!(matches!(add_note(&dir, "Brak", "X", "y", "r", "c").unwrap(), Added::NotFound(_)));
        let Added::Added(nid) = add_note(&dir, "Projekty", "Router", "# Router\nKonfiguracja", "Brakuje opisu.", "c").unwrap() else {
            panic!()
        };
        assert_eq!(add_note(&dir, "Projekty", "Router", "inna", "r", "c").unwrap(), Added::AlreadyPending);
        let Added::Added(e1) = add_edit(&dir, "Projekty/VPN.md", "Tunel do biura\n\nPort 1194", "Port.", "c").unwrap() else {
            panic!()
        };
        // Nowsza zmiana tej samej Notatki zastępuje starszą.
        let Added::Added(e2) = add_edit(&dir, "Projekty/VPN.md", "Tunel do biura\n\nPort 443", "Port 443.", "c").unwrap() else {
            panic!()
        };
        assert!(pending(&dir).unwrap().iter().all(|p| p.id != e1));
        assert!(matches!(add_edit(&dir, "Projekty/VPN.md", "Tunel do biura\n", "r", "c").unwrap(), Added::Invalid(_)));

        let idx = Index::build(&dir);
        let path = accept(&dir, &idx, nid).unwrap();
        assert_eq!(path, "Projekty/Router.md");
        let n = vault::read_note(&dir, &path).unwrap();
        assert_eq!(n.title, "Router");
        assert!(n.body.contains("Konfiguracja"));

        assert_eq!(accept(&dir, &idx, e2).unwrap(), "Projekty/VPN.md");
        let v = vault::read_note(&dir, "Projekty/VPN.md").unwrap();
        assert!(v.body.contains("Port 443") && v.frontmatter.contains("order: 2"));

        // Konflikt: Notatka zmieniona po Propozycji nie jest nadpisywana.
        let Added::Added(e3) = add_edit(&dir, "Projekty/VPN.md", "całkiem nowa", "r", "c").unwrap() else { panic!() };
        vault::save_body(&dir, "Projekty/VPN.md", "zmiana użytkownika", &v.raw).unwrap();
        assert!(accept(&dir, &idx, e3).is_err());
        assert_eq!(vault::read_note(&dir, "Projekty/VPN.md").unwrap().body.trim(), "zmiana użytkownika");
        fs::remove_dir_all(&dir).unwrap();
    }
}
