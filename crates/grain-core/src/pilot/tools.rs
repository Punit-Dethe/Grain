//! The tools the model sees, and parsing their arguments into a [`Call`].
//!
//! The whole list is budgeted at ~1.5k tokens. Tabs are `t1`, `t2`…; controls
//! are the bracketed numbers from Pilot's reads. Neither is a browser id, so
//! the model cannot name a tab Pilot did not open.

use serde_json::{json, Value};

use super::page::View;
use super::policy;
use super::{clip, MAX_SCROLL_AMOUNT, MAX_STEPS};

/// One Pilot tool, as the Agent catalogue needs it.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub read_only: bool,
    pub schema: Value,
}

fn tab_property() -> Value {
    json!({"type": "string", "description": "Tab like t1. Default: the last one used."})
}

fn control_property() -> Value {
    json!({"type": "integer", "minimum": 1, "description": "Control number from a read, e.g. 12 for [12]."})
}

/// Every tool, in the order the catalogue lists them.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "open",
            title: "Open a page in the user's browser",
            description: "Open an https link in a new tab of the user's own browser, signed in as them, to act on that site. Not for finding information: use web search. Returns the tab (t1…) and a read of what is on screen.",
            keywords: &["website", "url", "visit", "go to", "browse", "book", "order", "web page"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "tabs",
            title: "List Pilot's tabs",
            description: "List the tabs Pilot opened, with their titles.",
            keywords: &["tabs", "which tab", "open tabs"],
            read_only: true,
            schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        Tool {
            name: "read",
            title: "Read a tab",
            description: "Read what is on screen in a tab, with numbered controls. find: only rows of the whole page matching these words (cheapest). detail=page: the whole page.",
            keywords: &["read", "look", "page", "see", "find", "what is on", "buttons", "text"],
            read_only: true,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "find": {"type": "string"},
                    "detail": {"type": "string", "enum": ["screen", "page"]}
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "click",
            title: "Click a control",
            description: "Click a numbered control. Returns what changed.",
            keywords: &["click", "press", "tap", "select", "button", "link"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {"tab": tab_property(), "control": control_property()},
                "required": ["control"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "type",
            title: "Type into a field",
            description: "Put text into a numbered field. submit=true presses Enter after. Never for passwords, codes or card details.",
            keywords: &["type", "fill", "enter text", "search", "write", "field"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "control": control_property(),
                    "text": {"type": "string"},
                    "submit": {"type": "boolean"}
                },
                "required": ["control", "text"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "choose",
            title: "Pick a drop-down option",
            description: "Pick an option in a numbered drop-down by its text.",
            keywords: &["dropdown", "choose", "option", "select", "pick"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "control": control_property(),
                    "option": {"type": "string"}
                },
                "required": ["control", "option"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "press",
            title: "Press a key",
            description: "Press one key in the focused part of a tab.",
            keywords: &["key", "enter", "escape", "tab", "arrow"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "key": {"type": "string", "enum": policy::KEYS}
                },
                "required": ["key"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "scroll",
            title: "Scroll a tab",
            description: "Scroll a tab to reveal more. amount is wheel notches (default 5).",
            keywords: &["scroll", "more", "down", "up", "load more"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "direction": {"type": "string", "enum": ["down", "up", "left", "right"]},
                    "amount": {"type": "integer", "minimum": 1, "maximum": MAX_SCROLL_AMOUNT}
                },
                "required": ["direction"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "steps",
            title: "Several actions at once",
            description: "Run up to 8 click/type/press actions in one tab in order, stopping at the first failure. Returns what changed once.",
            keywords: &["steps", "fill form", "several", "batch"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "steps": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_STEPS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {"type": "string", "enum": ["click", "type", "press"]},
                                "control": {"type": "integer", "minimum": 1},
                                "text": {"type": "string"},
                                "key": {"type": "string", "enum": policy::KEYS}
                            },
                            "required": ["action"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["steps"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "show",
            title: "Hand the tab to the user",
            description: "Bring a tab to the user's screen and stop. reason: sign_in, user_action (pay, accept terms, a code, a CAPTCHA) or view. Use before any payment.",
            keywords: &["sign in", "log in", "pay", "payment", "confirm", "captcha", "show me", "let me see"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "reason": {"type": "string", "enum": ["sign_in", "user_action", "view"]},
                    "message": {"type": "string", "description": "One line for the user."}
                },
                "required": ["reason"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "close",
            title: "Close a tab Pilot opened",
            description: "Close a tab Pilot opened when you are done with it.",
            keywords: &["close", "done", "finished"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {"tab": tab_property()},
                "additionalProperties": false
            }),
        },
    ]
}

/// A tab handle, `t1` → 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(pub u32);

impl Handle {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let digits = raw.strip_prefix(['t', 'T']).unwrap_or(raw);
        digits
            .parse::<u32>()
            .ok()
            .filter(|n| *n >= 1 && digits.len() <= 4)
            .map(Handle)
            .ok_or_else(|| format!("`{}` is not a tab; use one like t1", clip(raw, 40)))
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "t{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowReason {
    SignIn,
    UserAction,
    View,
}

/// One action inside `steps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Click(u32),
    Type(u32, String),
    Press(&'static str),
}

/// One validated call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Open(String),
    Tabs,
    Read {
        tab: Option<Handle>,
        view: View,
    },
    Click {
        tab: Option<Handle>,
        control: u32,
    },
    Type {
        tab: Option<Handle>,
        control: u32,
        text: String,
        submit: bool,
    },
    Choose {
        tab: Option<Handle>,
        control: u32,
        option: String,
    },
    Press {
        tab: Option<Handle>,
        key: &'static str,
    },
    Scroll {
        tab: Option<Handle>,
        direction: &'static str,
        amount: u32,
    },
    Steps {
        tab: Option<Handle>,
        steps: Vec<Step>,
    },
    Show {
        tab: Option<Handle>,
        reason: ShowReason,
        message: Option<String>,
    },
    Close {
        tab: Option<Handle>,
    },
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    text(args, key).ok_or_else(|| format!("`{key}` is required"))
}

fn tab(args: &Value) -> Result<Option<Handle>, String> {
    match text(args, "tab").map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => Handle::parse(raw).map(Some),
    }
}

/// A control number: 12, "12" or "[12]".
fn control(args: &Value) -> Result<u32, String> {
    let value = args.get("control").ok_or("`control` is required")?;
    let number = match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().trim_matches(['[', ']']).parse::<u64>().ok(),
        _ => None,
    };
    number
        .filter(|n| (1..=u32::MAX as u64).contains(n))
        .map(|n| n as u32)
        .ok_or_else(|| "`control` must be a control number from a read, like 12".into())
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Validate the model's arguments for `action`.
pub fn parse(action: &str, args: &Value) -> Result<Call, String> {
    match action {
        "open" => Ok(Call::Open(policy::web_link(required(args, "url")?)?)),
        "tabs" => Ok(Call::Tabs),
        "read" => {
            let find = text(args, "find").map(str::trim).filter(|f| !f.is_empty());
            let view = match (find, text(args, "detail")) {
                (Some(find), _) => View::Find(policy::short_field(find, "find")?),
                (None, None | Some("screen")) => View::Visible,
                (None, Some("page")) => View::Page,
                (None, Some(other)) => {
                    return Err(format!("`{}` is not screen or page", clip(other, 40)))
                }
            };
            Ok(Call::Read {
                tab: tab(args)?,
                view,
            })
        }
        "click" => Ok(Call::Click {
            tab: tab(args)?,
            control: control(args)?,
        }),
        "type" => Ok(Call::Type {
            tab: tab(args)?,
            control: control(args)?,
            text: policy::typed_text(required(args, "text")?)?,
            submit: bool_arg(args, "submit"),
        }),
        "choose" => Ok(Call::Choose {
            tab: tab(args)?,
            control: control(args)?,
            option: policy::short_field(required(args, "option")?, "the option")?,
        }),
        "press" => Ok(Call::Press {
            tab: tab(args)?,
            key: policy::key(required(args, "key")?)?,
        }),
        "scroll" => {
            let direction = match required(args, "direction")? {
                "down" => "down",
                "up" => "up",
                "left" => "left",
                "right" => "right",
                other => {
                    return Err(format!(
                        "`{}` is not down, up, left or right",
                        clip(other, 40)
                    ))
                }
            };
            let amount = match args.get("amount") {
                None => 5,
                Some(value) => value
                    .as_u64()
                    .filter(|n| (1..=MAX_SCROLL_AMOUNT as u64).contains(n))
                    .ok_or_else(|| format!("`amount` must be 1 to {MAX_SCROLL_AMOUNT}"))?
                    as u32,
            };
            Ok(Call::Scroll {
                tab: tab(args)?,
                direction,
                amount,
            })
        }
        "steps" => {
            let items = args
                .get("steps")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty())
                .ok_or("`steps` must be a list of actions")?;
            if items.len() > MAX_STEPS {
                return Err(format!("at most {MAX_STEPS} steps at a time"));
            }
            let steps = items
                .iter()
                .enumerate()
                .map(|(at, step)| parse_step(step).map_err(|e| format!("step {}: {e}", at + 1)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Call::Steps {
                tab: tab(args)?,
                steps,
            })
        }
        "show" => Ok(Call::Show {
            tab: tab(args)?,
            reason: match required(args, "reason")? {
                "sign_in" => ShowReason::SignIn,
                "user_action" => ShowReason::UserAction,
                "view" => ShowReason::View,
                other => {
                    return Err(format!(
                        "`{}` is not sign_in, user_action or view",
                        clip(other, 40)
                    ))
                }
            },
            message: match text(args, "message").map(str::trim) {
                None | Some("") => None,
                Some(message) => Some(policy::show_message(message)?),
            },
        }),
        "close" => Ok(Call::Close { tab: tab(args)? }),
        other => Err(format!("Grain Pilot has no action `{}`", clip(other, 40))),
    }
}

fn parse_step(step: &Value) -> Result<Step, String> {
    Ok(match required(step, "action")? {
        "click" => Step::Click(control(step)?),
        "type" => Step::Type(control(step)?, policy::typed_text(required(step, "text")?)?),
        "press" => Step::Press(policy::key(required(step, "key")?)?),
        other => return Err(format!("`{}` is not click, type or press", clip(other, 40))),
    })
}

impl Call {
    /// Whether the call only reads.
    pub fn read_only(&self) -> bool {
        matches!(self, Call::Tabs | Call::Read { .. })
    }

    /// The tab the call names, if any.
    pub fn tab(&self) -> Option<Handle> {
        match self {
            Call::Open(_) | Call::Tabs => None,
            Call::Read { tab, .. }
            | Call::Click { tab, .. }
            | Call::Type { tab, .. }
            | Call::Choose { tab, .. }
            | Call::Press { tab, .. }
            | Call::Scroll { tab, .. }
            | Call::Steps { tab, .. }
            | Call::Show { tab, .. }
            | Call::Close { tab } => *tab,
        }
    }

    /// Log-safe description: the action and its shape, never typed text or a
    /// full address.
    pub fn describe_for_log(&self) -> String {
        match self {
            Call::Open(url) => format!("open host={}", super::page::host(url)),
            Call::Tabs => "tabs".into(),
            Call::Read { view, .. } => match view {
                View::Visible => "read".into(),
                View::Page => "read page".into(),
                View::Find(_) => "read find".into(),
            },
            Call::Click { control, .. } => format!("click {control}"),
            Call::Type {
                control,
                text,
                submit,
                ..
            } => {
                format!(
                    "type {control} chars={} submit={submit}",
                    text.chars().count()
                )
            }
            Call::Choose { control, .. } => format!("choose {control}"),
            Call::Press { key, .. } => format!("press {key}"),
            Call::Scroll {
                direction, amount, ..
            } => format!("scroll {direction} {amount}"),
            Call::Steps { steps, .. } => format!("steps {}", steps.len()),
            Call::Show { reason, .. } => format!("show {reason:?}"),
            Call::Close { .. } => "close".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_closed_schema_and_a_parse_arm() {
        let tools = tools();
        assert_eq!(tools.len(), 11);
        for tool in &tools {
            assert_eq!(tool.schema["type"], "object", "{}", tool.name);
            assert_eq!(tool.schema["additionalProperties"], false, "{}", tool.name);
            let error = parse(tool.name, &json!({})).err().unwrap_or_default();
            assert!(
                !error.contains("has no action"),
                "{} has no parse arm",
                tool.name
            );
        }
    }

    #[test]
    fn the_tool_list_fits_its_token_budget() {
        let text: String = tools()
            .iter()
            .map(|tool| format!("{}{}{}", tool.name, tool.description, tool.schema))
            .collect();
        // About four characters a token; budget 1.5k tokens.
        assert!(text.len() / 4 <= 1_500, "{} tokens", text.len() / 4);
    }

    #[test]
    fn read_only_tools_match_read_only_calls() {
        for tool in tools() {
            let sample = match tool.name {
                "open" => json!({"url": "https://example.com"}),
                "click" => json!({"control": 3}),
                "type" => json!({"control": 3, "text": "x"}),
                "choose" => json!({"control": 3, "option": "NORMAL"}),
                "press" => json!({"key": "return"}),
                "scroll" => json!({"direction": "down"}),
                "steps" => json!({"steps": [{"action": "click", "control": 1}]}),
                "show" => json!({"reason": "view"}),
                _ => json!({}),
            };
            let call = parse(tool.name, &sample).unwrap_or_else(|e| panic!("{}: {e}", tool.name));
            assert_eq!(call.read_only(), tool.read_only, "{}", tool.name);
        }
    }

    #[test]
    fn handles_and_control_numbers() {
        assert_eq!(Handle::parse("t3"), Ok(Handle(3)));
        assert_eq!(Handle::parse("2"), Ok(Handle(2)));
        assert!(Handle::parse("t0").is_err());
        assert!(Handle::parse("132036x").is_err());
        assert_eq!(Handle(4).to_string(), "t4");
        for raw in [json!(12), json!("12"), json!("[12]")] {
            let call = parse("click", &json!({"control": raw})).unwrap();
            assert_eq!(
                call,
                Call::Click {
                    tab: None,
                    control: 12
                }
            );
        }
        assert!(parse("click", &json!({"control": 0})).is_err());
        assert!(parse("click", &json!({"control": "s0001:12"})).is_err());
    }

    #[test]
    fn read_views() {
        let read = |args| parse("read", &args).unwrap();
        assert_eq!(
            read(json!({})),
            Call::Read {
                tab: None,
                view: View::Visible
            }
        );
        assert_eq!(
            read(json!({"detail": "page", "tab": "t2"})),
            Call::Read {
                tab: Some(Handle(2)),
                view: View::Page
            }
        );
        assert_eq!(
            read(json!({"find": " 11:20 "})),
            Call::Read {
                tab: None,
                view: View::Find("11:20".into())
            }
        );
        assert!(parse("read", &json!({"detail": "everything"})).is_err());
    }

    #[test]
    fn refusals_happen_at_parse_time() {
        assert!(parse("open", &json!({"url": "file:///C:/x"})).is_err());
        assert!(parse("open_app", &json!({"name": "notepad"})).is_err());
        assert!(parse("press", &json!({"key": "ctrl+w"})).is_err());
        assert!(parse("scroll", &json!({"direction": "down", "amount": 500})).is_err());
        assert!(parse("type", &json!({"control": 1, "text": "a\u{1b}[2J"})).is_err());
        let too_many: Vec<Value> = (0..9)
            .map(|_| json!({"action": "press", "key": "tab"}))
            .collect();
        assert!(parse("steps", &json!({"steps": too_many})).is_err());
        let error = parse(
            "steps",
            &json!({"steps": [{"action": "press", "key": "tab"}, {"action": "drag"}]}),
        )
        .unwrap_err();
        assert!(error.starts_with("step 2"), "{error}");
    }

    #[test]
    fn log_lines_never_carry_typed_text_or_full_addresses() {
        let typed = parse("type", &json!({"control": 4, "text": "my secret note"})).unwrap();
        assert_eq!(typed.describe_for_log(), "type 4 chars=14 submit=false");
        let open = parse(
            "open",
            &json!({"url": "https://in.bookmyshow.com/buytickets/CPVM?x=1"}),
        )
        .unwrap();
        assert_eq!(open.describe_for_log(), "open host=in.bookmyshow.com");
    }
}
