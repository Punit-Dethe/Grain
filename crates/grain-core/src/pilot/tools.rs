//! The tools the model sees, and parsing their arguments into a [`Call`].
//!
//! Five tools, budgeted at ~600 tokens, because the list is re-sent on every
//! model round. Every page action goes through one `act` list. Tabs are `t1`,
//! `t2`…; controls are the bracketed numbers from Pilot's reads. Neither is a
//! browser id, so the model cannot name a tab Pilot did not open.

use serde_json::{json, Value};

use super::page::View;
use super::policy;
use super::{clip, MAX_STEPS};

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
    json!({"type": "string"})
}

/// Every tool, in the order the catalogue lists them.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "open",
            title: "Open a page in the user's browser",
            description: "Open an https link in a new tab of the user's own browser, signed in as them, to do something there (book, order, fill a form). Not for looking things up: use web search. Returns the tab (t1…) and what is on screen.",
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
            name: "read",
            title: "Read a tab",
            description: "Read a tab (default: the last one used). Rows of what is on screen; [n] is a control. find: only rows of the whole page with any of these words (cheapest). page: the whole page.",
            keywords: &["read", "look", "page", "see", "find", "what is on", "buttons", "text"],
            read_only: true,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "find": {"type": "string"},
                    "page": {"type": "boolean"}
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "act",
            title: "Act in a tab",
            description: "Do up to 8 steps in a tab, in order; stops at the first failure. Returns what changed. A step is one of {click:n} {fill:n,text,enter?} {choose:n,option} {key:\"enter\"} {scroll:\"down\"}, n from a read. Never fill passwords, codes or card details.",
            keywords: &[
                "click", "press", "tap", "select", "button", "link", "type", "fill", "enter text",
                "search", "field", "dropdown", "choose", "option", "key", "scroll", "form",
            ],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "do": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_STEPS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "click": {"type": "integer"},
                                "fill": {"type": "integer"},
                                "text": {"type": "string"},
                                "enter": {"type": "boolean"},
                                "choose": {"type": "integer"},
                                "option": {"type": "string"},
                                "key": {"type": "string"},
                                "scroll": {"type": "string", "enum": ["down", "up", "left", "right"]}
                            },
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["do"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "show",
            title: "Hand the tab to the user",
            description: "Bring a tab in front of the user and stop. reason: sign_in, user_action (pay, accept terms, a code, a CAPTCHA) or view. Always before paying.",
            keywords: &["sign in", "log in", "pay", "payment", "confirm", "captcha", "show me", "let me see"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "reason": {"type": "string", "enum": ["sign_in", "user_action", "view"]},
                    "message": {"type": "string"}
                },
                "required": ["reason"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "tabs",
            title: "Pilot's tabs",
            description: "List Pilot's tabs. close: a tab to close when done with it.",
            keywords: &["tabs", "which tab", "open tabs", "close", "done", "finished"],
            read_only: true,
            schema: json!({
                "type": "object",
                "properties": {"close": tab_property()},
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

/// One step of `act`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Click(u32),
    Fill {
        control: u32,
        text: String,
        enter: bool,
    },
    Choose {
        control: u32,
        option: String,
    },
    Key(&'static str),
    Scroll(&'static str),
}

/// One validated call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Open(String),
    Read {
        tab: Option<Handle>,
        view: View,
    },
    Act {
        tab: Option<Handle>,
        steps: Vec<Step>,
    },
    Show {
        tab: Option<Handle>,
        reason: ShowReason,
        message: Option<String>,
    },
    Tabs {
        close: Option<Handle>,
    },
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    text(args, key).ok_or_else(|| format!("`{key}` is required"))
}

fn handle(args: &Value, key: &str) -> Result<Option<Handle>, String> {
    match text(args, key).map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => Handle::parse(raw).map(Some),
    }
}

/// A control number: 12, "12" or "[12]".
fn control(value: &Value) -> Result<u32, String> {
    let number = match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().trim_matches(['[', ']']).parse::<u64>().ok(),
        _ => None,
    };
    number
        .filter(|n| (1..=u32::MAX as u64).contains(n))
        .map(|n| n as u32)
        .ok_or_else(|| "a control must be a number from a read, like 12".into())
}

/// Validate the model's arguments for `action`.
pub fn parse(action: &str, args: &Value) -> Result<Call, String> {
    match action {
        "open" => Ok(Call::Open(policy::web_link(required(args, "url")?)?)),
        "read" => {
            let find = text(args, "find").map(str::trim).filter(|f| !f.is_empty());
            let whole = args.get("page").and_then(Value::as_bool).unwrap_or(false);
            let view = match (find, whole) {
                (Some(find), _) => View::Find(policy::short_field(find, "find")?),
                (None, true) => View::Page,
                (None, false) => View::Visible,
            };
            Ok(Call::Read {
                tab: handle(args, "tab")?,
                view,
            })
        }
        "act" => {
            let items = args
                .get("do")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty())
                .ok_or("`do` must be a list of steps")?;
            if items.len() > MAX_STEPS {
                return Err(format!("at most {MAX_STEPS} steps at a time"));
            }
            let steps = items
                .iter()
                .enumerate()
                .map(|(at, step)| parse_step(step).map_err(|e| format!("step {}: {e}", at + 1)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Call::Act {
                tab: handle(args, "tab")?,
                steps,
            })
        }
        "show" => Ok(Call::Show {
            tab: handle(args, "tab")?,
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
        "tabs" => Ok(Call::Tabs {
            close: handle(args, "close")?,
        }),
        other => Err(format!("Grain Pilot has no action `{}`", clip(other, 40))),
    }
}

/// One `act` step: exactly one of click, fill, choose, key or scroll.
fn parse_step(step: &Value) -> Result<Step, String> {
    let object = step.as_object().ok_or("a step must be an object")?;
    let verbs: Vec<&str> = ["click", "fill", "choose", "key", "scroll"]
        .into_iter()
        .filter(|verb| object.contains_key(*verb))
        .collect();
    let [verb] = verbs[..] else {
        return Err("give exactly one of click, fill, choose, key or scroll".into());
    };
    let value = &object[verb];
    Ok(match verb {
        "click" => Step::Click(control(value)?),
        "fill" => Step::Fill {
            control: control(value)?,
            text: policy::typed_text(required(step, "text")?)?,
            enter: step.get("enter").and_then(Value::as_bool).unwrap_or(false),
        },
        "choose" => Step::Choose {
            control: control(value)?,
            option: policy::short_field(required(step, "option")?, "the option")?,
        },
        "key" => Step::Key(policy::key(value.as_str().unwrap_or(""))?),
        _ => Step::Scroll(match value.as_str().unwrap_or("") {
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
        }),
    })
}

impl Step {
    /// What the step did, for a reply; never the typed text.
    pub fn done(&self) -> String {
        match self {
            Step::Click(n) => format!("Clicked [{n}]."),
            Step::Fill { control, .. } => format!("Typed into [{control}]."),
            Step::Choose { control, option } => format!("Picked \"{option}\" in [{control}]."),
            Step::Key(key) => format!("Pressed {key}."),
            Step::Scroll(direction) => format!("Scrolled {direction}."),
        }
    }

    /// The control the step acts on, if any.
    pub fn control(&self) -> Option<u32> {
        match self {
            Step::Click(n) | Step::Fill { control: n, .. } | Step::Choose { control: n, .. } => {
                Some(*n)
            }
            Step::Key(_) | Step::Scroll(_) => None,
        }
    }

    fn describe_for_log(&self) -> String {
        match self {
            Step::Click(n) => format!("click {n}"),
            Step::Fill {
                control,
                text,
                enter,
            } => format!(
                "fill {control} chars={} enter={enter}",
                text.chars().count()
            ),
            Step::Choose { control, .. } => format!("choose {control}"),
            Step::Key(key) => format!("key {key}"),
            Step::Scroll(direction) => format!("scroll {direction}"),
        }
    }
}

impl Call {
    /// Whether the call only reads.
    pub fn read_only(&self) -> bool {
        matches!(self, Call::Read { .. } | Call::Tabs { close: None })
    }

    /// Log-safe description: the action and its shape, never typed text or a
    /// full address.
    pub fn describe_for_log(&self) -> String {
        match self {
            Call::Open(url) => format!("open host={}", super::page::host(url)),
            Call::Read { view, .. } => match view {
                View::Visible => "read".into(),
                View::Page => "read page".into(),
                View::Find(_) => "read find".into(),
            },
            Call::Act { steps, .. } => format!(
                "act {}",
                steps
                    .iter()
                    .map(Step::describe_for_log)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Call::Show { reason, .. } => format!("show {reason:?}"),
            Call::Tabs { close } => match close {
                Some(handle) => format!("tabs close {handle}"),
                None => "tabs".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_closed_schema_and_a_parse_arm() {
        let tools = tools();
        assert_eq!(tools.len(), 5);
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
        // About four characters a token; budget 600 tokens.
        println!(
            "tool list: ~{} tokens ({} chars)",
            text.len() / 4,
            text.len()
        );
        assert!(text.len() / 4 <= 600, "{} tokens", text.len() / 4);
    }

    #[test]
    fn read_only_tools_match_read_only_calls() {
        for tool in tools() {
            let sample = match tool.name {
                "open" => json!({"url": "https://example.com"}),
                "act" => json!({"do": [{"click": 1}]}),
                "show" => json!({"reason": "view"}),
                _ => json!({}),
            };
            let call = parse(tool.name, &sample).unwrap_or_else(|e| panic!("{}: {e}", tool.name));
            assert_eq!(call.read_only(), tool.read_only, "{}", tool.name);
        }
        assert!(!parse("tabs", &json!({"close": "t2"})).unwrap().read_only());
    }

    #[test]
    fn handles_and_control_numbers() {
        assert_eq!(Handle::parse("t3"), Ok(Handle(3)));
        assert_eq!(Handle::parse("2"), Ok(Handle(2)));
        assert!(Handle::parse("t0").is_err());
        assert!(Handle::parse("132036x").is_err());
        assert_eq!(Handle(4).to_string(), "t4");
        for raw in [json!(12), json!("12"), json!("[12]")] {
            let call = parse("act", &json!({"do": [{"click": raw}]})).unwrap();
            assert_eq!(
                call,
                Call::Act {
                    tab: None,
                    steps: vec![Step::Click(12)]
                }
            );
        }
        assert!(parse("act", &json!({"do": [{"click": 0}]})).is_err());
        assert!(parse("act", &json!({"do": [{"click": "s0001:12"}]})).is_err());
    }

    #[test]
    fn step_reports_never_carry_typed_text() {
        let fill = Step::Fill {
            control: 2,
            text: "hunter2".into(),
            enter: true,
        };
        assert_eq!(fill.done(), "Typed into [2].");
        assert_eq!(Step::Click(25).done(), "Clicked [25].");
        let choose = Step::Choose {
            control: 12,
            option: "Two".into(),
        };
        assert_eq!(choose.done(), "Picked \"Two\" in [12].");
    }

    #[test]
    fn act_steps_parse_each_verb() {
        let call = parse(
            "act",
            &json!({"tab": "t2", "do": [
                {"fill": 4, "text": "Pune", "enter": true},
                {"choose": 7, "option": "NORMAL"},
                {"key": "Escape"},
                {"scroll": "down"},
                {"click": "[9]"}
            ]}),
        )
        .unwrap();
        assert_eq!(
            call,
            Call::Act {
                tab: Some(Handle(2)),
                steps: vec![
                    Step::Fill {
                        control: 4,
                        text: "Pune".into(),
                        enter: true
                    },
                    Step::Choose {
                        control: 7,
                        option: "NORMAL".into()
                    },
                    Step::Key("escape"),
                    Step::Scroll("down"),
                    Step::Click(9),
                ]
            }
        );
        // Exactly one verb per step.
        assert!(parse(
            "act",
            &json!({"do": [{"click": 1, "fill": 2, "text": "x"}]})
        )
        .is_err());
        assert!(parse("act", &json!({"do": [{"text": "x"}]})).is_err());
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
            read(json!({"page": true, "tab": "t2"})),
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
    }

    #[test]
    fn refusals_happen_at_parse_time() {
        assert!(parse("open", &json!({"url": "file:///C:/x"})).is_err());
        assert!(parse("click", &json!({"control": 1})).is_err());
        assert!(parse("act", &json!({"do": [{"key": "ctrl+w"}]})).is_err());
        assert!(parse("act", &json!({"do": [{"scroll": "sideways"}]})).is_err());
        assert!(parse("act", &json!({"do": [{"fill": 1, "text": "a\u{1b}[2J"}]})).is_err());
        let too_many: Vec<Value> = (0..9).map(|_| json!({"key": "tab"})).collect();
        assert!(parse("act", &json!({"do": too_many})).is_err());
        let error = parse("act", &json!({"do": [{"key": "tab"}, {"drag": 3}]})).unwrap_err();
        assert!(error.starts_with("step 2"), "{error}");
    }

    #[test]
    fn log_lines_never_carry_typed_text_or_full_addresses() {
        let typed = parse(
            "act",
            &json!({"do": [{"fill": 4, "text": "my secret note"}]}),
        )
        .unwrap();
        assert_eq!(typed.describe_for_log(), "act fill 4 chars=14 enter=false");
        let open = parse(
            "open",
            &json!({"url": "https://in.bookmyshow.com/buytickets/CPVM?x=1"}),
        )
        .unwrap();
        assert_eq!(open.describe_for_log(), "open host=in.bookmyshow.com");
    }
}
