//! Operacje na drzewie Skarbca: Foldery (nowy, zmiana nazwy, Kosz, przywracanie) i przenoszenie Notatek.
//!
//! Po przeniesieniu poprawiamy Linki, które wskazywały starą ścieżkę (`[[Folder/Notatka]]`) albo starą
//! nazwę pliku Notatki folderu, a także kolejność Folderów i kolejkę Propozycji. Linki po samej nazwie
//! pliku (`[[Notatka]]`) działają dalej bez zmian.

use crate::index::Index;
use crate::proposals;
use crate::vault::{self, parent_rel, resolve, used_names, APP_DIR, TRASH_DIR};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn last(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn stem(p: &str) -> &str {
    last(p).strip_suffix(".md").unwrap_or(last(p))
}

fn check_folder(path: &str) -> Result<(), String> {
    if path.is_empty() || path.starts_with(APP_DIR) || path.split('/').any(|s| s.starts_with('.') || s.ends_with(".assets")) {
        return Err("To nie jest Folder Notatek".into());
    }
    Ok(())
}

/// Notatki (`.md`) w katalogu i podkatalogach, ścieżki względem Skarbca.
fn notes_under(vault: &Path, dir: &str) -> Vec<String> {
    fn walk(vault: &Path, abs: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(abs) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !name.ends_with(".assets") {
                    walk(vault, &p, out);
                }
            } else if name.to_lowercase().ends_with(".md") {
                out.push(p.strip_prefix(vault).unwrap_or(&p).to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(abs) = resolve(vault, dir) {
        walk(vault, &abs, &mut out);
    }
    out
}

/// Poprawia Linki do przeniesionych Notatek. `idx` to indeks sprzed przeniesienia, `moved` mapuje
/// stare ścieżki na nowe. Zwraca liczbę poprawionych Notatek.
pub fn fix_links(vault: &Path, idx: &Index, moved: &HashMap<String, String>) -> usize {
    let mut fixed = 0;
    for note in idx.paths() {
        let mut repl: Vec<(String, String)> = Vec::new();
        for t in idx.link_targets(&note) {
            let Some(old) = idx.resolve(&note, &t) else { continue };
            let Some(new) = moved.get(&old) else { continue };
            let new_target = if t.contains('/') {
                new.trim_end_matches(".md").to_string()
            } else if stem(&old) != stem(new) {
                stem(new).to_string()
            } else {
                continue;
            };
            if new_target != t && !repl.iter().any(|(a, _)| a == &t) {
                repl.push((t, new_target));
            }
        }
        if repl.is_empty() {
            continue;
        }
        let at = moved.get(&note).cloned().unwrap_or(note);
        let Ok(abs) = resolve(vault, &at) else { continue };
        let Ok(mut raw) = fs::read_to_string(&abs) else { continue };
        let before = raw.clone();
        for (a, b) in &repl {
            for end in ["]]", "|", "#"] {
                raw = raw.replace(&format!("[[{a}{end}"), &format!("[[{b}{end}"));
            }
        }
        if raw != before && fs::write(&abs, raw).is_ok() {
            fixed += 1;
        }
    }
    fixed
}

/// Kolejność Folderów i kolejka Propozycji po zmianie ścieżek.
fn after_move(vault: &Path, folder_from: Option<(&str, &str)>, moved: &HashMap<String, String>) {
    if let Some((from, to)) = folder_from {
        let p = vault.join(APP_DIR).join("kolejnosc-folderow.json");
        if let Ok(s) = fs::read_to_string(&p) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, i64>>(&s) {
                let map: HashMap<String, i64> = map
                    .into_iter()
                    .map(|(k, v)| {
                        if k == from {
                            (to.to_string(), v)
                        } else if let Some(rest) = k.strip_prefix(&format!("{from}/")) {
                            (format!("{to}/{rest}"), v)
                        } else {
                            (k, v)
                        }
                    })
                    .collect();
                let _ = fs::write(&p, serde_json::to_string_pretty(&map).unwrap());
            }
        }
    }
    let _ = proposals::rename_paths(vault, moved);
}

/// Nowy Folder w Folderze `parent` (`""` to korzeń Skarbca).
pub fn create_folder(vault: &Path, parent: &str, name: &str) -> Result<String, String> {
    if !parent.is_empty() {
        check_folder(parent)?;
    }
    let dir = resolve(vault, parent)?;
    let base = amno_import::names::sanitize(if name.trim().is_empty() { "Nowy Folder" } else { name });
    let n = amno_import::names::unique(&base, &[""], &mut used_names(&dir));
    fs::create_dir_all(dir.join(&n)).map_err(|e| e.to_string())?;
    Ok(join(parent, &n))
}

/// Zmiana nazwy Folderu razem z jego Notatką folderu; poprawia Linki. Zwraca (nowa ścieżka, poprawione Notatki).
pub fn rename_folder(vault: &Path, idx: &Index, path: &str, new_name: &str) -> Result<(String, usize), String> {
    check_folder(path)?;
    let parent = parent_rel(path);
    let old_name = last(path).to_string();
    let base = amno_import::names::sanitize(new_name);
    if base == old_name {
        return Ok((path.to_string(), 0));
    }
    let mut used = used_names(&resolve(vault, parent)?);
    used.remove(&old_name.to_lowercase());
    let n = amno_import::names::unique(&base, &[""], &mut used);
    let new_path = join(parent, &n);
    let notes = notes_under(vault, path);
    fs::rename(resolve(vault, path)?, resolve(vault, &new_path)?).map_err(|e| e.to_string())?;
    // Notatka folderu ma nazwę Folderu, więc zmienia się razem z nim.
    let new_dir = resolve(vault, &new_path)?;
    let fnote_old = new_dir.join(format!("{old_name}.md"));
    let has_fnote = fnote_old.is_file() && !new_dir.join(format!("{n}.md")).exists();
    if has_fnote {
        fs::rename(&fnote_old, new_dir.join(format!("{n}.md"))).map_err(|e| e.to_string())?;
        let a = new_dir.join(format!("{old_name}.assets"));
        if a.is_dir() {
            let _ = fs::rename(&a, new_dir.join(format!("{n}.assets")));
        }
    }
    let mut moved = HashMap::new();
    for old in notes {
        let rest = &old[path.len() + 1..];
        let new = if has_fnote && rest == format!("{old_name}.md") {
            format!("{new_path}/{n}.md")
        } else {
            format!("{new_path}/{rest}")
        };
        moved.insert(old, new);
    }
    let fixed = fix_links(vault, idx, &moved);
    after_move(vault, Some((path, &new_path)), &moved);
    Ok((new_path, fixed))
}

/// Przenosi Notatkę do innego Folderu; poprawia Linki zapisane ze ścieżką.
pub fn move_note(vault: &Path, idx: &Index, path: &str, folder: &str) -> Result<(String, usize), String> {
    if !folder.is_empty() {
        check_folder(folder)?;
    }
    if parent_rel(path) == folder {
        return Ok((path.to_string(), 0));
    }
    let new = vault::move_note(vault, path, folder)?;
    let moved = HashMap::from([(path.to_string(), new.clone())]);
    let fixed = fix_links(vault, idx, &moved);
    after_move(vault, None, &moved);
    Ok((new, fixed))
}

/// Przenosi katalog pod nowy katalog nadrzędny, z unikalną nazwą.
fn move_dir(vault: &Path, path: &str, to_parent: &str) -> Result<String, String> {
    let dest_parent = resolve(vault, to_parent)?;
    fs::create_dir_all(&dest_parent).map_err(|e| e.to_string())?;
    let n = amno_import::names::unique(last(path), &[""], &mut used_names(&dest_parent));
    let new_path = join(to_parent, &n);
    fs::rename(resolve(vault, path)?, resolve(vault, &new_path)?).map_err(|e| e.to_string())?;
    Ok(new_path)
}

/// Folder do Kosza (pod tą samą ścieżką w `.kosz/`, żeby dało się go przywrócić).
pub fn trash_folder(vault: &Path, path: &str) -> Result<String, String> {
    check_folder(path)?;
    let parent = parent_rel(path);
    let to = if parent.is_empty() { TRASH_DIR.to_string() } else { join(TRASH_DIR, parent) };
    move_dir(vault, path, &to)
}

pub fn restore_folder(vault: &Path, path: &str) -> Result<String, String> {
    let inner = path.strip_prefix(&format!("{TRASH_DIR}/")).ok_or("Folder nie jest w Koszu")?;
    move_dir(vault, path, parent_rel(inner))
}

/// Ukrywa Folder przed AI przez Tag `noai` na jego Notatce folderu (tworzy ją, jeśli jej nie ma).
pub fn set_folder_noai(vault: &Path, path: &str, on: bool) -> Result<String, String> {
    check_folder(path)?;
    let name = last(path);
    let fnote = format!("{path}/{name}.md");
    if !resolve(vault, &fnote)?.is_file() {
        if !on {
            return Ok(fnote);
        }
        let created = vault::create_note(vault, path, name)?;
        if created != fnote {
            return Err("Nie udało się utworzyć Notatki folderu".into());
        }
    }
    vault::set_fm_tag(vault, &fnote, crate::index::NOAI_TAG, on)?;
    Ok(fnote)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, p: &str, s: &str) {
        let f = dir.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, s).unwrap();
    }

    #[test]
    fn rename_move_trash_restore() {
        let dir = std::env::temp_dir().join(format!("nw-ops-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write(&dir, "Stary/Stary.md", "Opis Folderu");
        write(&dir, "Stary/A.md", "a");
        write(&dir, "Stary/Pod/B.md", "b");
        write(&dir, "Inne/B.md", "inne b");
        write(&dir, "Linki.md", "[[Stary]] [[Stary/Pod/B|bee]] [[A]] [[Stary/A#x]]");
        write(&dir, ".notes-workbench/kolejnosc-folderow.json", r#"{"Stary": 1, "Stary/Pod": 2, "Inne": 3}"#);
        let idx = Index::build(&dir);

        let (p, fixed) = rename_folder(&dir, &idx, "Stary", "Nowy: folder").unwrap();
        assert_eq!(p, "Nowy_ folder");
        assert_eq!(fixed, 1);
        assert!(dir.join("Nowy_ folder/Nowy_ folder.md").is_file());
        assert!(dir.join("Nowy_ folder/Pod/B.md").is_file());
        let l = fs::read_to_string(dir.join("Linki.md")).unwrap();
        assert_eq!(l, "[[Nowy_ folder]] [[Nowy_ folder/Pod/B|bee]] [[A]] [[Nowy_ folder/A#x]]");
        let order = fs::read_to_string(dir.join(".notes-workbench/kolejnosc-folderow.json")).unwrap();
        assert!(order.contains("\"Nowy_ folder/Pod\": 2") && !order.contains("Stary"), "{order}");

        let idx = Index::build(&dir);
        let (np, _) = move_note(&dir, &idx, "Nowy_ folder/A.md", "Inne").unwrap();
        assert_eq!(np, "Inne/A.md");
        assert_eq!(fs::read_to_string(dir.join("Linki.md")).unwrap(), "[[Nowy_ folder]] [[Nowy_ folder/Pod/B|bee]] [[A]] [[Inne/A#x]]");

        let f = create_folder(&dir, "Inne", "Nowy").unwrap();
        assert_eq!(f, "Inne/Nowy");
        let t = trash_folder(&dir, "Nowy_ folder").unwrap();
        assert_eq!(t, ".kosz/Nowy_ folder");
        assert!(dir.join(".kosz/Nowy_ folder/Pod/B.md").is_file());
        assert_eq!(restore_folder(&dir, &t).unwrap(), "Nowy_ folder");

        let fnote = set_folder_noai(&dir, "Inne", true).unwrap();
        assert_eq!(fnote, "Inne/Inne.md");
        assert!(Index::build(&dir).is_noai("Inne/B.md"));
        assert!(check_folder(".kosz").is_err() && check_folder("X/Y.assets").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
