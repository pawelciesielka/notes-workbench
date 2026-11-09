//! Treść Notatki z HTML AMNO jako ciąg linii ze sformatowanymi fragmentami i tabel.

use ego_tree::NodeRef;
use scraper::{Html, Node};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub mono: bool,
    pub color: Option<String>,
    pub bg: Option<String>,
    /// Odnośnik: (href, identyfikator węzła `<a>`), żeby sąsiednie, osobne linki się nie zlewały.
    pub link: Option<(String, usize)>,
}

impl Style {
    pub fn emphasis(&self) -> bool {
        self.bold || self.italic || self.strike
    }
}

#[derive(Debug, Clone)]
pub enum Inline {
    Text(String, Style),
    Image {
        src: String,
        width: Option<u32>,
        height: Option<u32>,
        link: Option<(String, usize)>,
    },
    Checkbox(bool),
}

#[derive(Debug, Clone, Default)]
pub struct Line {
    pub items: Vec<Inline>,
    pub align: Option<String>,
    /// Linia pusta, ale zapisana fontem o stałej szerokości (pusta linia w kodzie).
    pub mono_hint: bool,
}

impl Line {
    pub fn is_blank(&self) -> bool {
        self.items.iter().all(|i| matches!(i, Inline::Text(t, _) if t.trim().is_empty()))
    }

    /// Linia kodu: cała tekstowa treść zapisana fontem o stałej szerokości.
    pub fn is_code(&self) -> bool {
        let mut has_text = false;
        for item in &self.items {
            match item {
                Inline::Text(t, s) => {
                    if !s.mono {
                        if !t.trim().is_empty() {
                            return false;
                        }
                    } else if !t.trim().is_empty() {
                        has_text = true;
                    }
                    if s.link.is_some() && !t.trim().is_empty() {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        has_text
    }

    pub fn raw_text(&self) -> String {
        self.items
            .iter()
            .filter_map(|i| match i {
                Inline::Text(t, _) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum Block {
    Line(Line),
    Table(Table),
}

#[derive(Debug, Clone, Default)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub blocks: Vec<Block>,
    pub colspan: u32,
    pub rowspan: u32,
    pub bg: Option<String>,
}

impl Table {
    /// Czy tabelę da się zapisać jako tabelę GFM bez utraty informacji.
    pub fn is_simple(&self) -> bool {
        self.rows.iter().flatten().all(|c| {
            c.colspan <= 1
                && c.rowspan <= 1
                && c.bg.is_none()
                && c.blocks.iter().all(|b| matches!(b, Block::Line(l) if l.align.is_none()))
        })
    }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    cur: Line,
    align: Option<String>,
    link_seq: usize,
}

impl Builder {
    fn flush_line(&mut self) {
        let mut line = std::mem::take(&mut self.cur);
        line.align = self.align.clone();
        self.blocks.push(Block::Line(line));
    }
    fn flush_if_open(&mut self) {
        if !self.cur.items.is_empty() || self.cur.mono_hint {
            self.flush_line();
        }
    }
    fn push_text(&mut self, text: &str, style: &Style) {
        if style.mono {
            self.cur.mono_hint = true;
        }
        if text.is_empty() {
            return;
        }
        if let Some(Inline::Text(prev, ps)) = self.cur.items.last_mut() {
            if ps == style {
                prev.push_str(text);
                return;
            }
        }
        self.cur.items.push(Inline::Text(text.to_string(), style.clone()));
    }
}

const MONO_FONTS: [&str; 8] = [
    "courier", "consolas", "monaco", "menlo", "lucida console", "monospace", "cascadia", "source code",
];

fn is_white(c: &str) -> bool {
    matches!(c.to_ascii_lowercase().as_str(), "#ffffff" | "#fffeff" | "white" | "transparent")
}

fn is_black(c: &str) -> bool {
    matches!(c.to_ascii_lowercase().as_str(), "#000000" | "black" | "windowtext")
}

fn css_props(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|p| {
            let (k, v) = p.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect()
}

fn apply_css(style: &mut Style, css: &str) {
    for (k, v) in css_props(css) {
        match k.as_str() {
            "font-family" => {
                let fam = v.to_ascii_lowercase();
                if fam != "inherit" {
                    style.mono = MONO_FONTS.iter().any(|m| fam.contains(m));
                }
            }
            "color" => style.color = if is_black(&v) || v == "inherit" { None } else { Some(v) },
            "background" | "background-color" => {
                style.bg = if is_white(&v) || v == "inherit" { None } else { Some(v) }
            }
            "font-weight" => style.bold = v == "bold" || v.parse::<u32>().is_ok_and(|w| w >= 600),
            "font-style" => style.italic = v == "italic",
            "text-decoration" => {
                if v.contains("underline") {
                    style.underline = true;
                }
                if v.contains("line-through") {
                    style.strike = true;
                }
            }
            _ => {}
        }
    }
}

fn align_of(el: &scraper::node::Element) -> Option<String> {
    if let Some(a) = el.attr("align") {
        return Some(a.to_ascii_lowercase());
    }
    el.attr("style").and_then(|s| {
        css_props(s).into_iter().find(|(k, _)| k == "text-align").map(|(_, v)| v.to_ascii_lowercase())
    })
    .filter(|a| a != "left" && a != "justify")
}

const INLINE_TAGS: [&str; 12] = ["span", "font", "b", "strong", "i", "em", "u", "s", "strike", "del", "a", "sup"];

fn walk(node: NodeRef<Node>, style: &Style, b: &mut Builder) {
    match node.value() {
        Node::Text(t) => {
            let s: &str = t;
            let parent_inline = node
                .parent()
                .and_then(|p| p.value().as_element().map(|e| INLINE_TAGS.contains(&e.name())))
                .unwrap_or(false);
            if s.chars().all(char::is_whitespace) && (s.contains('\n') || s.contains('\r') || !parent_inline) {
                return;
            }
            let s = s.replace("\r\n", " ").replace(['\n', '\r'], " ");
            b.push_text(&s, style);
        }
        Node::Element(el) => {
            let name = el.name();
            let mut st = style.clone();
            match name {
                "script" | "style" | "head" | "title" => return,
                "br" => {
                    b.flush_line();
                    return;
                }
                "img" => {
                    let dim = |a: &str| el.attr(a).and_then(|v| v.trim().parse::<u32>().ok());
                    b.cur.items.push(Inline::Image {
                        src: el.attr("src").unwrap_or_default().to_string(),
                        width: dim("width"),
                        height: dim("height"),
                        link: style.link.clone(),
                    });
                    return;
                }
                "input" => {
                    if el.attr("type").is_none_or(|t| t.eq_ignore_ascii_case("checkbox")) {
                        b.cur.items.push(Inline::Checkbox(el.attr("checked").is_some()));
                    }
                    return;
                }
                "table" => {
                    b.flush_if_open();
                    let t = parse_table(node, style, &mut b.link_seq);
                    b.blocks.push(Block::Table(t));
                    return;
                }
                "div" | "p" | "center" => {
                    b.flush_if_open();
                    let prev = b.align.clone();
                    let a = if name == "center" { Some("center".to_string()) } else { align_of(el) };
                    if a.is_some() {
                        b.align = a;
                    }
                    if let Some(css) = el.attr("style") {
                        apply_css(&mut st, css);
                    }
                    for c in node.children() {
                        walk(c, &st, b);
                    }
                    b.flush_if_open();
                    b.align = prev;
                    return;
                }
                "b" | "strong" => st.bold = true,
                "i" | "em" => st.italic = true,
                "u" => st.underline = true,
                "s" | "strike" | "del" => st.strike = true,
                "a" => {
                    if let Some(h) = el.attr("href") {
                        b.link_seq += 1;
                        st.link = Some((h.to_string(), b.link_seq));
                    }
                }
                "font" => {
                    if let Some(c) = el.attr("color") {
                        st.color = if is_black(c) { None } else { Some(c.to_string()) };
                    }
                    if let Some(f) = el.attr("face") {
                        let f = f.to_ascii_lowercase();
                        st.mono = MONO_FONTS.iter().any(|m| f.contains(m));
                    }
                }
                _ => {}
            }
            if let Some(css) = el.attr("style") {
                apply_css(&mut st, css);
            }
            for c in node.children() {
                walk(c, &st, b);
            }
        }
        _ => {
            for c in node.children() {
                walk(c, style, b);
            }
        }
    }
}

fn parse_table(node: NodeRef<Node>, style: &Style, link_seq: &mut usize) -> Table {
    let mut table = Table::default();
    fn rows<'a>(n: NodeRef<'a, Node>, out: &mut Vec<NodeRef<'a, Node>>) {
        for c in n.children() {
            if let Some(el) = c.value().as_element() {
                match el.name() {
                    "tr" => out.push(c),
                    "tbody" | "thead" | "tfoot" => rows(c, out),
                    _ => {}
                }
            }
        }
    }
    let mut trs = Vec::new();
    rows(node, &mut trs);
    for tr in trs {
        let mut cells = Vec::new();
        for td in tr.children() {
            let Some(el) = td.value().as_element() else { continue };
            if el.name() != "td" && el.name() != "th" {
                continue;
            }
            let span = |a: &str| el.attr(a).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1);
            let mut bg = el.attr("bgcolor").map(str::to_string);
            if let Some(css) = el.attr("style") {
                for (k, v) in css_props(css) {
                    if k == "background" || k == "background-color" {
                        bg = Some(v);
                    }
                }
            }
            let bg = bg.filter(|c| !is_white(c));
            let mut st = style.clone();
            if el.name() == "th" {
                st.bold = true;
            }
            let mut b = Builder { link_seq: *link_seq, ..Default::default() };
            for c in td.children() {
                walk(c, &st, &mut b);
            }
            b.flush_if_open();
            *link_seq = b.link_seq;
            let mut blocks = b.blocks;
            while matches!(blocks.last(), Some(Block::Line(l)) if l.is_blank()) {
                blocks.pop();
            }
            cells.push(Cell { blocks, colspan: span("colspan"), rowspan: span("rowspan"), bg });
        }
        table.rows.push(cells);
    }
    table
}

/// Zamienia HTML treści Notatki na bloki.
pub fn parse_body(html: &str) -> Vec<Block> {
    let doc = Html::parse_fragment(html);
    let mut b = Builder::default();
    for c in doc.tree.root().children() {
        walk(c, &Style::default(), &mut b);
    }
    b.flush_if_open();
    let mut blocks = b.blocks;
    while matches!(blocks.last(), Some(Block::Line(l)) if l.is_blank()) {
        blocks.pop();
    }
    while matches!(blocks.first(), Some(Block::Line(l)) if l.is_blank()) {
        blocks.remove(0);
    }
    blocks
}
