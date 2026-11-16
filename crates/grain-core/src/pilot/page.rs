//! Page reads from the Grain Pilot extension, shaped for the model.
//!
//! The extension's page script returns page-controller's simplified HTML
//! (`[12]<button aria-label=…>Text />`, one element per entry, page text in
//! between) with Grain's stable control numbers. This module turns it into
//! short rows (`[12] button "Text"`), applies the read views, caps the size,
//! and reports what changed after an action.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{clip, MAX_CHANGE_CHARS, MAX_READ_CHARS};

/// Which part of a page a read returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// The part of the page on screen: controls and the text around them.
    Visible,
    /// The whole page.
    Page,
    /// Rows of the whole page matching any of these words.
    Find(String),
}

impl View {
    /// The extension's read view for this view.
    pub fn extension_view(&self) -> &'static str {
        match self {
            View::Visible => "visible",
            View::Page | View::Find(_) => "full",
        }
    }
}

/// One control on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub number: u32,
    /// The row as the model sees it.
    pub row: String,
    /// Words that name the control (label, title, text), for the payment rule.
    pub name: String,
    /// A password, card or one-time-code field.
    pub secret: bool,
}

/// A read, shaped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub title: String,
    pub url: String,
    /// Rows in page order: controls and page text.
    pub rows: Vec<String>,
    pub controls: Vec<Control>,
    /// "More below" / "more above" hints from the extension.
    pub above: Option<String>,
    pub below: Option<String>,
}

impl Read {
    pub fn control(&self, number: u32) -> Option<&Control> {
        self.controls
            .iter()
            .find(|control| control.number == number)
    }
}

/// Parse the extension's read reply.
pub fn parse(value: &Value) -> Option<Read> {
    let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or("");
    let content = value.get("content")?.as_str()?;
    let mut read = Read {
        title: clip(text("title").trim(), 200),
        url: text("url").to_string(),
        above: hint(text("header")),
        below: hint(text("footer")),
        ..Read::default()
    };
    let mut open: Option<(usize, u32, String)> = None;
    for line in content.lines() {
        if let Some((depth, number, mut body)) = open.take() {
            body.push('\n');
            body.push_str(line.trim());
            if line.trim_end().ends_with("/>") {
                push_element(&mut read, depth, number, &body);
            } else {
                open = Some((depth, number, body));
            }
            continue;
        }
        let depth = line.len() - line.trim_start_matches('\t').len();
        let trimmed = line.trim();
        if let Some((number, body)) = element_start(trimmed) {
            if trimmed.ends_with("/>") {
                push_element(&mut read, depth, number, body);
            } else {
                open = Some((depth, number, body.to_string()));
            }
        } else if !trimmed.is_empty() && trimmed != "[Start of page]" && trimmed != "[End of page]"
        {
            read.rows
                .push(format!("{}{}", indent(depth), clip(trimmed, 200)));
        }
    }
    if let Some((depth, number, body)) = open {
        push_element(&mut read, depth, number, &body);
    }
    Some(read)
}

/// `*[12]<button …` → (12, "<button …").
fn element_start(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix('*').unwrap_or(line).strip_prefix('[')?;
    let close = rest.find(']')?;
    let number = rest[..close].parse().ok()?;
    let body = &rest[close + 1..];
    body.starts_with('<').then_some((number, body))
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth.min(8))
}

/// A scroll hint from the header or footer, if the page continues.
fn hint(text: &str) -> Option<String> {
    let line = text.lines().last()?.trim();
    let line = line.trim_matches('.').trim();
    (line.contains("pixels")).then(|| {
        line.replace(" - scroll to see more", "")
            .trim_end_matches('.')
            .trim()
            .to_string()
    })
}

/// Attributes worth showing; the rest (ids, classes, data-*) are noise.
const KEPT_ATTRIBUTES: &[&str] = &["value", "placeholder", "checked", "selected", "expanded"];

fn push_element(read: &mut Read, depth: usize, number: u32, body: &str) {
    // `<tag attrs>text />` or `<tag attrs />`.
    let body = body.trim().trim_end_matches("/>").trim_end();
    let body = body.strip_prefix('<').unwrap_or(body);
    let tag_end = body
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(body.len());
    let tag = &body[..tag_end];
    let (attrs, text) = match body[tag_end..].find('>') {
        Some(at) => (&body[tag_end..tag_end + at], &body[tag_end + at + 1..]),
        None => (&body[tag_end..], ""),
    };
    let attrs = attributes(attrs);
    let secret = attrs.iter().any(|(key, _)| key == "secret");
    let get = |name: &str| {
        attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let role = role(tag, get("role"), get("type"));
    let label = get("aria-label")
        .or_else(|| get("title"))
        .unwrap_or("")
        .trim();
    let text = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let text = clip(&text, 120);
    let shown_attribute = attrs.iter().any(|(key, value)| {
        KEPT_ATTRIBUTES.contains(&key.as_str()) && !(key == "checked" && value == "false")
    });
    // A nameless div, span or link (a click handler on a wrapper) tells the
    // model nothing and costs a row; form fields stay even unnamed.
    if label.is_empty() && text.is_empty() && !shown_attribute && !secret && !is_field(&role) {
        return;
    }
    let mut row = format!("{}[{number}] {role}", indent(depth));
    if !label.is_empty() {
        row.push_str(&format!(" \"{}\"", clip(label, 80)));
    }
    if !text.is_empty() && text != label {
        if label.is_empty() {
            row.push_str(&format!(" \"{text}\""));
        } else {
            row.push_str(&format!(" ({text})"));
        }
    }
    for (key, value) in &attrs {
        if KEPT_ATTRIBUTES.contains(&key.as_str()) && !(key == "checked" && value == "false") {
            row.push_str(&format!(" {key}={value}"));
        }
    }
    if secret {
        row.push_str(" secret");
    }
    let name = format!("{label} {text}").trim().to_string();
    read.rows.push(row.clone());
    read.controls.push(Control {
        number,
        row: row.trim_start().to_string(),
        name,
        secret,
    });
}

fn is_field(role: &str) -> bool {
    matches!(
        role,
        "field"
            | "search field"
            | "checkbox"
            | "radio"
            | "slider"
            | "dropdown"
            | "textbox"
            | "combobox"
    )
}

/// `type=search placeholder=Search Wikipedia id=x` → pairs. Values run to the
/// next ` key=`, since page-controller does not quote them. A bare word (the
/// page script's `secret`) has an empty value.
fn attributes(raw: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for word in raw.split_whitespace() {
        let starts_pair = word.split_once('=').is_some_and(|(key, _)| {
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
        if starts_pair {
            let (key, value) = word.split_once('=').unwrap_or((word, ""));
            out.push((key.to_ascii_lowercase(), value.to_string()));
        } else if word == "secret" {
            out.push(("secret".into(), String::new()));
        } else if let Some((_, value)) = out.last_mut() {
            value.push(' ');
            value.push_str(word);
        }
    }
    out
}

fn role(tag: &str, role: Option<&str>, kind: Option<&str>) -> String {
    if let Some(role) = role.filter(|role| !role.is_empty()) {
        return role.to_string();
    }
    match (tag, kind.unwrap_or("")) {
        ("a", _) => "link".into(),
        ("select", _) => "dropdown".into(),
        ("textarea", _) => "field".into(),
        ("input", "checkbox") => "checkbox".into(),
        ("input", "radio") => "radio".into(),
        ("input", "range") => "slider".into(),
        ("input", "button" | "submit" | "reset" | "image") => "button".into(),
        ("input", "search") => "search field".into(),
        ("input", _) => "field".into(),
        (other, _) => other.to_string(),
    }
}

/// The text the model reads for `view`, capped at [`MAX_READ_CHARS`].
pub fn render(read: &Read, view: &View) -> String {
    let rows: Vec<&String> = match view {
        View::Find(words) => {
            let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
            read.rows
                .iter()
                .filter(|row| {
                    let row = row.to_lowercase();
                    words.iter().any(|word| row.contains(word))
                })
                .collect()
        }
        _ => read.rows.iter().collect(),
    };
    let mut out = format!("{} ({})\n", read.title, host(&read.url));
    if let (Some(above), View::Visible) = (&read.above, view) {
        out.push_str(&format!("({above})\n"));
    }
    if rows.is_empty() {
        out.push_str(match view {
            View::Find(_) => "Nothing on the page matches.\n",
            _ => "The page shows nothing to read yet.\n",
        });
    }
    let mut omitted = 0;
    for row in &rows {
        if out.len() + row.len() + 1 > MAX_READ_CHARS {
            omitted += 1;
            continue;
        }
        out.push_str(row.trim_end());
        out.push('\n');
    }
    if omitted > 0 {
        out.push_str(&format!(
            "… {omitted} more rows. Use find, or scroll and read again.\n"
        ));
    }
    if let (Some(below), View::Visible) = (&read.below, view) {
        out.push_str(&format!("({below}; scroll, or read detail=page)\n"));
    }
    out.trim_end().to_string()
}

/// What changed between two reads of the same tab, capped at
/// [`MAX_CHANGE_CHARS`]. A new page lists its rows as a fresh read would.
pub fn changes(before: &Read, after: &Read) -> String {
    if before.url != after.url || before.title != after.title {
        let mut text = format!("Now on: {}", render(after, &View::Visible));
        if text.len() > MAX_CHANGE_CHARS {
            text = clip(&text, MAX_CHANGE_CHARS);
        }
        return text;
    }
    let seen: HashSet<&str> = before.rows.iter().map(|row| row.trim()).collect();
    let now: HashSet<&str> = after.rows.iter().map(|row| row.trim()).collect();
    let added: Vec<&str> = after
        .rows
        .iter()
        .map(|row| row.trim())
        .filter(|row| !seen.contains(row))
        .collect();
    let removed = before
        .rows
        .iter()
        .filter(|row| !now.contains(row.trim()))
        .count();
    if added.is_empty() && removed == 0 {
        return "Nothing visible changed.".into();
    }
    let mut text = String::new();
    if !added.is_empty() {
        text.push_str("New or changed:\n");
        let mut left = 0;
        for row in &added {
            if text.len() + row.len() + 1 > MAX_CHANGE_CHARS {
                left += 1;
                continue;
            }
            text.push_str(row);
            text.push('\n');
        }
        if left > 0 {
            text.push_str(&format!("… {left} more; read to see them.\n"));
        }
    }
    if removed > 0 {
        text.push_str(&format!("{removed} rows went away.\n"));
    }
    text.trim_end().to_string()
}

/// Whether a read is only a loading placeholder ("Please wait…", "Loading…")
/// or empty, so it is worth reading again shortly.
pub fn looks_loading(read: &Read) -> bool {
    if read.rows.is_empty() {
        return true;
    }
    read.rows.len() <= 3
        && read.rows.iter().any(|row| {
            let row = row.to_lowercase();
            row.contains("please wait") || row.contains("loading")
        })
}

/// Host of a URL for logs and headers, never the path or query.
pub fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_string()
}

/// Control numbers to rows, for looking a number up in the last reads.
pub fn controls_by_number(read: &Read) -> HashMap<u32, &Control> {
    read.controls.iter().map(|c| (c.number, c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHOWTIMES: &str = include_str!("../../tests/fixtures/pilot/pc_bms_showtimes.txt");
    const SEATS: &str = include_str!("../../tests/fixtures/pilot/pc_bms_seats.txt");
    const ZOMATO: &str = include_str!("../../tests/fixtures/pilot/pc_zomato.txt");
    const WIKIPEDIA: &str = include_str!("../../tests/fixtures/pilot/pc_wikipedia.txt");

    fn read(content: &str) -> Read {
        parse(&json!({
            "url": "https://in.bookmyshow.com/x?y=1",
            "title": "Page",
            "header": "Current Page: …\n\n[Start of page]",
            "footer": "... 1200 pixels below (1.5 pages) - scroll to see more ...",
            "content": content,
        }))
        .unwrap()
    }

    #[test]
    fn rows_are_short_and_keep_numbers_labels_and_values() {
        let seats = read(SEATS);
        let rows = seats.rows.join("\n");
        assert!(rows.contains("[0] slider value=2"), "{rows}");
        assert!(rows.contains("[3] button \"Select Seats\""), "{rows}");
        assert!(rows.contains("How many seats?"));
        // A multi-line element joins its text.
        assert!(rows.contains("NORMAL · ₹500 · Available"), "{rows}");
        let shows = read(SHOWTIMES);
        let show = shows
            .controls
            .iter()
            .find(|c| c.name.contains("Book 07:30 AM"))
            .expect("show button");
        assert_eq!(
            show.row,
            format!("[{}] button \"Book 07:30 AM\" (IMAX)", show.number)
        );
    }

    #[test]
    fn noise_attributes_are_dropped() {
        let wiki = read(WIKIPEDIA);
        let rows = wiki.rows.join("\n");
        assert!(!rows.contains("id="), "ids leaked");
        assert!(!rows.contains("data-"), "data attributes leaked");
        assert!(!rows.contains("/>"));
        // Nameless wrappers are dropped; the labelled controls stay.
        assert!(
            !rows.lines().any(|row| row.trim_end().ends_with("] div")),
            "{rows}"
        );
        assert!(!rows.lines().any(|row| row.trim_end().ends_with("] span")));
        assert!(rows.contains("link \"Donate\""));
    }

    #[test]
    fn reads_fit_and_are_smaller_than_the_raw_page() {
        for (name, raw) in [
            ("showtimes", SHOWTIMES),
            ("zomato", ZOMATO),
            ("wikipedia", WIKIPEDIA),
        ] {
            let shown = render(&read(raw), &View::Page);
            assert!(
                shown.len() <= MAX_READ_CHARS + 200,
                "{name}: {}",
                shown.len()
            );
            assert!(
                shown.len() < raw.len(),
                "{name}: {} vs {}",
                shown.len(),
                raw.len()
            );
            println!(
                "{name}: raw ~{} tokens, shown ~{} tokens",
                raw.len() / 4,
                shown.len() / 4
            );
        }
    }

    #[test]
    fn find_keeps_only_matching_rows() {
        let shown = render(&read(SHOWTIMES), &View::Find("07:30".into()));
        assert!(shown.contains("Book 07:30 AM"), "{shown}");
        assert!(shown.len() < 1_000, "{}", shown.len());
        let none = render(&read(SEATS), &View::Find("zzzz".into()));
        assert!(none.contains("Nothing on the page matches"));
    }

    #[test]
    fn visible_view_carries_the_scroll_hint() {
        let shown = render(&read(SEATS), &View::Visible);
        assert!(shown.contains("1200 pixels below"), "{shown}");
        assert!(shown.starts_with("Page (in.bookmyshow.com)"), "{shown}");
    }

    #[test]
    fn secret_fields_are_marked() {
        let page = read("*[4]<input secret type=password placeholder=Password />");
        let field = page.control(4).unwrap();
        assert!(field.secret);
        assert!(field.row.ends_with("secret"), "{}", field.row);
    }

    #[test]
    fn changes_on_the_same_page_and_on_a_new_page() {
        let before = read(SEATS);
        let mut after = before.clone();
        after.rows.push("[9] button \"Proceed\"".into());
        let report = changes(&before, &after);
        assert!(
            report.contains("New or changed:\n[9] button \"Proceed\""),
            "{report}"
        );
        assert_eq!(changes(&before, &before), "Nothing visible changed.");
        let moved = Read {
            url: "https://in.bookmyshow.com/seat".into(),
            ..after.clone()
        };
        assert!(changes(&before, &moved).starts_with("Now on: Page"));
    }

    #[test]
    fn loading_placeholders_are_recognised() {
        let waiting = read("[0]<a >Skip to main content />\nPlease wait, while we load the seats");
        assert!(looks_loading(&waiting));
        assert!(looks_loading(&Read::default()));
        assert!(!looks_loading(&read(SEATS)));
    }

    #[test]
    fn hosts_drop_paths_and_queries() {
        assert_eq!(
            host("https://in.bookmyshow.com/buytickets/CPVM?x=1"),
            "in.bookmyshow.com"
        );
        assert_eq!(host("not a url"), "not a url");
    }
}
