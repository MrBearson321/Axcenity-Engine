//! The documentation built into the editor: the Markdown pages under the
//! repository's `documentation/` folder (the same files the GitBook site
//! publishes), read into blocks the Learn and Documentation pages draw.
//!
//! Only a small part of Markdown is read, so the pages look the same in the
//! editor, on GitHub and on GitBook: `#` headings, paragraphs, `-` and `1.`
//! lists (one level), fenced code, `>` notes, pipe tables, `---` rules, and
//! inside text `**bold**`, `` `code` `` and `[links](target)`. Anything else
//! is listed in [`Page::problems`], and a test fails on it.
//!
//! `SUMMARY.md` (GitBook's table of contents) gives the order: each `## `
//! heading is a section, each `* [Title](path.md)` under it a page.

/// Where a feature stands, from a page's `status:` line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    /// Built, tested and on by default.
    #[default]
    Stable,
    /// Built, with a known part missing or off by default.
    Partial,
    /// Built, but opt-in, early or likely to change.
    Experimental,
}

impl Status {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "stable" => Some(Self::Stable),
            "partial" => Some(Self::Partial),
            "experimental" => Some(Self::Experimental),
            _ => None,
        }
    }

    /// The badge's words.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Partial => "Partly built",
            Self::Experimental => "Experimental",
        }
    }
}

/// A run of text inside a paragraph, list item, note or table cell.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Span {
    Text(String),
    Strong(String),
    Code(String),
    /// `[text](target)`; `code` when the text was in backticks.
    Link {
        text: String,
        target: String,
        code: bool,
    },
}

/// One block of a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Heading {
        level: u8,
        text: String,
        anchor: String,
    },
    Paragraph(Vec<Span>),
    List {
        ordered: bool,
        items: Vec<Vec<Span>>,
    },
    Code {
        lang: String,
        text: String,
    },
    /// A `>` quote, drawn as a callout.
    Note(Vec<Span>),
    Table {
        header: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
    Rule,
}

/// One Markdown file.
#[derive(Clone, Debug, Default)]
pub(crate) struct Page {
    /// Relative to `documentation/`, with `/`: `learn/building.md`.
    pub(crate) path: String,
    /// The `# ` heading.
    pub(crate) title: String,
    /// The `description:` line: one sentence for cards and search.
    pub(crate) description: String,
    pub(crate) status: Status,
    /// The `samples:` list: folders in `examples/` the page follows.
    pub(crate) samples: Vec<String>,
    /// The `icon:` line: a Phosphor icon name, such as `cube`.
    pub(crate) icon: String,
    pub(crate) blocks: Vec<Block>,
    /// Markdown the editor can't show, as `line: what`. Tests keep it empty.
    pub(crate) problems: Vec<String>,
}

/// A `## ` group of pages in `SUMMARY.md`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Section {
    pub(crate) title: String,
    /// Indexes into [`Library::pages`], in order.
    pub(crate) pages: Vec<usize>,
    /// Paths `SUMMARY.md` lists here that aren't built in (the generated
    /// API pages, which the editor shows as its API Reference instead).
    pub(crate) elsewhere: Vec<(String, String)>,
}

/// Every built-in page, and the order `SUMMARY.md` gives them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Library {
    pub(crate) pages: Vec<Page>,
    pub(crate) sections: Vec<Section>,
    /// `SUMMARY.md` links to pages that aren't built in, and pages it
    /// doesn't list. Tests keep it empty.
    pub(crate) problems: Vec<String>,
}

/// Where a link goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    /// A built-in page (by index), at a heading's anchor if one is given.
    Page(usize, Option<String>),
    /// A heading on the same page.
    Anchor(String),
    /// A sample project, `examples/<name>`.
    Sample(String),
    /// A page of the generated API reference, shown in the API Reference: the
    /// entry it names (`Part`, `Part.touched` for `api/classes/Part.md#touched`)
    /// or nothing for a list.
    Api(String),
    /// Anything on the web.
    Url(String),
    /// A relative link to nothing built in.
    Missing(String),
}

/// The folder the generated API reference is written to, under
/// `documentation/`. Its pages aren't built in: the editor's API Reference
/// page shows the same entries, searchable.
pub(crate) const API_FOLDER: &str = "api/";

impl Library {
    /// Reads `files`: `(path relative to documentation/, text)`. `SUMMARY.md`
    /// gives the sections; `README.md` files and pages are read as pages.
    pub(crate) fn load(files: &[(&str, &str)]) -> Self {
        let mut library = Library::default();
        for (path, text) in files {
            if *path == "SUMMARY.md" || path.starts_with(API_FOLDER) {
                continue;
            }
            library.pages.push(Page::parse(path, text));
        }
        library.pages.sort_by(|a, b| a.path.cmp(&b.path));
        let summary = files
            .iter()
            .find(|(path, _)| *path == "SUMMARY.md")
            .map_or("", |(_, text)| *text);
        library.read_summary(summary);
        library
    }

    fn read_summary(&mut self, summary: &str) {
        let mut listed = vec![false; self.pages.len()];
        let mut section = Section {
            title: String::new(),
            ..Default::default()
        };
        for line in summary.lines() {
            let trimmed = line.trim();
            if let Some(title) = trimmed.strip_prefix("## ") {
                if !section.pages.is_empty() || !section.elsewhere.is_empty() {
                    self.sections.push(std::mem::take(&mut section));
                }
                section.title = title.trim().to_owned();
                continue;
            }
            let Some(item) = trimmed
                .strip_prefix("* ")
                .or_else(|| trimmed.strip_prefix("- "))
            else {
                continue;
            };
            let spans = parse_inline(item);
            let Some(Span::Link { text, target, .. }) = spans.first() else {
                continue;
            };
            if target.starts_with(API_FOLDER) {
                // The generated pages under it are the API Reference's own
                // entries; only the top of the list gets a row.
                if !line.starts_with(char::is_whitespace) {
                    section.elsewhere.push((text.clone(), target.clone()));
                }
                continue;
            }
            match self.index_of(target) {
                Some(index) => {
                    listed[index] = true;
                    section.pages.push(index);
                }
                None => self
                    .problems
                    .push(format!("SUMMARY.md lists `{target}`, which isn't a page")),
            }
        }
        if !section.pages.is_empty() || !section.elsewhere.is_empty() {
            self.sections.push(section);
        }
        for (index, listed) in listed.into_iter().enumerate() {
            if !listed {
                self.problems.push(format!(
                    "`{}` isn't listed in SUMMARY.md",
                    self.pages[index].path
                ));
            }
        }
    }

    /// The page at `path` (relative to `documentation/`).
    pub(crate) fn index_of(&self, path: &str) -> Option<usize> {
        self.pages.iter().position(|page| page.path == path)
    }

    /// Where `target`, a link on the page `from`, goes.
    pub(crate) fn resolve(&self, from: &str, target: &str) -> LinkTarget {
        if target.starts_with("http://") || target.starts_with("https://") {
            if let Some(name) = sample_in_url(target) {
                return LinkTarget::Sample(name);
            }
            return LinkTarget::Url(target.to_owned());
        }
        if let Some(anchor) = target.strip_prefix('#') {
            return LinkTarget::Anchor(anchor.to_owned());
        }
        let (path, anchor) = match target.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor.to_owned())),
            None => (target, None),
        };
        let path = join(from, path);
        if let Some(page) = path.strip_prefix(API_FOLDER) {
            return LinkTarget::Api(api_entry(page, anchor));
        }
        match self.index_of(&path) {
            Some(index) => LinkTarget::Page(index, anchor),
            None => LinkTarget::Missing(path),
        }
    }

    /// The pages and headings that match `query`, best first: a match in a
    /// page's title, then in a heading, then in the text under it.
    pub(crate) fn search(&self, query: &str) -> Vec<Hit> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(str::to_lowercase)
            .filter(|word| !word.is_empty())
            .collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (index, page) in self.pages.iter().enumerate() {
            for part in page.parts() {
                let haystack = format!("{} {}", part.heading, part.text).to_lowercase();
                if !words.iter().all(|word| haystack.contains(word.as_str())) {
                    continue;
                }
                let title = page.title.to_lowercase();
                let heading = part.heading.to_lowercase();
                let mut score = 0;
                for word in &words {
                    if part.anchor.is_none() && title.contains(word.as_str()) {
                        score += 8;
                    }
                    if heading.contains(word.as_str()) {
                        score += 4;
                    }
                    score += 1;
                }
                hits.push(Hit {
                    page: index,
                    anchor: part.anchor,
                    heading: part.heading,
                    snippet: snippet(&part.text, &words[0]),
                    score,
                });
            }
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.page.cmp(&b.page)));
        hits
    }
}

/// A search result: a page, or a heading on it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hit {
    pub(crate) page: usize,
    /// `None` for the top of the page.
    pub(crate) anchor: Option<String>,
    /// The page's title, or the heading's text.
    pub(crate) heading: String,
    /// A line of the text around the first word searched for.
    pub(crate) snippet: String,
    score: u32,
}

/// A page's text split at its `##` headings, for search.
struct Part {
    anchor: Option<String>,
    heading: String,
    text: String,
}

impl Page {
    /// Reads one Markdown file.
    pub(crate) fn parse(path: &str, text: &str) -> Self {
        let mut page = Page {
            path: path.to_owned(),
            ..Default::default()
        };
        let mut lines: Vec<&str> = text.lines().collect();
        let mut first = 0;
        if lines.first().is_some_and(|line| line.trim() == "---") {
            match lines.iter().skip(1).position(|line| line.trim() == "---") {
                Some(end) => {
                    for line in &lines[1..=end] {
                        page.front_matter(line);
                    }
                    first = end + 2;
                }
                None => page.problems.push("1: front matter never ends".into()),
            }
        }
        lines.drain(..first.min(lines.len()));
        page.blocks = parse_blocks(&lines, first, &mut page.problems);
        if let Some(index) = page
            .blocks
            .iter()
            .position(|block| matches!(block, Block::Heading { level: 1, .. }))
        {
            if let Block::Heading { text, .. } = page.blocks.remove(index) {
                page.title = text;
            }
        } else {
            page.problems.push("the page has no `# ` title".into());
        }
        page
    }

    fn front_matter(&mut self, line: &str) {
        let Some((key, value)) = line.split_once(':') else {
            return;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "description" => self.description = value.to_owned(),
            "icon" => self.icon = value.to_owned(),
            "status" => match Status::parse(value) {
                Some(status) => self.status = status,
                None => self.problems.push(format!(
                    "status `{value}` isn't stable, partial or experimental"
                )),
            },
            "samples" => {
                self.samples = value
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            _ => {}
        }
    }

    /// The headings on the page (any level below the title), with anchors.
    pub(crate) fn headings(&self) -> impl Iterator<Item = (u8, &str, &str)> {
        self.blocks.iter().filter_map(|block| match block {
            Block::Heading {
                level,
                text,
                anchor,
            } => Some((*level, text.as_str(), anchor.as_str())),
            _ => None,
        })
    }

    /// The link targets on the page.
    #[cfg(test)]
    pub(crate) fn links(&self) -> Vec<&str> {
        let mut links = Vec::new();
        for spans in self.span_lists() {
            for span in spans {
                if let Span::Link { target, .. } = span {
                    links.push(target.as_str());
                }
            }
        }
        links
    }

    /// The text in backticks on the page, outside code blocks.
    pub(crate) fn code_spans(&self) -> Vec<&str> {
        let mut code = Vec::new();
        for spans in self.span_lists() {
            for span in spans {
                match span {
                    Span::Code(text) => code.push(text.as_str()),
                    Span::Link {
                        text, code: true, ..
                    } => code.push(text.as_str()),
                    _ => {}
                }
            }
        }
        code
    }

    /// Every run of spans on the page.
    fn span_lists(&self) -> Vec<&[Span]> {
        let mut lists: Vec<&[Span]> = Vec::new();
        for block in &self.blocks {
            match block {
                Block::Paragraph(spans) | Block::Note(spans) => lists.push(spans),
                Block::List { items, .. } => lists.extend(items.iter().map(Vec::as_slice)),
                Block::Table { header, rows } => {
                    lists.extend(header.iter().map(Vec::as_slice));
                    for row in rows {
                        lists.extend(row.iter().map(Vec::as_slice));
                    }
                }
                _ => {}
            }
        }
        lists
    }

    /// The page split at its `##` headings, as plain text.
    fn parts(&self) -> Vec<Part> {
        let mut parts = vec![Part {
            anchor: None,
            heading: self.title.clone(),
            text: self.description.clone(),
        }];
        for block in &self.blocks {
            match block {
                Block::Heading {
                    level: 2,
                    text,
                    anchor,
                } => parts.push(Part {
                    anchor: Some(anchor.clone()),
                    heading: text.clone(),
                    text: String::new(),
                }),
                other => {
                    let text = plain_text(other);
                    let part = parts.last_mut().expect("there's always a first part");
                    if !part.text.is_empty() {
                        part.text.push(' ');
                    }
                    part.text.push_str(&text);
                }
            }
        }
        parts
    }
}

/// `block` without its markup, for search.
fn plain_text(block: &Block) -> String {
    let spans_text = |spans: &[Span]| -> String {
        spans
            .iter()
            .map(|span| match span {
                Span::Text(text) | Span::Strong(text) | Span::Code(text) => text.as_str(),
                Span::Link { text, .. } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("")
    };
    match block {
        Block::Heading { text, .. } => text.clone(),
        Block::Paragraph(spans) | Block::Note(spans) => spans_text(spans),
        Block::List { items, .. } => items
            .iter()
            .map(|item| spans_text(item))
            .collect::<Vec<_>>()
            .join(" "),
        Block::Code { text, .. } => text.clone(),
        Block::Table { header, rows } => header
            .iter()
            .chain(rows.iter().flatten())
            .map(|cell| spans_text(cell))
            .collect::<Vec<_>>()
            .join(" "),
        Block::Rule => String::new(),
    }
}

/// About 120 characters of `text` around the first `word`, on one line.
fn snippet(text: &str, word: &str) -> String {
    const BEFORE: usize = 40;
    const LENGTH: usize = 120;
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII, so find the place
    // in characters.
    let at = lower
        .find(word)
        .map_or(0, |byte| lower[..byte].chars().count());
    let chars: Vec<char> = text.chars().collect();
    let start = at.saturating_sub(BEFORE);
    let end = (start + LENGTH).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// The sample named by a GitHub link to `.../examples/<name>`.
fn sample_in_url(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("/examples/")?;
    let name = rest.split(['/', '#', '?']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// `target` relative to the folder of the page `from`, with `..` and `.`
/// worked out: `join("learn/a.md", "../reference/b.md")` is `reference/b.md`.
pub(crate) fn join(from: &str, target: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for piece in target.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// GitHub's (and GitBook's) anchor for a heading: lower case, punctuation
/// dropped, spaces to `-`.
pub(crate) fn slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Reads the lines of a page (after its front matter, which was `offset`
/// lines) into blocks.
fn parse_blocks(lines: &[&str], offset: usize, problems: &mut Vec<String>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut index = 0;
    let flush = |paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(parse_inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    while index < lines.len() {
        let line = lines[index];
        let number = index + offset + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
            index += 1;
            continue;
        }
        if let Some(fence) = trimmed.strip_prefix("```") {
            flush(&mut paragraph, &mut blocks);
            let lang = fence.trim().to_owned();
            let mut code = Vec::new();
            index += 1;
            while index < lines.len() && !lines[index].trim_start().starts_with("```") {
                code.push(lines[index]);
                index += 1;
            }
            if index == lines.len() {
                problems.push(format!("{number}: a code block never ends"));
            }
            blocks.push(Block::Code {
                lang,
                text: code.join("\n"),
            });
            index += 1;
            continue;
        }
        if let Some(heading) = heading(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(heading);
            index += 1;
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Rule);
            index += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            flush(&mut paragraph, &mut blocks);
            let mut quote = Vec::new();
            while index < lines.len() && lines[index].trim().starts_with('>') {
                quote.push(lines[index].trim().trim_start_matches('>').trim());
                index += 1;
            }
            blocks.push(Block::Note(parse_inline(&quote.join(" "))));
            continue;
        }
        if trimmed.starts_with('|') {
            flush(&mut paragraph, &mut blocks);
            let mut table = Vec::new();
            while index < lines.len() && lines[index].trim().starts_with('|') {
                table.push(lines[index].trim());
                index += 1;
            }
            blocks.push(parse_table(&table, number, problems));
            continue;
        }
        if let Some((ordered, _)) = list_item(trimmed) {
            flush(&mut paragraph, &mut blocks);
            let mut items: Vec<String> = Vec::new();
            while index < lines.len() {
                let line = lines[index];
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break;
                }
                let indented = line.starts_with(' ') || line.starts_with('\t');
                match list_item(trimmed) {
                    Some((kind, text)) if !indented && kind == ordered => {
                        items.push(text.to_owned());
                    }
                    Some(_) if indented => {
                        problems.push(format!(
                            "{}: lists inside lists aren't shown",
                            index + offset + 1
                        ));
                        items.push(trimmed.to_owned());
                    }
                    Some(_) => break,
                    None => {
                        // A wrapped line carries on the item before it.
                        if let Some(last) = items.last_mut() {
                            last.push(' ');
                            last.push_str(trimmed);
                        }
                    }
                }
                index += 1;
            }
            blocks.push(Block::List {
                ordered,
                items: items.iter().map(|item| parse_inline(item)).collect(),
            });
            continue;
        }
        if trimmed.starts_with('<') || trimmed.starts_with("![") || trimmed.starts_with("{%") {
            problems.push(format!(
                "{number}: HTML, images and GitBook blocks aren't shown"
            ));
        }
        paragraph.push(trimmed);
        index += 1;
    }
    flush(&mut paragraph, &mut blocks);
    for block in &blocks {
        check_spans(block, problems);
    }
    blocks
}

/// A `#` heading line.
fn heading(line: &str) -> Option<Block> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if !(1..=4).contains(&level) {
        return None;
    }
    let text = line[level..].strip_prefix(' ')?.trim().to_owned();
    // Backticks show as code on GitHub, but the anchor drops them.
    let plain = text.replace('`', "");
    Some(Block::Heading {
        level: level as u8,
        anchor: slug(&plain),
        text: plain,
    })
}

/// `(ordered, text)` for a `- ` or `1. ` line.
fn list_item(line: &str) -> Option<(bool, &str)> {
    if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some((false, text));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        return line[digits..].strip_prefix(". ").map(|text| (true, text));
    }
    None
}

/// A pipe table: a header row, a `|---|` row, then rows.
fn parse_table(lines: &[&str], number: usize, problems: &mut Vec<String>) -> Block {
    let cells = |line: &str| -> Vec<Vec<Span>> {
        let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
        split_cells(inner)
            .iter()
            .map(|cell| parse_inline(cell.trim()))
            .collect()
    };
    let is_rule = |line: &str| line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '));
    if lines.len() < 2 || !is_rule(lines[1]) {
        problems.push(format!(
            "{number}: a table needs a `|---|` line under its header"
        ));
        return Block::Table {
            header: Vec::new(),
            rows: lines.iter().map(|line| cells(line)).collect(),
        };
    }
    let header = cells(lines[0]);
    let rows: Vec<Vec<Vec<Span>>> = lines[2..].iter().map(|line| cells(line)).collect();
    if rows.iter().any(|row| row.len() != header.len()) {
        problems.push(format!(
            "{number}: a table row has a different number of cells"
        ));
    }
    Block::Table { header, rows }
}

/// Splits a table row at `|`s that aren't inside backticks or escaped.
fn split_cells(row: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut in_code = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cells.last_mut().unwrap().push('|');
                chars.next();
            }
            '`' => {
                in_code = !in_code;
                cells.last_mut().unwrap().push(c);
            }
            '|' if !in_code => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

/// Flags unclosed markup left in a block's text.
fn check_spans(block: &Block, problems: &mut Vec<String>) {
    let check = |spans: &[Span], problems: &mut Vec<String>| {
        for span in spans {
            if let Span::Text(text) = span
                && (text.contains("**") || text.contains('`'))
            {
                problems.push(format!("unclosed `**` or backtick in: {text}"));
            }
        }
    };
    match block {
        Block::Paragraph(spans) | Block::Note(spans) => check(spans, problems),
        Block::List { items, .. } => items.iter().for_each(|item| check(item, problems)),
        Block::Table { header, rows } => header
            .iter()
            .chain(rows.iter().flatten())
            .for_each(|cell| check(cell, problems)),
        _ => {}
    }
}

/// Reads `**bold**`, `` `code` `` and `[text](target)` in a line of text.
pub(crate) fn parse_inline(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let push_plain = |plain: &mut String, spans: &mut Vec<Span>| {
        if !plain.is_empty() {
            spans.push(Span::Text(std::mem::take(plain)));
        }
    };
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            push_plain(&mut plain, &mut spans);
            spans.push(Span::Code(after[..end].to_owned()));
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            push_plain(&mut plain, &mut spans);
            spans.push(Span::Strong(after[..end].to_owned()));
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some(close) = after.find("](")
            && let Some(end) = after[close + 2..].find(')')
        {
            push_plain(&mut plain, &mut spans);
            let label = &after[..close];
            let code = label.len() >= 2 && label.starts_with('`') && label.ends_with('`');
            spans.push(Span::Link {
                text: label.trim_matches('`').to_owned(),
                target: after[close + 2..close + 2 + end].to_owned(),
                code,
            });
            rest = &after[close + 2 + end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('\\')
            && let Some(escaped) = after.chars().next()
            && "`*[]()|\\_".contains(escaped)
        {
            plain.push(escaped);
            rest = &after[escaped.len_utf8()..];
            continue;
        }
        plain.push(c);
        rest = &rest[c.len_utf8()..];
    }
    push_plain(&mut plain, &mut spans);
    spans
}

/// What a link into the generated API pages names: `classes/Part.md#touched`
/// is `Part.touched` (searched for without regard to case), `enums.md#material`
/// is `material`, and a list page such as `classes.md` is nothing.
fn api_entry(page: &str, anchor: Option<String>) -> String {
    let stem = page
        .rsplit('/')
        .next()
        .unwrap_or(page)
        .trim_end_matches(".md");
    let on_own_page = page.contains('/');
    match (on_own_page, anchor) {
        (true, Some(anchor)) => format!("{stem}.{anchor}"),
        (true, None) => stem.to_owned(),
        (false, Some(anchor)) => anchor,
        (false, None) => String::new(),
    }
}
