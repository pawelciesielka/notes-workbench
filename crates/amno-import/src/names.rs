//! Nazwy plików i katalogów w Skarbcu.

use std::collections::HashSet;

/// Nazwa pliku z tytułu: bez znaków niedozwolonych w Windows i w linkach Obsidiana.
pub fn sanitize(title: &str) -> String {
    let mapped: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '/' | '\\' | '|' | '?' | '*' | '#' | '^' | '[' | ']' => '_',
            '"' => '\'',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut s = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 80 {
        s = s.chars().take(80).collect();
    }
    let mut s = s.trim().trim_end_matches(['.', ' ']).to_string();
    if s.starts_with('.') {
        s = format!("_{}", &s[1..]);
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        s.push('_');
    }
    if s.is_empty() {
        s = "Bez tytułu".into();
    }
    s
}

/// Rezerwuje unikalną nazwę w katalogu (bez względu na wielkość liter, jak w Windows).
/// `exts` to rozszerzenia, które element zajmie (np. `[".md", ".assets"]` dla Notatki, `[""]` dla Folderu).
pub fn unique(base: &str, exts: &[&str], used: &mut HashSet<String>) -> String {
    let mut n = 1;
    loop {
        let cand = if n == 1 { base.to_string() } else { format!("{base} ({n})") };
        if exts.iter().all(|e| !used.contains(&format!("{cand}{e}").to_lowercase())) {
            for e in exts {
                used.insert(format!("{cand}{e}").to_lowercase());
            }
            return cand;
        }
        n += 1;
    }
}

/// Koduje ścieżkę do użycia jako adres odnośnika w Markdownie.
pub fn enc_url_path(p: &str) -> String {
    let mut s = String::with_capacity(p.len());
    for c in p.chars() {
        match c {
            ' ' => s.push_str("%20"),
            '%' => s.push_str("%25"),
            '(' => s.push_str("%28"),
            ')' => s.push_str("%29"),
            '<' => s.push_str("%3C"),
            '>' => s.push_str("%3E"),
            '#' => s.push_str("%23"),
            '[' => s.push_str("%5B"),
            ']' => s.push_str("%5D"),
            '^' => s.push_str("%5E"),
            '`' => s.push_str("%60"),
            '{' => s.push_str("%7B"),
            '}' => s.push_str("%7D"),
            '|' => s.push_str("%7C"),
            '"' => s.push_str("%22"),
            '\\' => s.push_str("%5C"),
            c => s.push(c),
        }
    }
    s
}

/// Koduje adres zewnętrzny tylko tam, gdzie Markdown by go przerwał.
pub fn enc_url(u: &str) -> String {
    let mut s = String::with_capacity(u.len());
    for c in u.chars() {
        match c {
            ' ' => s.push_str("%20"),
            '(' => s.push_str("%28"),
            ')' => s.push_str("%29"),
            '<' => s.push_str("%3C"),
            '>' => s.push_str("%3E"),
            '\\' => s.push_str("%5C"),
            c if c.is_control() => {}
            c => s.push(c),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_titles() {
        assert_eq!(sanitize("\"role:medical doctor, ...\""), "'role_medical doctor, ...'");
        assert_eq!(sanitize("Carelink "), "Carelink");
        assert_eq!(sanitize("CON"), "CON_");
        assert_eq!(sanitize(""), "Bez tytułu");
        assert_eq!(sanitize(".env"), "_env");
    }

    #[test]
    fn unique_names() {
        let mut used = HashSet::new();
        assert_eq!(unique("Minecraft", &[".md", ".assets"], &mut used), "Minecraft");
        assert_eq!(unique("minecraft", &[".md", ".assets"], &mut used), "minecraft (2)");
        assert_eq!(unique("Minecraft", &[""], &mut used), "Minecraft");
    }
}
