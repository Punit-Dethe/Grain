//! Page reads from the Grain Pilot extension, shaped for the model.
//!
//! The extension's page script returns page-controller's simplified HTML
//! (`[12]<button aria-label=…>Text />`, one element per entry, page text in
//! between) with Grain's stable control numbers. This module turns it into
//! short rows, applies the read views, caps the size, and reports what changed
//! after an action.
//!
//! Rows are written for the fewest tokens that stay unambiguous: a plain
//! clickable control is just `[12] Book 07:30 AM (IMAX)`; only controls that
//! take a different step say what they are (`[4] field Search`,
//! `[7] dropdown Seats`, `[9] checkbox Terms checked`). No indents, no quotes.

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
    let read = Read {
        title: clip(text("title").trim(), 200),
        url: text("url").to_string(),
        above: hint(text("header")),
        below: hint(text("footer")),
        ..Read::default()
    };
    Some(shape(read, items(content)))
}

/// One element of a read, before it becomes a row.
struct Element {
    number: u32,
    /// Nesting depth (page-controller indents children with tabs).
    depth: usize,
    tag: String,
    kind: &'static str,
    /// Its own name: aria-label, title, image alt, or a button's value.
    label: String,
    text: String,
    attrs: Vec<(String, String)>,
    secret: bool,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

enum Item {
    Text(String),
    Element(Element),
}

/// The read's content as elements and page text, in page order.
fn items(content: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let mut open: Option<(usize, u32, String)> = None;
    for line in content.lines() {
        if let Some((depth, number, mut body)) = open.take() {
            body.push('\n');
            body.push_str(line.trim());
            if line.trim_end().ends_with("/>") {
                items.push(Item::Element(element(number, depth, &body)));
            } else {
                open = Some((depth, number, body));
            }
            continue;
        }
        let depth = line.len() - line.trim_start_matches('\t').len();
        let trimmed = line.trim();
        if let Some((number, body)) = element_start(trimmed) {
            if trimmed.ends_with("/>") {
                items.push(Item::Element(element(number, depth, body)));
            } else {
                open = Some((depth, number, body.to_string()));
            }
        } else if !trimmed.is_empty() && trimmed != "[Start of page]" && trimmed != "[End of page]"
        {
            items.push(Item::Text(clip(trimmed, MAX_TEXT_ROW_CHARS)));
        }
    }
    if let Some((depth, number, body)) = open {
        items.push(Item::Element(element(number, depth, &body)));
    }
    items
}

/// `*[12]<button …` → (12, "<button …").
fn element_start(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix('*').unwrap_or(line).strip_prefix('[')?;
    let close = rest.find(']')?;
    let number = rest[..close].parse().ok()?;
    let body = &rest[close + 1..];
    body.starts_with('<').then_some((number, body))
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

/// Longest line of plain page text kept in a row.
const MAX_TEXT_ROW_CHARS: usize = 160;
/// A nested control repeating its parent's name is dropped only when the
/// repeat is at least this long, so short distinct controls ("+", "M") stay.
const MIN_REPEAT_CHARS: usize = 4;

/// `<tag attrs>text />` or `<tag attrs />` as an element.
fn element(number: u32, depth: usize, body: &str) -> Element {
    let body = body.trim().trim_end_matches("/>").trim_end();
    let body = body.strip_prefix('<').unwrap_or(body);
    let tag_end = body
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(body.len());
    let tag = body[..tag_end].to_ascii_lowercase();
    let (attrs, text) = match body[tag_end..].find('>') {
        Some(at) => (&body[tag_end..tag_end + at], &body[tag_end + at + 1..]),
        None => (&body[tag_end..], ""),
    };
    let attrs = attributes(attrs);
    let get = |name: &str| {
        attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
    };
    let input_type = get("type");
    let kind = kind(&tag, get("role"), input_type);
    let is_button_input =
        tag == "input" && matches!(input_type, Some("submit" | "button" | "reset"));
    let label = get("aria-label")
        .or_else(|| get("title"))
        .or_else(|| get("alt"))
        .or_else(|| if is_button_input { get("value") } else { None })
        .unwrap_or("")
        .trim()
        .to_string();
    let text = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    Element {
        number,
        depth,
        secret: attrs.iter().any(|(key, _)| key == "secret"),
        tag,
        kind,
        label,
        text: clip(&text, 120),
        attrs,
    }
}

/// `type=search placeholder=Search Wikipedia id=x` → pairs. Values run to the
/// next ` key=`, since page-controller does not quote them; the page script's
/// own `value="…"` is unquoted, and a repeated key keeps its first value. ARIA
/// state is folded into the plain names (`aria-expanded` → `expanded`). A bare
/// word (the page script's `secret`) has an empty value.
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
    let mut seen = HashSet::new();
    out.into_iter()
        .map(|(key, value)| {
            let key = match key.as_str() {
                "aria-expanded" => "expanded".to_string(),
                "aria-checked" => "checked".to_string(),
                "aria-selected" => "selected".to_string(),
                _ => key,
            };
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value)
                .to_string();
            (key, value)
        })
        .filter(|(key, _)| seen.insert(key.clone()))
        .collect()
}

/// What a control is, said only when it takes something other than a click:
/// `field` (fill), `dropdown` (choose), `checkbox`, `radio`, `switch`,
/// `slider`, `file`. Empty for anything clicked: links, buttons, tabs, menu
/// items.
fn kind(tag: &str, role: Option<&str>, input_type: Option<&str>) -> &'static str {
    if let Some(role) = role {
        return match role {
            "textbox" | "searchbox" | "combobox" => "field",
            "checkbox" | "menuitemcheckbox" => "checkbox",
            "radio" | "menuitemradio" => "radio",
            "switch" => "switch",
            "slider" | "spinbutton" => "slider",
            _ => "",
        };
    }
    match (tag, input_type.unwrap_or("")) {
        ("select", _) => "dropdown",
        ("textarea", _) => "field",
        ("input", "checkbox") => "checkbox",
        ("input", "radio") => "radio",
        ("input", "range") => "slider",
        ("input", "file") => "file",
        ("input", "button" | "submit" | "reset" | "image" | "hidden") => "",
        ("input", _) => "field",
        _ => "",
    }
}

/// Turn elements and text into rows:
///
/// - A field without a name of its own takes its label's text: the `<label>`
///   wrapping it, the `<label for=…>` naming its id, or (for a checkbox or
///   radio) the text right after it. That label then shows no row of its own.
/// - A control nested in the control before it, repeating part of its name
///   ("04:05 PM" inside "Book 04:05 PM"), shows no row: clicking the parent
///   does the same.
/// - A nameless wrapper (a click handler on a div) shows no row.
///
/// Every element with a name stays in `controls`, rowless or not, so a number
/// the model has seen always resolves and the payment rule sees its name.
fn shape(mut read: Read, items: Vec<Item>) -> Read {
    let mut hidden = vec![false; items.len()];
    let mut borrowed: Vec<Option<String>> = vec![None; items.len()];
    for at in 0..items.len() {
        let Item::Element(field) = &items[at] else {
            continue;
        };
        if field.kind.is_empty() || !field.label.is_empty() {
            continue;
        }
        let wrapping = at.checked_sub(1).and_then(|before| match &items[before] {
            Item::Element(label)
                if label.tag == "label"
                    && label.depth + 1 == field.depth
                    && !label.text.is_empty() =>
            {
                Some(before)
            }
            _ => None,
        });
        let pointing = || {
            let id = field.attr("id").filter(|id| !id.is_empty())?;
            items.iter().position(|item| {
                matches!(item, Item::Element(label)
                    if label.tag == "label" && label.attr("for") == Some(id) && !label.text.is_empty())
            })
        };
        // A label right before the field that names no other element (the
        // field's id can be missing from the read, as on password fields).
        let before = || {
            let before = at.checked_sub(1)?;
            let Item::Element(label) = &items[before] else {
                return None;
            };
            let names_other = label.attr("for").is_some_and(|target| {
                items.iter().any(|item| {
                    matches!(item, Item::Element(other)
                        if other.number != field.number && other.attr("id") == Some(target))
                })
            });
            (label.tag == "label" && !label.text.is_empty() && !names_other).then_some(before)
        };
        if let Some(label_at) = wrapping.or_else(pointing).or_else(before) {
            if let Item::Element(label) = &items[label_at] {
                borrowed[at] = Some(label.text.clone());
                hidden[label_at] = true;
            }
        } else if matches!(field.kind, "checkbox" | "radio" | "switch") {
            if let Some(Item::Text(text)) = items.get(at + 1) {
                borrowed[at] = Some(text.clone());
                hidden[at + 1] = true;
            }
        }
    }
    for at in 1..items.len() {
        let (Item::Element(parent), Item::Element(child)) = (&items[at - 1], &items[at]) else {
            continue;
        };
        if hidden[at - 1] || !child.kind.is_empty() || child.depth != parent.depth + 1 {
            continue;
        }
        let repeat = if child.label.is_empty() {
            &child.text
        } else {
            &child.label
        };
        let parent_name = format!("{} {}", parent.label, parent.text).to_lowercase();
        if repeat.chars().count() >= MIN_REPEAT_CHARS
            && parent_name.contains(&repeat.to_lowercase())
        {
            hidden[at] = true;
        }
    }
    // page-controller cuts long names to 20 characters plus "...". A control
    // whose cut name begins the next control's name (a product's image link
    // before its title link) repeats it.
    for at in 0..items.len().saturating_sub(1) {
        let (Item::Element(cut), Item::Element(next)) = (&items[at], &items[at + 1]) else {
            continue;
        };
        let Some(stem) = cut.label.strip_suffix("...").map(str::trim_end) else {
            continue;
        };
        let next_name = format!("{} {}", next.label, next.text);
        if cut.kind.is_empty()
            && next.kind.is_empty()
            && cut.text.is_empty()
            && stem.chars().count() >= MIN_REPEAT_CHARS
            && next_name.trim_start().starts_with(stem)
        {
            hidden[at] = true;
        }
    }
    for (at, item) in items.into_iter().enumerate() {
        match item {
            Item::Text(text) => {
                if !hidden[at] {
                    read.rows.push(text);
                }
            }
            Item::Element(element) => {
                if let Some(control) = control(&element, borrowed[at].as_deref()) {
                    if !hidden[at] {
                        read.rows.push(control.row.clone());
                    }
                    read.controls.push(control);
                }
            }
        }
    }
    read
}

/// An element's row and control, or `None` for a nameless wrapper.
fn control(element: &Element, borrowed: Option<&str>) -> Option<Control> {
    let mut label = if element.label.is_empty() {
        borrowed.unwrap_or("").trim()
    } else {
        element.label.as_str()
    };
    // A name cut to "Lofi Chill S..." whose text carries it in full shows
    // the text once.
    if let Some(stem) = label.strip_suffix("...").map(str::trim_end) {
        if !stem.is_empty() && element.text.starts_with(stem) {
            label = "";
        }
    }
    // Text the name already holds ("Home · Home", "Book 04:40 PM · 04:40 PM")
    // says nothing more.
    let text = element
        .text
        .split(" · ")
        .filter(|part| !label.contains(part))
        .collect::<Vec<_>>()
        .join(" · ");
    let text = text.as_str();
    let mut extras = String::new();
    for (key, value) in &element.attrs {
        match (key.as_str(), value.as_str()) {
            ("checked" | "selected", "false" | "") => {}
            ("checked" | "selected", _) => extras.push_str(&format!(" {key}")),
            // Open or closed matters for menus, not for a field's suggestions.
            ("expanded", value) if !value.is_empty() && element.kind != "field" => {
                extras.push_str(&format!(" expanded={value}"))
            }
            // A dropdown's text lists every option; its value is the chosen one.
            ("value", value) if element.kind == "dropdown" && !value.is_empty() => {
                extras.push_str(&format!(" value={}", clip(value, 80)))
            }
            ("placeholder" | "value", value)
                if value.is_empty()
                    || value == label
                    || value == element.label
                    || text.contains(value) => {}
            ("placeholder" | "value", value) => {
                extras.push_str(&format!(" {key}={}", clip(value, 80)))
            }
            _ => {}
        }
    }
    // A nameless div, span or link (a click handler on a wrapper) tells the
    // model nothing and costs a row; form fields stay even unnamed.
    if label.is_empty()
        && text.is_empty()
        && extras.is_empty()
        && !element.secret
        && element.kind.is_empty()
    {
        return None;
    }
    let mut row = format!("[{}]", element.number);
    if !element.kind.is_empty() {
        row.push(' ');
        row.push_str(element.kind);
    }
    if !label.is_empty() {
        row.push(' ');
        row.push_str(&clip(label, 80));
    }
    if !text.is_empty() && text != label {
        if label.is_empty() {
            row.push(' ');
            row.push_str(text);
        } else {
            row.push_str(&format!(" ({text})"));
        }
    }
    row.push_str(&extras);
    if element.secret {
        row.push_str(" secret");
    }
    Some(Control {
        number: element.number,
        row,
        name: format!("{label} {text}").trim().to_string(),
        secret: element.secret,
    })
}

/// The rows the model reads for `view`, capped at [`MAX_READ_CHARS`]. The
/// header (tab, title, host) is the reply's, in `render.rs`.
pub fn body(read: &Read, view: &View) -> String {
    let rows: Vec<&str> = match view {
        View::Find(words) => found(&read.rows, words),
        _ => read.rows.iter().map(String::as_str).collect(),
    };
    let mut out = String::new();
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
            omitted += usize::from(*row != GAP);
            continue;
        }
        out.push_str(row.trim_end());
        out.push('\n');
    }
    if omitted > 0 {
        out.push_str(&format!("… {omitted} more rows: use find, or scroll.\n"));
    }
    if let (Some(below), View::Visible) = (&read.below, view) {
        out.push_str(&format!("({below})\n"));
    }
    out.trim_end().to_string()
}

/// Marks rows skipped between two find results.
const GAP: &str = "…";

/// Rows kept before the first of a run of matches, so a match keeps what it
/// belongs to ("Aadi Anant", "Marathi" before its "Book 07:30 AM").
const FIND_CONTEXT_ROWS: usize = 2;

/// Rows matching any of `words`, each run led by its context rows, with
/// [`GAP`] where rows were skipped.
fn found<'a>(rows: &'a [String], words: &str) -> Vec<&'a str> {
    let mut words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
    // "to", "of", "a" match nearly every row; they count only when nothing
    // longer was asked for.
    if words.iter().any(|word| word.chars().count() > 2) {
        words.retain(|word| word.chars().count() > 2);
    }
    let hits: Vec<bool> = rows
        .iter()
        .map(|row| {
            let row = row.to_lowercase();
            words.iter().any(|word| row.contains(word))
        })
        .collect();
    if !hits.contains(&true) {
        return Vec::new();
    }
    let mut keep = hits.clone();
    for at in 0..rows.len() {
        if !hits[at] || (at > 0 && hits[at - 1]) {
            continue;
        }
        // The nearest rows that say something, skipping separators like "|".
        let mut wanted = FIND_CONTEXT_ROWS;
        for before in (at.saturating_sub(FIND_CONTEXT_ROWS * 2)..at).rev() {
            if wanted == 0 || keep[before] {
                break;
            }
            if says_something(&rows[before]) {
                keep[before] = true;
                wanted -= 1;
            }
        }
    }
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    for (at, row) in rows.iter().enumerate().filter(|(at, _)| keep[*at]) {
        if let Some(last) = last {
            if rows[last + 1..at].iter().any(|row| says_something(row)) {
                out.push(GAP);
            }
        }
        out.push(row.as_str());
        last = Some(at);
    }
    out
}

/// Whether a row has at least three letters or digits besides its number.
fn says_something(row: &str) -> bool {
    let text = row
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .map_or(row, |(_, text)| text);
    text.chars().filter(|c| c.is_alphanumeric()).count() >= 3
}

/// What an action did to a tab's screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A different page: report it as a fresh read.
    NewPage,
    Nothing,
    /// Rows that appeared or changed, then how many went away, capped at
    /// [`MAX_CHANGE_CHARS`].
    Rows(String),
}

/// What changed between two on-screen reads of the same tab.
pub fn changes(before: &Read, after: &Read) -> Change {
    if before.url != after.url || before.title != after.title {
        return Change::NewPage;
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
        return Change::Nothing;
    }
    let mut text = String::new();
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
        text.push_str(&format!("… {left} more: read to see them.\n"));
    }
    if removed > 0 {
        text.push_str(&format!("({removed} rows went away)\n"));
    }
    Change::Rows(text.trim_end().to_string())
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
        assert!(rows.contains("[3] Select Seats"), "{rows}");
        assert!(!rows.contains('"'), "rows carry no quotes");
        assert!(!rows.lines().any(|row| row.starts_with(' ')), "no indents");
        assert!(rows.contains("How many seats?"));
        // A multi-line element joins its text.
        assert!(rows.contains("NORMAL · ₹500 · Available"), "{rows}");
        let shows = read(SHOWTIMES);
        let show = shows
            .controls
            .iter()
            .find(|c| c.name.contains("Book 07:30 AM"))
            .expect("show button");
        assert_eq!(show.row, format!("[{}] Book 07:30 AM (IMAX)", show.number));
    }

    #[test]
    fn only_non_click_controls_say_what_they_are() {
        let page = read(
            "[1]<a >Home />\n[2]<input type=search placeholder=Search />\n[3]<select aria-label=Seats />\n[4]<input type=checkbox checked=true aria-label=Terms />\n[5]<input type=checkbox checked=false role=button aria-label=Menu />\n[6]<div role=combobox aria-label=City />",
        );
        let rows: Vec<&str> = page.rows.iter().map(String::as_str).collect();
        assert_eq!(
            rows,
            [
                "[1] Home",
                "[2] field placeholder=Search",
                "[3] dropdown Seats",
                "[4] checkbox Terms checked",
                "[5] Menu",
                "[6] field City",
            ]
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
            !rows.lines().any(|row| row.trim_end().ends_with(']')),
            "{rows}"
        );
        assert!(rows.contains("] Donate"), "{rows}");
    }

    #[test]
    fn reads_fit_and_are_smaller_than_the_raw_page() {
        for (name, raw) in [
            ("showtimes", SHOWTIMES),
            ("zomato", ZOMATO),
            ("wikipedia", WIKIPEDIA),
        ] {
            let shown = body(&read(raw), &View::Page);
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
        let shown = body(&read(SHOWTIMES), &View::Find("07:30".into()));
        assert!(shown.contains("Book 07:30 AM"), "{shown}");
        assert!(shown.len() < 1_000, "{}", shown.len());
        let none = body(&read(SEATS), &View::Find("zzzz".into()));
        assert!(none.contains("Nothing on the page matches"));
    }

    #[test]
    fn find_leads_each_run_of_matches_with_its_context() {
        let rows: Vec<String> = [
            "Header",
            "Movie A",
            "Hindi",
            "[1] Book 10:00",
            "[2] Book 13:00",
            "Movie B",
            "English",
            "[3] Book 18:00",
            "Footer",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            found(&rows, "book"),
            [
                "Movie A",
                "Hindi",
                "[1] Book 10:00",
                "[2] Book 13:00",
                "Movie B",
                "English",
                "[3] Book 18:00"
            ]
        );
        assert_eq!(
            found(&rows, "13:00 footer"),
            [
                "Hindi",
                "[1] Book 10:00",
                "[2] Book 13:00",
                GAP,
                "English",
                "[3] Book 18:00",
                "Footer"
            ]
        );
        assert_eq!(found(&rows, "header"), ["Header"]);
        assert!(found(&rows, "zzzz").is_empty());

        // Separators are skipped as context and never cause a gap mark.
        let news: Vec<String> = [
            "Story",
            "[1] 3 hours ago",
            "|",
            "[2] hide",
            "|",
            "[3] 219 comments",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            found(&news, "comments"),
            ["[1] 3 hours ago", "[2] hide", "[3] 219 comments"]
        );
        // Short words count only when nothing longer was asked for.
        let menu: Vec<String> = ["Go to top", "[1] EN"].map(String::from).to_vec();
        assert!(found(&menu, "add to cart").is_empty());
        assert_eq!(found(&menu, "to"), ["Go to top"]);
        assert_eq!(found(&menu, "EN"), ["Go to top", "[1] EN"]);

        let shows = body(&read(SHOWTIMES), &View::Find("Book".into()));
        let first = shows
            .lines()
            .position(|row| row.contains("] Book "))
            .unwrap();
        assert!(
            first >= 1,
            "the first show keeps its movie:
{shows}"
        );
    }

    #[test]
    fn cut_names_and_repeated_text_show_once() {
        let page = read(concat!(
            "[1]<a aria-label=Zebronics Wattz 60CC... />
",
            "[2]<a >Zebronics Wattz 60CC4 Type-C cable />
",
            "[3]<a aria-label=90's Chill Lofi S... >90's Chill Lofi Study Music />
",
            "[4]<a aria-label=Home >Home · Home />
",
            "[5]<input role=combobox aria-expanded=false placeholder=Search />
",
            "[6]<button aria-expanded=false >Menu />\n",
            "[7]<button aria-label=Book 04:40 PM >04:40 PM · IMAX />",
        ));
        assert_eq!(
            page.rows,
            [
                "[2] Zebronics Wattz 60CC4 Type-C cable",
                "[3] 90's Chill Lofi Study Music",
                "[4] Home",
                "[5] field placeholder=Search",
                "[6] Menu expanded=false",
                "[7] Book 04:40 PM (IMAX)",
            ]
        );
        assert!(
            page.control(1).is_some(),
            "the hidden cut name still resolves"
        );
    }

    #[test]
    fn visible_view_carries_the_scroll_hint() {
        let shown = body(&read(SEATS), &View::Visible);
        assert!(shown.contains("1200 pixels below"), "{shown}");
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
        after.rows.push("[9] Proceed".into());
        after.rows.remove(0);
        assert_eq!(
            changes(&before, &after),
            Change::Rows("[9] Proceed\n(1 rows went away)".into())
        );
        assert_eq!(changes(&before, &before), Change::Nothing);
        let moved = Read {
            url: "https://in.bookmyshow.com/seat".into(),
            ..after.clone()
        };
        assert_eq!(changes(&before, &moved), Change::NewPage);
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

    // ── Captured sites ──────────────────────────────────────────────────
    //
    // Real page-controller reads of public pages of many kinds, saved by the
    // desktop crate's `live_pilot_capture_site_fixtures`.

    fn sites_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("pilot")
            .join("sites")
    }

    fn site(file: &str) -> (Value, Read) {
        let raw: Value = serde_json::from_str(
            &std::fs::read_to_string(sites_dir().join(file))
                .unwrap_or_else(|e| panic!("{file}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{file}: {e}"));
        let read = parse(&raw).unwrap_or_else(|| panic!("{file}: unreadable"));
        (raw, read)
    }

    fn all_sites() -> Vec<String> {
        let mut files: Vec<String> = std::fs::read_dir(sites_dir())
            .expect("captured sites")
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.ends_with(".json"))
            .collect();
        files.sort();
        assert!(files.len() >= 20, "{} captured reads", files.len());
        files
    }

    #[test]
    fn every_captured_site_keeps_every_named_control() {
        for file in all_sites() {
            let (raw, read) = site(&file);
            let content = raw["content"].as_str().unwrap_or("");
            let known: HashSet<u32> = read.controls.iter().map(|c| c.number).collect();
            for item in items(content) {
                if let Item::Element(element) = item {
                    let named = !element.label.is_empty()
                        || !element.text.is_empty()
                        || !element.kind.is_empty();
                    assert!(
                        !named || known.contains(&element.number),
                        "{file}: [{}] lost",
                        element.number
                    );
                }
            }
            let mut shown = HashSet::new();
            for row in &read.rows {
                assert!(!row.starts_with(' ') && !row.is_empty(), "{file}: {row:?}");
                if let Some(number) = row
                    .strip_prefix('[')
                    .and_then(|rest| rest.split_once(']'))
                    .and_then(|(n, rest)| Some((n.parse::<u32>().ok()?, rest)))
                {
                    assert!(!number.1.trim().is_empty(), "{file}: bare row {row}");
                    assert!(known.contains(&number.0), "{file}: {row}");
                    assert!(shown.insert(number.0), "{file}: [{}] twice", number.0);
                }
            }
            let page = body(&read, &View::Page);
            assert!(page.len() <= MAX_READ_CHARS + 200, "{file}: {}", page.len());
            println!(
                "{file}: raw ~{} tokens, shown ~{} tokens",
                content.len() / 4,
                page.len() / 4
            );
        }
    }

    /// Print captured reads as the model sees them, for review:
    /// `PILOT_SITES=amazon_search.visible.json,… [PILOT_FIND=words] cargo test -p grain-core
    /// --lib show_captured -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn show_captured_sites() {
        let files = std::env::var("PILOT_SITES").unwrap_or_default();
        for file in files.split(',').filter(|file| !file.is_empty()) {
            let (_, read) = site(file);
            let view = match std::env::var("PILOT_FIND") {
                Ok(words) => View::Find(words),
                Err(_) if file.contains(".full.") => View::Page,
                Err(_) => View::Visible,
            };
            println!("===== {file}\n{}\n", body(&read, &view));
        }
    }

    #[test]
    fn form_controls_are_named_from_their_labels() {
        let (_, github) = site("github_login.visible.json");
        let rows = github.rows.join("\n");
        for row in [
            "[2] field Username or email address",
            "[4] field Password secret",
            "[6] Sign in",
        ] {
            assert!(rows.lines().any(|line| line == row), "{row}\n{rows}");
        }
        assert!(
            !rows.contains("[1] "),
            "the folded label has no row:\n{rows}"
        );
        assert!(github.control(1).is_some(), "but its number still resolves");

        let (_, form) = site("selenium_form.full.json");
        let rows = form.rows.join("\n");
        for row in [
            "[2] field Text input",
            "[4] field Password secret",
            "[6] field Textarea",
            "[12] dropdown Dropdown (select) (Open this select menu · One · Two · Three) value=Open this select menu",
            "[16] file File input",
            "[18] checkbox Checked checkbox checked",
            "[20] checkbox Default checkbox",
            "[22] radio Checked radio checked",
            "[25] Submit",
            "[31] slider Example range value=5",
        ] {
            assert!(rows.lines().any(|line| line == row), "{row}\n{rows}");
        }

        let (_, boxes) = site("internet_checkboxes.full.json");
        let rows = boxes.rows.join("\n");
        assert!(rows.contains("[2] checkbox checkbox 1\n"), "{rows}");
        assert!(rows.contains("[3] checkbox checkbox 2 checked"), "{rows}");
    }

    #[test]
    fn nested_repeats_are_dropped_but_short_names_stay() {
        let shows = read(SHOWTIMES);
        let rows = shows.rows.join("\n");
        assert!(rows.contains("Book 04:05 PM"), "{rows}");
        assert!(
            !rows.lines().any(|row| row.ends_with("] 04:05 PM")),
            "nested time repeats its Book button:\n{rows}"
        );
        // The repeat still resolves for a number seen earlier.
        assert!(shows.control(121).is_some());
        let page = read("[1]<button aria-label=Add one />\n\t[2]<span >+ />");
        assert_eq!(page.rows, ["[1] Add one", "[2] +"]);
    }
}
