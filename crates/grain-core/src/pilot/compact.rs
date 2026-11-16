//! Turning a driver read into the short list the model reads.
//!
//! A raw `get_window_state` tree of a busy page is ~16k tokens: every text
//! node, link address, action list, element id, deep indentation, and the
//! browser's own toolbars and tab strip (which name every tab the user has
//! open). Compaction, in order:
//!
//! 1. Keep only the page: the first `Document` row and its subtree. For a
//!    browser this drops the toolbars and the tab strip (a privacy rule, not
//!    only a token saving). Windows without a document keep every row.
//! 2. Drop link addresses, action lists and element ids (never parsed).
//! 3. Drop rows whose text only repeats their children's text.
//! 4. Drop a row that repeats the name of the control it sits in.
//! 5. The default view keeps controls only; `detail: "page"` adds text rows;
//!    `find` keeps rows whose text matches and the control they sit in.
//! 6. Indentation is flattened to at most two levels.
//! 7. The result is capped, ending with how many rows were left out.
//!
//! Controls carry Pilot's own numbers ([`super::numbering`]).

use super::numbering::{keys_in_order, ControlKey, Numbering};
use super::{clip, MAX_CHANGE_CHARS, MAX_READ_CHARS};

/// One row of the driver's markdown tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub depth: usize,
    /// The driver's row index (`[N]`); rows without one cannot be acted on.
    pub index: Option<u32>,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
    pub enabled: bool,
}

/// Parse the driver's markdown tree (`tree_markdown`).
pub fn parse_tree(markdown: &str) -> Vec<Row> {
    markdown.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Row> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = line.trim_start_matches(' ').strip_prefix("- ")?;
    let (index, rest) = match rest.strip_prefix('[') {
        Some(after) => {
            let close = after.find(']')?;
            let index = after[..close].parse::<u32>().ok()?;
            (Some(index), after[close + 1..].trim_start())
        }
        None => (None, rest),
    };
    let role_end = rest.find([' ', '[']).unwrap_or(rest.len());
    let role = rest[..role_end].to_string();
    if role.is_empty() {
        return None;
    }
    let mut rest = rest[role_end..].trim_start();
    let mut name = String::new();
    if rest.starts_with('"') {
        let (text, after) = quoted(rest)?;
        name = text;
        rest = after.trim_start();
    }
    let attrs = rest.strip_prefix('[').unwrap_or("");
    Some(Row {
        depth: indent / 2,
        index,
        role,
        name: name.trim().to_string(),
        value: attribute(attrs, "value="),
        enabled: !attrs.contains("enabled=false"),
    })
}

/// A double-quoted string with `\"` and `\\` escapes, and what follows it.
fn quoted(text: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    out.push(escaped);
                }
            }
            '"' => return Some((out, &text[at + 1..])),
            _ => out.push(c),
        }
    }
    None
}

fn attribute(attrs: &str, key: &str) -> Option<String> {
    let mut search = attrs;
    while let Some(at) = search.find(key) {
        let before_ok = at == 0 || search.as_bytes()[at - 1] == b' ';
        let after = &search[at + key.len()..];
        if before_ok && after.starts_with('"') {
            return quoted(after).map(|(value, _)| value);
        }
        search = after;
    }
    None
}

/// The model-facing role of a control, or `None` when the row is not one.
fn control_role(row: &Row, inside_dropdown: bool) -> Option<&'static str> {
    Some(match row.role.as_str() {
        "Button" | "SplitButton" => "button",
        "Hyperlink" => "link",
        "Edit" => "field",
        "ComboBox" => "dropdown",
        "CheckBox" => "checkbox",
        "RadioButton" => "radio",
        "MenuItem" => "menu item",
        "TabItem" => "tab",
        "Slider" => "slider",
        "Spinner" => "spinner",
        "ListItem" if inside_dropdown => "option",
        _ => return None,
    })
}

fn is_text_role(role: &str) -> bool {
    matches!(
        role,
        "Text"
            | "Header"
            | "HeaderItem"
            | "DataItem"
            | "Group"
            | "ListItem"
            | "Custom"
            | "StatusBar"
            | "ToolTip"
    )
}

/// Controls the model cannot tell apart are not shown: an unnamed button or
/// link says nothing about what it does. Unnamed fields and drop-downs are
/// kept (a search box often has no label).
fn worth_showing(role: &str, row: &Row) -> bool {
    !row.name.is_empty() || matches!(role, "field" | "dropdown" | "slider" | "spinner")
}

/// Which rows a read shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// Controls only (the default).
    Controls,
    /// Controls and text.
    Page,
    /// Rows whose text matches every word (or, failing that, any word), and
    /// the control each sits in.
    Find(String),
}

/// One compacted read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Read {
    /// The page or window title.
    pub title: String,
    /// The page address, when the window is a browser page.
    pub url: Option<String>,
    /// The lines the model reads, already numbered.
    pub lines: Vec<String>,
    /// Every control on the page with its driver row and identity, shown or
    /// not, for resolving numbers against this read.
    pub controls: Vec<(u32, ControlKey)>,
    /// Rows left out by the length cap.
    pub omitted: usize,
}

impl Read {
    /// The read as text, ending with the left-out count.
    pub fn body(&self) -> String {
        let mut out = self.lines.join("\n");
        if self.omitted > 0 {
            out.push_str(&format!(
                "\n… {} more rows not shown. Use read with find to look for something specific.",
                self.omitted
            ));
        }
        if out.is_empty() {
            out.push_str(
                "(Nothing to act on here yet. The page may still be loading: read again.)",
            );
        }
        out
    }
}

struct Kept {
    row: usize,
    role: Option<&'static str>,
    level: usize,
}

/// Compact a driver read for the model.
pub fn compact(markdown: &str, view: &View, numbering: &mut Numbering) -> Read {
    let rows = parse_tree(markdown);
    let (page, title, url) = page_rows(&rows);

    // Control role of every page row, and its identity for numbering.
    let mut dropdown_depth: Option<usize> = None;
    let roles: Vec<Option<&'static str>> = page
        .iter()
        .map(|&i| {
            let row = &rows[i];
            if dropdown_depth.is_some_and(|depth| row.depth <= depth) {
                dropdown_depth = None;
            }
            let role = control_role(row, dropdown_depth.is_some());
            if row.role == "ComboBox" {
                dropdown_depth = Some(row.depth);
            }
            role.filter(|role| row.index.is_some() && worth_showing(role, row))
        })
        .collect();
    let keys = keys_in_order(
        page.iter()
            .zip(&roles)
            .filter_map(|(&i, role)| role.map(|role| (role, rows[i].name.as_str()))),
    );
    let mut controls = Vec::with_capacity(keys.len());
    let mut key_iter = keys.into_iter();
    let mut key_of_row = vec![None; page.len()];
    for (slot, (&i, role)) in page.iter().zip(&roles).enumerate() {
        if role.is_some() {
            let key = key_iter.next().expect("one key per control");
            controls.push((rows[i].index.unwrap_or_default(), key.clone()));
            key_of_row[slot] = Some(key);
        }
    }

    let find_terms = match view {
        View::Find(query) => Some(terms(query)),
        _ => None,
    };
    let selected = select_rows(&rows, &page, &roles, view, find_terms.as_deref());

    // Flatten: a kept row's level is the number of kept ancestors, at most 2.
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (depth, level)
    let mut kept: Vec<Kept> = Vec::new();
    let mut ancestor_names: Vec<(usize, String)> = Vec::new(); // (depth, name)
    for (slot, &i) in page.iter().enumerate() {
        if !selected[slot] {
            continue;
        }
        let row = &rows[i];
        while stack.last().is_some_and(|(depth, _)| *depth >= row.depth) {
            stack.pop();
        }
        while ancestor_names
            .last()
            .is_some_and(|(depth, _)| *depth >= row.depth)
        {
            ancestor_names.pop();
        }
        // Step 4: a row that repeats the control it sits in adds nothing.
        if roles[slot].is_none()
            && ancestor_names
                .last()
                .is_some_and(|(_, name)| name.eq_ignore_ascii_case(&row.name))
        {
            continue;
        }
        let level = stack
            .last()
            .map(|(_, level)| (level + 1).min(2))
            .unwrap_or(0);
        stack.push((row.depth, level));
        if roles[slot].is_some() {
            ancestor_names.push((row.depth, row.name.clone()));
        }
        kept.push(Kept {
            row: slot,
            role: roles[slot],
            level,
        });
    }

    // Number, format and cap.
    let mut lines = Vec::new();
    let mut used = 0usize;
    let mut omitted = 0usize;
    for item in &kept {
        if used >= MAX_READ_CHARS {
            omitted += 1;
            continue;
        }
        let row = &rows[page[item.row]];
        let indent = "  ".repeat(item.level);
        let line = match (item.role, &key_of_row[item.row]) {
            (Some(role), Some(key)) => {
                let number = numbering.number(key);
                format_control(&indent, number, role, row)
            }
            _ => format!("{indent}· {}", clip(&row.name, 200)),
        };
        used += line.chars().count() + 1;
        lines.push(line);
    }

    Read {
        title,
        url,
        lines,
        controls,
        omitted,
    }
}

fn format_control(indent: &str, number: u32, role: &str, row: &Row) -> String {
    let mut line = format!("{indent}[{number}] {role}");
    if !row.name.is_empty() {
        line.push_str(&format!(" \"{}\"", clip(&row.name, 150)));
    }
    if let Some(value) = shown_value(role, row) {
        line.push_str(&format!(" = \"{}\"", clip(&value, 80)));
    }
    if !row.enabled {
        line.push_str(" (disabled)");
    }
    line
}

/// A field's or drop-down's current value, never an address.
fn shown_value(role: &str, row: &Row) -> Option<String> {
    if !matches!(
        role,
        "field" | "dropdown" | "slider" | "spinner" | "checkbox"
    ) {
        return None;
    }
    let value = row.value.as_deref()?.trim();
    let address = ["http://", "https://", "about:", "chrome:", "file:"]
        .iter()
        .any(|scheme| value.starts_with(scheme));
    (!value.is_empty() && !address && value != row.name).then(|| value.to_string())
}

/// Step 1: the rows of the page, its title and address.
fn page_rows(rows: &[Row]) -> (Vec<usize>, String, Option<String>) {
    let Some(doc) = rows.iter().position(|row| row.role == "Document") else {
        let title = rows.first().map(|row| row.name.clone()).unwrap_or_default();
        return ((0..rows.len()).collect(), title, None);
    };
    let depth = rows[doc].depth;
    let end = rows[doc + 1..]
        .iter()
        .position(|row| row.depth <= depth)
        .map(|offset| doc + 1 + offset)
        .unwrap_or(rows.len());
    let url = rows[doc]
        .value
        .clone()
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"));
    (((doc + 1)..end).collect(), rows[doc].name.clone(), url)
}

fn terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !term.is_empty())
        .collect()
}

fn select_rows(
    rows: &[Row],
    page: &[usize],
    roles: &[Option<&'static str>],
    view: &View,
    find: Option<&[String]>,
) -> Vec<bool> {
    let text_row = |slot: usize| {
        let row = &rows[page[slot]];
        roles[slot].is_none() && !row.name.is_empty() && is_text_role(&row.role)
    };
    match view {
        View::Controls => roles.iter().map(Option::is_some).collect(),
        View::Page => (0..page.len())
            .map(|slot| {
                roles[slot].is_some() || (text_row(slot) && !repeats_children(rows, page, slot))
            })
            .collect(),
        View::Find(_) => {
            let terms = find.unwrap_or(&[]);
            let haystack = |slot: usize| {
                let row = &rows[page[slot]];
                format!("{} {}", row.name, row.value.as_deref().unwrap_or("")).to_lowercase()
            };
            let candidate = |slot: usize| {
                (roles[slot].is_some() || text_row(slot)) && !repeats_children(rows, page, slot)
            };
            let mut hits: Vec<bool> = (0..page.len())
                .map(|slot| {
                    candidate(slot) && terms.iter().all(|term| haystack(slot).contains(term))
                })
                .collect();
            if !hits.iter().any(|hit| *hit) && terms.len() > 1 {
                hits = (0..page.len())
                    .map(|slot| {
                        candidate(slot) && terms.iter().any(|term| haystack(slot).contains(term))
                    })
                    .collect();
            }
            // Keep the control a matched row sits in (an option's drop-down).
            let mut selected = hits.clone();
            for slot in 0..page.len() {
                if !hits[slot] {
                    continue;
                }
                let depth = rows[page[slot]].depth;
                let mut wanted = depth;
                for back in (0..slot).rev() {
                    let row = &rows[page[back]];
                    if row.depth < wanted {
                        wanted = row.depth;
                        if roles[back].is_some() {
                            selected[back] = true;
                            break;
                        }
                    }
                }
            }
            selected
        }
    }
}

/// Step 3: a text row whose name is only its descendants' names joined
/// (a table row or card that spells out every cell) adds nothing.
fn repeats_children(rows: &[Row], page: &[usize], slot: usize) -> bool {
    let row = &rows[page[slot]];
    if row.name.is_empty() {
        return false;
    }
    let mut named = 0;
    for &i in &page[slot + 1..] {
        let child = &rows[i];
        if child.depth <= row.depth {
            break;
        }
        if child.name.is_empty() {
            continue;
        }
        if !row.name.contains(child.name.as_str()) {
            return false;
        }
        named += 1;
    }
    named > 0
}

/// The options of a drop-down (by driver row) in the latest read, and which
/// one is selected now (matched against the drop-down's value).
pub fn dropdown_options(markdown: &str, dropdown_row: u32) -> Option<(Vec<String>, Option<usize>)> {
    let rows = parse_tree(markdown);
    let at = rows
        .iter()
        .position(|row| row.index == Some(dropdown_row) && row.role == "ComboBox")?;
    let depth = rows[at].depth;
    let options: Vec<String> = rows[at + 1..]
        .iter()
        .take_while(|row| row.depth > depth)
        .filter(|row| row.role == "ListItem")
        .map(|row| row.name.clone())
        .collect();
    let current = rows[at]
        .value
        .as_deref()
        .and_then(|value| options.iter().position(|option| option == value));
    Some((options, current))
}

/// What an action changed, comparing the read before and after it.
pub fn changes(before: &Read, after: &Read) -> String {
    if before.url != after.url || before.title != after.title {
        let mut text = format!("The page is now \"{}\"", clip(&after.title, 150));
        if let Some(url) = &after.url {
            text.push_str(&format!(" ({})", host(url)));
        }
        text.push_str(".\n");
        text.push_str(&after.body());
        return text;
    }
    let old: std::collections::HashSet<&str> =
        before.lines.iter().map(|line| line.trim()).collect();
    let new: std::collections::HashSet<&str> = after.lines.iter().map(|line| line.trim()).collect();
    let added: Vec<&String> = after
        .lines
        .iter()
        .filter(|line| !old.contains(line.trim()))
        .collect();
    let removed = before
        .lines
        .iter()
        .filter(|line| !new.contains(line.trim()))
        .count();
    if added.is_empty() && removed == 0 {
        return "Nothing on the page changed yet. If you expected a change, read again in a moment."
            .to_string();
    }
    let mut text = String::new();
    if !added.is_empty() {
        text.push_str("New or changed on the page:\n");
        let mut used = 0;
        let mut left_out = 0;
        for line in &added {
            if used >= MAX_CHANGE_CHARS {
                left_out += 1;
                continue;
            }
            used += line.chars().count() + 1;
            text.push_str(line.trim_start());
            text.push('\n');
        }
        if left_out > 0 {
            text.push_str(&format!(
                "… {left_out} more. Read the page to see everything.\n"
            ));
        }
    }
    if removed > 0 {
        text.push_str(&format!("{removed} rows are gone from the page.\n"));
    }
    text.trim_end().to_string()
}

/// The host of a link, for short replies.
pub fn host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| clip(url, 80))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWTIMES: &str = include_str!("../../tests/fixtures/pilot/bms_showtimes.md");
    const SEATS: &str = include_str!("../../tests/fixtures/pilot/bms_seat_layout.md");
    const ZOMATO: &str = include_str!("../../tests/fixtures/pilot/zomato_booking.md");

    /// Rough token count: about four characters a token.
    fn tokens(read: &Read) -> usize {
        read.body().chars().count() / 4
    }

    fn read(markdown: &str, view: View) -> Read {
        compact(markdown, &view, &mut Numbering::new())
    }

    #[test]
    fn parses_rows_with_escaped_names_and_attributes() {
        let rows = parse_tree(
            "- [0] Window \"A \\\"quoted\\\" title\" [value=\"x\" actions=[invoke]]\n  - Text \"no index\"\n    - [7] Edit \"Username\" [id=username enabled=false actions=[set_value,text]]",
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "A \"quoted\" title");
        assert_eq!(rows[0].value.as_deref(), Some("x"));
        assert_eq!(rows[1].index, None);
        assert_eq!(
            (rows[2].depth, rows[2].index, rows[2].enabled),
            (2, Some(7), false)
        );
    }

    #[test]
    fn browser_rows_and_other_tab_titles_never_reach_the_model() {
        for fixture in [SHOWTIMES, SEATS, ZOMATO] {
            for view in [View::Controls, View::Page, View::Find("private".into())] {
                let body = read(fixture, view).body();
                assert!(!body.contains("Private tab title"), "{body}");
                assert!(!body.contains("urlbar"));
            }
        }
    }

    #[test]
    fn no_addresses_ids_or_action_lists_survive() {
        for fixture in [SHOWTIMES, SEATS, ZOMATO] {
            let body = read(fixture, View::Page).body();
            assert!(!body.contains("https://"), "address leaked");
            assert!(!body.contains("actions="));
            assert!(!body.contains(" id="));
        }
    }

    #[test]
    fn showtimes_page_fits_and_keeps_the_show_buttons() {
        let compacted = read(SHOWTIMES, View::Controls);
        let body = compacted.body();
        assert!(tokens(&compacted) <= 4_000, "{} tokens", tokens(&compacted));
        assert!(body.contains("button \"Book 11:20 AM\""), "{body}");
        assert_eq!(
            compacted.url.as_deref().map(host).as_deref(),
            Some("in.bookmyshow.com")
        );
    }

    #[test]
    fn seat_page_is_tiny_and_keeps_its_controls() {
        let compacted = read(SEATS, View::Controls);
        let body = compacted.body();
        assert!(
            tokens(&compacted) <= 400,
            "{} tokens:\n{body}",
            tokens(&compacted)
        );
        assert!(
            body.contains("\"Open accessibility seat selection\""),
            "{body}"
        );
        assert!(body.contains("\"11:20 AM IMAX\""));
        assert!(body.contains("button \"Select Seats\""), "{body}");
    }

    #[test]
    fn zomato_booking_page_fits() {
        let compacted = read(ZOMATO, View::Controls);
        assert!(tokens(&compacted) <= 1_200, "{} tokens", tokens(&compacted));
    }

    #[test]
    fn find_is_small_and_keeps_matches() {
        for (fixture, query, expect) in [
            (SHOWTIMES, "11:20", "Book 11:20 AM"),
            (SEATS, "accessibility", "Open accessibility seat selection"),
        ] {
            let compacted = read(fixture, View::Find(query.into()));
            let body = compacted.body();
            assert!(tokens(&compacted) <= 600, "{} tokens", tokens(&compacted));
            assert!(body.contains(expect), "{body}");
        }
    }

    #[test]
    fn page_view_adds_text_but_drops_rows_that_repeat_their_cells() {
        let compacted = read(SEATS, View::Page);
        let body = compacted.body();
        assert!(
            body.contains("· Ramayana (IMAX 3D)(Hindi) - Hindi"),
            "{body}"
        );
        let showtimes = read(SHOWTIMES, View::Page).body();
        // The grid row that spells out every show is dropped; the buttons stay.
        assert!(
            !showtimes.contains("· Ramayana (U) Hindi , 2D Book"),
            "grid row kept"
        );
    }

    #[test]
    fn numbers_are_stable_across_reads_and_resolve_to_driver_rows() {
        let mut numbering = Numbering::new();
        let first = compact(SEATS, &View::Controls, &mut numbering);
        let again = compact(SEATS, &View::Find("11:20".into()), &mut numbering);
        let line = |read: &Read| {
            read.lines
                .iter()
                .find(|line| line.contains("11:20 AM IMAX"))
                .cloned()
                .unwrap()
        };
        assert_eq!(line(&first).trim(), line(&again).trim());
        let number: u32 = line(&first).trim()[1..]
            .split(']')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            super::super::numbering::resolve(&numbering, number, &again.controls),
            super::super::numbering::Resolved::Row(125)
        );
    }

    /// Prints raw vs compacted sizes (`cargo test -p grain-core pilot::compact::tests::size_report -- --nocapture`).
    #[test]
    fn size_report() {
        for (name, fixture, query) in [
            ("showtimes", SHOWTIMES, "11:20"),
            ("seat page", SEATS, "accessibility"),
            ("zomato", ZOMATO, "book"),
        ] {
            let raw = fixture.chars().count() / 4;
            let controls = tokens(&read(fixture, View::Controls));
            let page = tokens(&read(fixture, View::Page));
            let find = tokens(&read(fixture, View::Find(query.into())));
            eprintln!(
                "{name}: raw ~{raw} tokens, controls ~{controls}, page ~{page}, find ~{find}"
            );
        }
    }

    #[test]
    fn length_is_capped_with_a_count() {
        let mut markdown =
            String::from("- [0] Document \"Big\" [value=\"https://example.com/\"]\n");
        for i in 1..3_000 {
            markdown.push_str(&format!(
                "  - [{i}] Button \"Button number {i} with a long label\"\n"
            ));
        }
        let compacted = read(&markdown, View::Controls);
        assert!(compacted.omitted > 0);
        assert!(compacted.body().chars().count() <= MAX_READ_CHARS + 300);
        assert!(compacted.body().contains("more rows not shown"));
    }

    #[test]
    fn dropdown_options_and_current_choice() {
        let markdown = "- [0] Document \"x\"\n  - [1] ComboBox \"Select seat category\" [value=\"NORMAL - ₹340.00\"]\n    - [2] List\n      - [3] ListItem \"Select Category\"\n      - [4] ListItem \"NORMAL - ₹340.00\"\n      - [5] ListItem \"EXECUTIVE - ₹380.00\"\n  - [6] Button \"Next\"";
        let (options, current) = dropdown_options(markdown, 1).unwrap();
        assert_eq!(options.len(), 3);
        assert_eq!(current, Some(1));
        let compacted = read(markdown, View::Controls);
        let body = compacted.body();
        assert!(
            body.contains("dropdown \"Select seat category\" = \"NORMAL - ₹340.00\""),
            "{body}"
        );
        assert!(
            body.contains("  [") && body.contains("option \"EXECUTIVE - ₹380.00\""),
            "{body}"
        );
    }

    #[test]
    fn changes_report_new_rows_or_a_new_page() {
        let mut numbering = Numbering::new();
        let before = compact("- [0] Document \"Seats\" [value=\"https://a.example/seat\"]\n  - [1] Button \"Select seat 10 in row D\"", &View::Controls, &mut numbering);
        let same = compact("- [0] Document \"Seats\" [value=\"https://a.example/seat\"]\n  - [1] Button \"Select seat 10 in row D\"", &View::Controls, &mut numbering);
        assert!(changes(&before, &same).starts_with("Nothing on the page changed"));
        let pay = compact("- [0] Document \"Seats\" [value=\"https://a.example/seat\"]\n  - [1] Button \"Select seat 10 in row D\"\n  - [2] Button \"Pay ₹ 340\"", &View::Controls, &mut numbering);
        let text = changes(&before, &pay);
        assert!(text.contains("button \"Pay ₹ 340\""), "{text}");
        let moved = compact("- [0] Document \"Summary\" [value=\"https://a.example/summary\"]\n  - [1] Button \"Accept\"", &View::Controls, &mut numbering);
        assert!(changes(&pay, &moved).starts_with("The page is now \"Summary\" (a.example)"));
    }

    #[test]
    fn a_native_window_without_a_document_keeps_its_rows() {
        let compacted = read("- [0] Window \"Untitled - Notepad\"\n  - [1] Edit \"Text editor\" [value=\"hello\"]\n  - [2] Button \"Close\"", View::Controls);
        assert_eq!(compacted.title, "Untitled - Notepad");
        assert!(compacted
            .body()
            .contains("field \"Text editor\" = \"hello\""));
    }
}
