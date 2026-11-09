//! Sprawdza, czy Markdown Notatki niesie ten sam tekst co jej HTML w Eksporcie AMNO.

use ego_tree::NodeRef;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use scraper::{Html, Node};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Extract {
    pub text: String,
    pub images: usize,
    pub links: usize,
}

fn has_content(node: NodeRef<Node>) -> bool {
    node.descendants().any(|d| match d.value() {
        Node::Text(t) => !t.trim().is_empty(),
        Node::Element(e) => e.name() == "img",
        _ => false,
    })
}

fn html_extract_into(node: NodeRef<Node>, out: &mut Extract, skip_call_links: bool) {
    match node.value() {
        Node::Text(t) => out.text.push_str(t),
        Node::Element(el) => {
            match el.name() {
                "script" | "style" | "head" | "title" => return,
                "img" => out.images += 1,
                "input" => out.text.push(if el.attr("checked").is_some() { '☑' } else { '☐' }),
                "a" => {
                    if let Some(h) = el.attr("href") {
                        let dropped = skip_call_links && h.to_ascii_lowercase().starts_with("call:");
                        // Odnośnik bez treści (same odstępy) nie jest widoczny i nie jest przenoszony.
                        if !dropped && has_content(node) {
                            out.links += 1;
                        }
                    }
                }
                "br" => out.text.push('\n'),
                _ => {}
            }
            for c in node.children() {
                html_extract_into(c, out, skip_call_links);
            }
        }
        _ => {
            for c in node.children() {
                html_extract_into(c, out, skip_call_links);
            }
        }
    }
}

/// Tekst, obrazy i odnośniki z HTML treści (z Eksportu albo z tabeli HTML w Markdownie).
pub fn html_extract(html: &str, skip_call_links: bool) -> Extract {
    let doc = Html::parse_fragment(html);
    let mut out = Extract::default();
    html_extract_into(doc.tree.root(), &mut out, skip_call_links);
    out
}

/// Tekst, obrazy i odnośniki z Markdownu, tak jak zobaczy je czytelnik.
pub fn md_extract(md: &str) -> Extract {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut out = Extract::default();
    let mut html_buf = String::new();
    let mut in_html_block = false;
    let mut inline_html = String::new();
    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::HtmlBlock) => {
                in_html_block = true;
                html_buf.clear();
            }
            Event::End(TagEnd::HtmlBlock) => {
                in_html_block = false;
                let e = html_extract(&html_buf, false);
                out.text.push_str(&e.text);
                out.images += e.images;
                out.links += e.links;
            }
            Event::Html(h) => {
                if in_html_block {
                    html_buf.push_str(&h);
                } else {
                    inline_html.push_str(&h);
                }
            }
            Event::InlineHtml(h) => inline_html.push_str(&h),
            Event::Text(t) | Event::Code(t) => {
                if in_html_block {
                    html_buf.push_str(&t);
                } else {
                    out.text.push_str(&t);
                }
            }
            Event::TaskListMarker(c) => out.text.push(if c { '☑' } else { '☐' }),
            Event::Start(Tag::Image { .. }) => out.images += 1,
            Event::Start(Tag::Link { .. }) => out.links += 1,
            Event::SoftBreak | Event::HardBreak => out.text.push('\n'),
            _ => {}
        }
    }
    // Znaczniki wstawek (np. <img>, <a>) wewnątrz akapitów.
    let lower = inline_html.to_ascii_lowercase();
    out.images += lower.matches("<img ").count();
    out.links += lower.matches("<a ").count();
    out
}

pub fn normalize(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '\u{200b}' && *c != '\u{feff}').collect()
}

/// Pierwsze miejsce różnicy (z kontekstem) albo `None`, gdy teksty są zgodne.
pub fn first_difference(expected: &str, actual: &str) -> Option<(String, String)> {
    let e: Vec<char> = normalize(expected).chars().collect();
    let a: Vec<char> = normalize(actual).chars().collect();
    if e == a {
        return None;
    }
    let pos = e.iter().zip(a.iter()).take_while(|(x, y)| x == y).count();
    let ctx = |v: &[char]| -> String {
        let start = pos.saturating_sub(20);
        let end = (pos + 40).min(v.len());
        v[start..end].iter().collect()
    };
    Some((ctx(&e), ctx(&a)))
}
