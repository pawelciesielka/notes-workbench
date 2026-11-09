//! Zapis bloków jako Markdown (CommonMark + GFM) ze wstawkami HTML tam, gdzie Markdown
//! nie wyraża formatowania (ADR 0001). Pojedyncze przejście linii to twarde złamanie
//! (renderer z remark-breaks, Obsidian domyślnie).

use crate::model::{Block, Cell, Inline, Line, Style, Table};
use std::collections::HashMap;

/// Rozwiązuje odnośniki i obrazy na adresy w Skarbcu.
pub trait Resolver {
    /// Adres obrazu (już zakodowany) i jego rzeczywisty rozmiar w pikselach.
    fn image(&mut self, src: &str) -> (String, Option<(u32, u32)>);
    /// Adres odnośnika albo `None`, gdy odnośnik ma zniknąć (zostaje sam tekst).
    fn link(&mut self, href: &str) -> Option<String>;
}

pub fn blocks_to_md(blocks: &[Block], r: &mut dyn Resolver) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut in_task_list = false;
    // Po tabeli, bloku kodu i wyrównanym fragmencie następna treść musi zacząć się po pustej linii.
    let mut after_block = false;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Table(t) => {
                ensure_blank(&mut out);
                if t.is_simple() {
                    out.extend(gfm_table(t, r));
                } else {
                    out.push(html_table(t, r));
                }
                after_block = true;
                in_task_list = false;
                i += 1;
            }
            Block::Line(l) if l.is_code() && l.align.is_none() => {
                let mut j = i;
                let mut lines: Vec<&Line> = Vec::new();
                while let Some(Block::Line(m)) = blocks.get(j) {
                    if m.align.is_some() {
                        break;
                    }
                    if m.is_code() || m.is_blank() {
                        lines.push(m);
                        j += 1;
                    } else {
                        break;
                    }
                }
                while lines.last().is_some_and(|m| m.is_blank()) {
                    lines.pop();
                    j -= 1;
                }
                ensure_blank(&mut out);
                out.push(code_block(&lines));
                after_block = true;
                in_task_list = false;
                i = j;
            }
            Block::Line(l) if l.align.is_some() => {
                let align = l.align.clone().unwrap();
                let mut j = i;
                let mut inner = Vec::new();
                while let Some(Block::Line(m)) = blocks.get(j) {
                    if m.align.as_deref() != Some(align.as_str()) {
                        break;
                    }
                    inner.push(if m.is_blank() { String::new() } else { inline_md(&m.items, r, true, false) });
                    j += 1;
                }
                ensure_blank(&mut out);
                out.push(format!("<div align=\"{}\">", html_attr(&align)));
                out.push(String::new());
                out.extend(inner);
                out.push(String::new());
                out.push("</div>".into());
                after_block = true;
                in_task_list = false;
                i = j;
            }
            Block::Line(l) => {
                if l.is_blank() {
                    out.push(String::new());
                    in_task_list = false;
                    after_block = false;
                } else if let Some((checked, rest)) = task(l) {
                    if after_block {
                        ensure_blank(&mut out);
                        after_block = false;
                    }
                    let body = inline_md(rest, r, false, false);
                    let body = body.trim_start();
                    out.push(format!("- [{}] {}", if checked { "x" } else { " " }, body));
                    in_task_list = true;
                } else {
                    if in_task_list || after_block {
                        // Bez pustej linii zwykła linia stałaby się kontynuacją punktu listy
                        // albo częścią tabeli lub bloku HTML.
                        ensure_blank(&mut out);
                        in_task_list = false;
                        after_block = false;
                    }
                    out.push(inline_md(&l.items, r, true, false));
                }
                i += 1;
            }
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn ensure_blank(out: &mut Vec<String>) {
    if out.last().is_some_and(|l| !l.is_empty()) {
        out.push(String::new());
    }
}

/// Linia zaczynająca się od pola wyboru staje się punktem listy zadań.
fn task(l: &Line) -> Option<(bool, &[Inline])> {
    let mut idx = 0;
    while let Some(Inline::Text(t, _)) = l.items.get(idx) {
        if !t.trim().is_empty() {
            return None;
        }
        idx += 1;
    }
    match l.items.get(idx) {
        Some(Inline::Checkbox(c)) => {
            let mut rest = &l.items[idx + 1..];
            // Odstęp po polu wyboru dodaje już znacznik listy.
            if let Some(Inline::Text(t, _)) = rest.first() {
                if t.trim().is_empty() {
                    rest = &rest[1..];
                }
            }
            Some((*c, rest))
        }
        _ => None,
    }
}

fn code_block(lines: &[&Line]) -> String {
    let texts: Vec<String> = lines.iter().map(|l| l.raw_text()).collect();
    let longest = texts.iter().map(|t| longest_run(t, '`')).max().unwrap_or(0);
    let fence = "`".repeat((longest + 1).max(3));
    let mut s = String::new();
    s.push_str(&fence);
    s.push('\n');
    for t in &texts {
        s.push_str(t.trim_end_matches(['\r', '\n']));
        s.push('\n');
    }
    s.push_str(&fence);
    s
}

fn longest_run(s: &str, ch: char) -> usize {
    let (mut best, mut cur) = (0, 0);
    for c in s.chars() {
        if c == ch {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

fn code_span(text: &str, in_table: bool) -> String {
    let n = longest_run(text, '`') + 1;
    let fence = "`".repeat(n);
    let pad = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' '));
    let body = if in_table { text.replace('|', "\\|") } else { text.to_string() };
    if pad {
        format!("{fence} {body} {fence}")
    } else {
        format!("{fence}{body}{fence}")
    }
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace())
}

/// Escapuje tekst tak, by Markdown (i Obsidian) pokazał go dosłownie.
fn escape(text: &str, line_start: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut s = String::with_capacity(text.len() + 8);
    let first_non_ws = chars.iter().position(|c| !c.is_whitespace());
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        let at_start = line_start && Some(i) == first_non_ws;
        let esc = match c {
            '\\' | '`' | '*' | '[' | ']' | '<' | '#' | '$' | '^' | '~' | '|' => true,
            '_' => !(prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric)),
            '&' => {
                // Tylko gdy wygląda na encję HTML.
                let rest: String = chars[i + 1..].iter().take(12).collect();
                rest.find(';').is_some_and(|p| {
                    p > 0 && rest[..p].chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
                })
            }
            '=' => next == Some('=') || at_start,
            '%' => next == Some('%'),
            '>' | '+' | '-' => at_start,
            '.' | ')' => {
                line_start
                    && first_non_ws.is_some_and(|f| f < i && chars[f..i].iter().all(char::is_ascii_digit))
            }
            '!' => next == Some('['),
            _ => false,
        };
        if esc {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

/// Tekst wiodących odstępów jako encje, żeby wcięcia nie zamieniały się w blok kodu.
fn nbsp(ws: &str) -> String {
    ws.chars()
        .map(|c| match c {
            '\t' => "&nbsp;&nbsp;&nbsp;&nbsp;",
            _ => "&nbsp;",
        })
        .collect()
}

fn html_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
}

fn html_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn span_style(st: &Style) -> Option<String> {
    let mut css = Vec::new();
    if let Some(c) = &st.color {
        css.push(format!("color:{c}"));
    }
    if let Some(b) = &st.bg {
        css.push(format!("background:{b}"));
    }
    (!css.is_empty()).then(|| css.join(";"))
}

fn image_md(src: &str, w: Option<u32>, h: Option<u32>, r: &mut dyn Resolver, html: bool) -> String {
    let (dest, natural) = r.image(src);
    let same = match (natural, w, h) {
        (Some((nw, nh)), Some(w), Some(h)) => nw == w && nh == h,
        (_, None, None) => true,
        _ => false,
    };
    if same && !html {
        return format!("![]({dest})");
    }
    let mut attrs = String::new();
    if let Some(w) = w {
        attrs.push_str(&format!(" width=\"{w}\""));
        // Wysokość tylko gdy proporcje są inne niż w pliku.
        if let (Some(h), Some((nw, nh))) = (h, natural) {
            let a = w as f64 / h.max(1) as f64;
            let b = nw as f64 / nh.max(1) as f64;
            if (a - b).abs() / b > 0.02 {
                attrs.push_str(&format!(" height=\"{h}\""));
            }
        }
    }
    format!("<img src=\"{}\"{attrs}>", html_attr(&dest))
}

fn tok_link(i: &Inline) -> Option<&(String, usize)> {
    match i {
        Inline::Text(_, s) => s.link.as_ref(),
        Inline::Image { link, .. } => link.as_ref(),
        Inline::Checkbox(_) => None,
    }
}

/// Ustala adresy odnośników raz na odnośnik; odnośniki, które mają zniknąć, zdejmuje
/// z fragmentów i skleja sąsiednie fragmenty o tym samym stylu.
fn resolve_links(items: &[Inline], r: &mut dyn Resolver) -> (Vec<Inline>, HashMap<usize, String>) {
    let mut cache: HashMap<usize, Option<String>> = HashMap::new();
    let mut out: Vec<Inline> = Vec::with_capacity(items.len());
    for item in items {
        let mut item = item.clone();
        let link = match &mut item {
            Inline::Text(_, s) => &mut s.link,
            Inline::Image { link, .. } => link,
            Inline::Checkbox(_) => {
                out.push(item);
                continue;
            }
        };
        if let Some((href, id)) = link.clone() {
            let dest = cache.entry(id).or_insert_with(|| r.link(&href));
            if dest.is_none() {
                *link = None;
            }
        }
        if let (Some(Inline::Text(pt, ps)), Inline::Text(t, s)) = (out.last_mut(), &item) {
            if ps == s {
                pt.push_str(t);
                continue;
            }
        }
        out.push(item);
    }
    let dests = cache.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect();
    (out, dests)
}

/// Zapisuje linię jako Markdown.
pub fn inline_md(items: &[Inline], r: &mut dyn Resolver, line_start: bool, in_table: bool) -> String {
    let (items, dests) = resolve_links(items, r);
    let mut out = String::new();
    let mut i = 0;
    let mut first_text = line_start;
    while i < items.len() {
        let link = tok_link(&items[i]).cloned();
        let mut j = i;
        while j < items.len() && tok_link(&items[j]).cloned() == link {
            j += 1;
        }
        let mut content = String::new();
        for k in i..j {
            let prev_raw = if k > i { raw_last(&items[k - 1]) } else { None };
            let next_raw = if k + 1 < j { raw_first(&items[k + 1]) } else { None };
            let prev_emph = k > i && emph(&items[k - 1]);
            let next_emph = k + 1 < j && emph(&items[k + 1]);
            let at_line_start = out.is_empty() && content.is_empty() && first_text;
            content.push_str(&render_tok(
                &items[k],
                r,
                at_line_start,
                in_table,
                prev_raw,
                next_raw,
                prev_emph,
                next_emph,
            ));
            if matches!(&items[k], Inline::Text(t, _) if !t.trim().is_empty()) {
                first_text = false;
            }
        }
        match link.and_then(|(_, id)| dests.get(&id).cloned()) {
            Some(dest) if !content.trim().is_empty() => out.push_str(&format!("[{content}]({dest})")),
            _ => out.push_str(&content),
        }
        i = j;
    }
    out
}

fn raw_last(i: &Inline) -> Option<char> {
    match i {
        Inline::Text(t, _) => t.chars().last(),
        _ => Some('!'),
    }
}
fn raw_first(i: &Inline) -> Option<char> {
    match i {
        Inline::Text(t, _) => t.chars().next(),
        _ => Some('!'),
    }
}
fn emph(i: &Inline) -> bool {
    matches!(i, Inline::Text(t, s) if s.emphasis() && !t.trim().is_empty())
}

#[allow(clippy::too_many_arguments)]
fn render_tok(
    item: &Inline,
    r: &mut dyn Resolver,
    line_start: bool,
    in_table: bool,
    prev: Option<char>,
    next: Option<char>,
    prev_emph: bool,
    next_emph: bool,
) -> String {
    match item {
        Inline::Checkbox(c) => (if *c { "☑" } else { "☐" }).to_string(),
        Inline::Image { src, width, height, .. } => image_md(src, *width, *height, r, false),
        Inline::Text(text, st) => {
            let core_start = text.len() - text.trim_start().len();
            let core_end = text.trim_end().len();
            if core_end <= core_start {
                // Same odstępy.
                return if line_start { nbsp(text) } else { text.replace('\t', "    ") };
            }
            let lead = &text[..core_start];
            let core = &text[core_start..core_end];
            let trail = &text[core_end..];
            let mut s = if st.mono {
                code_span(core, in_table)
            } else {
                escape(core, line_start && lead.is_empty())
            };
            // Ograniczniki ** / * / ~~ tylko tam, gdzie CommonMark na pewno je rozpozna
            // (reguły flankowania); w pozostałych miejscach znaczniki HTML.
            let free = |c: Option<char>| c.is_none_or(|c| c.is_whitespace() || is_punct(c));
            let first = s.chars().next().unwrap();
            let last = s.chars().last().unwrap();
            let prev_eff = if lead.is_empty() { prev } else { Some(' ') };
            let next_eff = if trail.is_empty() { next } else { Some(' ') };
            let left_ok = !is_punct(first) || free(prev_eff);
            let right_ok = !is_punct(last) || free(next_eff);
            let md_ok = left_ok
                && right_ok
                && !(prev_emph && lead.is_empty())
                && !(next_emph && trail.is_empty())
                && !prev_eff.is_some_and(|c| c == '*' || c == '~' || c == '_' || c == '\\')
                && !next_eff.is_some_and(|c| c == '*' || c == '~' || c == '_');
            if st.strike {
                s = if md_ok { format!("~~{s}~~") } else { format!("<s>{s}</s>") };
            }
            match (st.bold, st.italic, md_ok) {
                (true, true, true) => s = format!("***{s}***"),
                (true, false, true) => s = format!("**{s}**"),
                (false, true, true) => s = format!("*{s}*"),
                (true, true, false) => s = format!("<b><i>{s}</i></b>"),
                (true, false, false) => s = format!("<b>{s}</b>"),
                (false, true, false) => s = format!("<i>{s}</i>"),
                _ => {}
            }
            if st.underline {
                s = format!("<u>{s}</u>");
            }
            if let Some(css) = span_style(st) {
                s = format!("<span style=\"{css}\">{s}</span>");
            }
            let lead_s = if line_start { nbsp(lead) } else { lead.replace('\t', "    ") };
            format!("{lead_s}{s}{}", trail.replace('\t', "    "))
        }
    }
}

// ---------- tabele ----------

fn cell_md(c: &Cell, r: &mut dyn Resolver) -> String {
    let mut parts = Vec::new();
    for b in &c.blocks {
        if let Block::Line(l) = b {
            if l.is_code() {
                parts.push(code_span(&l.raw_text(), true));
            } else if l.is_blank() {
                parts.push(String::new());
            } else {
                parts.push(inline_md(&l.items, r, false, true));
            }
        }
    }
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts.join("<br>")
}

fn gfm_table(t: &Table, r: &mut dyn Resolver) -> Vec<String> {
    let cols = t.rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let mut out = Vec::new();
    for (n, row) in t.rows.iter().enumerate() {
        let mut cells: Vec<String> = row.iter().map(|c| cell_md(c, r)).collect();
        cells.resize(cols, String::new());
        out.push(format!("| {} |", cells.join(" | ")));
        if n == 0 {
            out.push(format!("|{}", " --- |".repeat(cols)));
        }
    }
    if t.rows.is_empty() {
        out.push(format!("| {} |", " ".repeat(1)));
        out.push("| --- |".into());
    }
    out
}

fn html_table(t: &Table, r: &mut dyn Resolver) -> String {
    let mut s = String::from("<table>");
    for row in &t.rows {
        s.push_str("\n<tr>");
        for c in row {
            s.push_str("<td");
            if c.colspan > 1 {
                s.push_str(&format!(" colspan=\"{}\"", c.colspan));
            }
            if c.rowspan > 1 {
                s.push_str(&format!(" rowspan=\"{}\"", c.rowspan));
            }
            if let Some(bg) = &c.bg {
                s.push_str(&format!(" style=\"background:{}\"", html_attr(bg)));
            }
            s.push('>');
            s.push_str(&blocks_html(&c.blocks, r));
            s.push_str("</td>");
        }
        s.push_str("</tr>");
    }
    s.push_str("\n</table>");
    s
}

fn blocks_html(blocks: &[Block], r: &mut dyn Resolver) -> String {
    let mut parts: Vec<String> = Vec::new();
    for b in blocks {
        match b {
            Block::Table(t) => parts.push(html_table(t, r).replace('\n', "")),
            Block::Line(l) => {
                let mut h = inline_html(&l.items, r);
                if let Some(a) = &l.align {
                    h = format!("<div align=\"{}\">{h}</div>", html_attr(a));
                }
                parts.push(h);
            }
        }
    }
    parts.join("<br>")
}

/// Linia jako HTML (do tabel HTML, w których Markdown nie działa).
pub fn inline_html(items: &[Inline], r: &mut dyn Resolver) -> String {
    let (items, dests) = resolve_links(items, r);
    let items = &items[..];
    let mut out = String::new();
    let mut i = 0;
    while i < items.len() {
        let link = tok_link(&items[i]).cloned();
        let mut j = i;
        while j < items.len() && tok_link(&items[j]).cloned() == link {
            j += 1;
        }
        let mut content = String::new();
        for item in &items[i..j] {
            match item {
                Inline::Checkbox(c) => content.push(if *c { '☑' } else { '☐' }),
                Inline::Image { src, width, height, .. } => content.push_str(&image_md(src, *width, *height, r, true)),
                Inline::Text(t, st) => {
                    let mut s = html_text(t).replace("  ", " &nbsp;");
                    if st.mono {
                        s = format!("<code>{s}</code>");
                    }
                    if st.strike {
                        s = format!("<s>{s}</s>");
                    }
                    if st.italic {
                        s = format!("<i>{s}</i>");
                    }
                    if st.bold {
                        s = format!("<b>{s}</b>");
                    }
                    if st.underline {
                        s = format!("<u>{s}</u>");
                    }
                    if let Some(css) = span_style(st) {
                        s = format!("<span style=\"{css}\">{s}</span>");
                    }
                    content.push_str(&s);
                }
            }
        }
        match link.and_then(|(_, id)| dests.get(&id).cloned()) {
            Some(dest) => out.push_str(&format!("<a href=\"{}\">{content}</a>", html_attr(&dest))),
            None => out.push_str(&content),
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model, verify};

    struct NoLinks;
    impl Resolver for NoLinks {
        fn image(&mut self, src: &str) -> (String, Option<(u32, u32)>) {
            (src.to_string(), None)
        }
        fn link(&mut self, href: &str) -> Option<String> {
            (!href.starts_with("call:")).then(|| href.to_string())
        }
    }

    const S: &str = "<span style=\"font-family:'Segoe UI';font-size:12pt;\">";
    const C: &str = "<span style=\"font-family:'Courier New';font-size:12pt;\">";

    fn convert(html: &str) -> String {
        let md = blocks_to_md(&model::parse_body(html), &mut NoLinks);
        let e = verify::html_extract(html, true);
        let a = verify::md_extract(&md);
        assert_eq!(verify::first_difference(&e.text, &a.text), None, "tekst się różni:\n{md}");
        md
    }

    #[test]
    fn escapes_markdown_syntax() {
        let md = convert(&format!("{S}- punkt #tag *gwiazdka* [x] 1. a_b _c_</span><br />{S}1. lista</span><br />"));
        assert!(md.contains(r"\- punkt \#tag \*gwiazdka\* \[x\] 1. a_b \_c\_"), "{md}");
        assert!(md.contains(r"1\. lista"), "{md}");
    }

    #[test]
    fn bold_and_colors() {
        let md = convert(&format!("{S}<b>Introduction:</b></span><br />{S}a</span><span style=\"color:#F02828;\"><b>czerwone</b></span><br />"));
        assert!(md.contains("**Introduction:**"), "{md}");
        assert!(md.contains("<span style=\"color:#F02828\">"), "{md}");
    }

    #[test]
    fn code_lines_make_one_block() {
        let md = convert(&format!("{C}a = 1</span><br />{S}</span><br />{C}    b = 2</span><br />{S}koniec</span><br />"));
        assert!(md.contains("```\na = 1\n\n    b = 2\n```"), "{md}");
    }

    #[test]
    fn checkboxes_and_dropped_links() {
        let md = convert(&format!(
            "<input type=\"checkbox\" checked>{S} zrobione</span><br />{S}tel </span><a href=\"call:123\">{C}123</span></a>{C}-4</span><br />"
        ));
        assert!(md.contains("- [x] zrobione"), "{md}");
        assert!(md.contains("`123-4`"), "{md}");
    }

    #[test]
    fn complex_table_is_html() {
        let md = convert(&format!(
            "<table><tr><td colspan=2>{S}<b>A</b></span><br /></td></tr><tr><td>{S}x &lt; y</span><br /></td><td>{S}z</span><br /></table>"
        ));
        assert!(md.contains("<td colspan=\"2\"><b>A</b></td>"), "{md}");
        assert!(md.contains("x &lt; y"), "{md}");
    }
}
