//! Host-rendered extension views.
//!
//! Extensions control a small, serializable component tree; Grain owns every
//! rendered element, style, focus rule, window affordance, and trusted action
//! boundary. This module is intentionally data-only so the same validation runs
//! at every boundary without pulling renderer code into the SDK leaf.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

pub const VIEW_SCHEMA_VERSION: u16 = 1;
pub const VIEW_MAX_PAYLOAD_BYTES: usize = 128 * 1024;
pub const VIEW_MAX_EVENT_BYTES: usize = 64 * 1024;
pub const VIEW_MAX_NODES: usize = 128;
pub const VIEW_MAX_DEPTH: usize = 8;
pub const VIEW_MAX_CHILDREN: usize = 32;
pub const VIEW_MAX_FIELDS: usize = 32;
pub const VIEW_MAX_ACTIONS: usize = 4;
pub const VIEW_MAX_OPTIONS_PER_SELECT: usize = 50;
pub const VIEW_MAX_TOTAL_OPTIONS: usize = 128;
pub const VIEW_MAX_TOTAL_TEXT_BYTES: usize = 48 * 1024;

fn schema_version() -> u16 {
    VIEW_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionView {
    #[serde(default = "schema_version")]
    pub version: u16,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub root: ViewNode,
    #[serde(default)]
    pub actions: Vec<ViewAction>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ViewNode {
    Stack {
        #[serde(default)]
        gap: ViewGap,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Inline {
        #[serde(default)]
        gap: ViewGap,
        #[serde(default)]
        align: ViewAlign,
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Grid {
        columns: u8,
        #[serde(default)]
        gap: ViewGap,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Section {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Divider,
    Heading {
        text: String,
        #[serde(default)]
        level: ViewHeadingLevel,
    },
    Text {
        text: String,
        #[serde(default)]
        tone: ViewTone,
    },
    Badge {
        text: String,
        #[serde(default)]
        tone: ViewTone,
    },
    Metadata {
        label: String,
        value: String,
    },
    TextField {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        disabled: bool,
        #[serde(rename = "maxLength")]
        #[serde(default)]
        max_length: Option<u16>,
    },
    TextArea {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default = "default_textarea_rows")]
        rows: u8,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        disabled: bool,
        #[serde(rename = "maxLength")]
        #[serde(default)]
        max_length: Option<u16>,
    },
    Select {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
        options: Vec<ViewOption>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        disabled: bool,
    },
    Checkbox {
        id: String,
        label: String,
        #[serde(default)]
        checked: bool,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        disabled: bool,
    },
}

fn default_textarea_rows() -> u8 {
    5
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewGap {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewHeadingLevel {
    One,
    #[default]
    Two,
    Three,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewTone {
    #[default]
    Neutral,
    Muted,
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields)]
pub struct ViewOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewAction {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub intent: ViewActionIntent,
    #[serde(default)]
    pub kind: ViewActionKind,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewActionIntent {
    #[default]
    Primary,
    Secondary,
    Danger,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ViewActionKind {
    #[default]
    Submit,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum ViewValue {
    Text(String),
    Checked(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionViewEvent {
    Change {
        target: String,
        value: ViewValue,
        #[serde(default)]
        values: BTreeMap<String, ViewValue>,
    },
    Submit {
        target: String,
        #[serde(default)]
        values: BTreeMap<String, ViewValue>,
    },
    Cancel,
}

#[derive(Clone, Copy)]
enum FieldKind {
    Text { max: usize },
    Select,
    Checkbox,
}

struct FieldSpec {
    kind: FieldKind,
    required: bool,
    disabled: bool,
    options: HashSet<String>,
}

#[derive(Default)]
struct Validation {
    nodes: usize,
    fields: HashMap<String, FieldSpec>,
    actions: HashMap<String, (ViewActionKind, bool)>,
    ids: HashSet<String>,
    text_bytes: usize,
    options: usize,
}

impl Validation {
    fn text(&mut self, label: &str, text: &str, max: usize) -> Result<(), String> {
        if text.len() > max {
            return Err(format!("{label} exceeds the {max}-byte limit"));
        }
        self.text_bytes = self.text_bytes.saturating_add(text.len());
        if self.text_bytes > VIEW_MAX_TOTAL_TEXT_BYTES {
            return Err(format!(
                "view text exceeds the {VIEW_MAX_TOTAL_TEXT_BYTES}-byte total limit"
            ));
        }
        Ok(())
    }

    fn id(&mut self, id: &str) -> Result<(), String> {
        if !valid_id(id) {
            return Err(format!(
                "'{id}' is not a valid stable id (1-64 ASCII letters, digits, '.', '_', ':', '-')"
            ));
        }
        if !self.ids.insert(id.to_string()) {
            return Err(format!("duplicate interactive id '{id}'"));
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    id.len() <= 64
        && first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
}

fn validated_text_limit(
    requested: Option<u16>,
    default: usize,
    ceiling: usize,
    id: &str,
) -> Result<usize, String> {
    let limit = requested.map_or(default, usize::from);
    if !(1..=ceiling).contains(&limit) {
        return Err(format!(
            "field '{id}' maxLength must be between 1 and {ceiling}"
        ));
    }
    Ok(limit)
}

impl ExtensionView {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VIEW_SCHEMA_VERSION {
            return Err(format!(
                "unsupported view schema version {} (host supports {VIEW_SCHEMA_VERSION})",
                self.version
            ));
        }
        let payload = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if payload.len() > VIEW_MAX_PAYLOAD_BYTES {
            return Err(format!(
                "view payload exceeds the {VIEW_MAX_PAYLOAD_BYTES}-byte limit"
            ));
        }

        let mut state = Validation::default();
        state.text("view title", &self.title, 120)?;
        if self.title.trim().is_empty() {
            return Err("view title must not be empty".into());
        }
        if let Some(description) = &self.description {
            state.text("view description", description, 320)?;
        }
        validate_node(&self.root, 1, &mut state)?;

        if self.actions.len() > VIEW_MAX_ACTIONS {
            return Err(format!("a view accepts at most {VIEW_MAX_ACTIONS} actions"));
        }
        let mut primary = 0;
        for action in &self.actions {
            state.id(&action.id)?;
            state.text("action label", &action.label, 48)?;
            if action.label.trim().is_empty() {
                return Err(format!("action '{}' has an empty label", action.id));
            }
            if action.intent == ViewActionIntent::Primary {
                primary += 1;
            }
            if (action.intent == ViewActionIntent::Cancel)
                != (action.kind == ViewActionKind::Cancel)
            {
                return Err(format!(
                    "action '{}' must use cancel intent and kind together",
                    action.id
                ));
            }
            state
                .actions
                .insert(action.id.clone(), (action.kind, action.disabled));
        }
        if primary > 1 {
            return Err("a view may expose only one primary action".into());
        }
        Ok(())
    }

    pub fn validate_event(&self, event: &ExtensionViewEvent) -> Result<(), String> {
        let encoded = serde_json::to_vec(event).map_err(|error| error.to_string())?;
        if encoded.len() > VIEW_MAX_EVENT_BYTES {
            return Err(format!(
                "view event exceeds the {VIEW_MAX_EVENT_BYTES}-byte limit"
            ));
        }
        let mut state = Validation::default();
        validate_node(&self.root, 1, &mut state)?;
        for action in &self.actions {
            state
                .actions
                .insert(action.id.clone(), (action.kind, action.disabled));
        }

        match event {
            ExtensionViewEvent::Change {
                target,
                value,
                values,
            } => {
                let field = state
                    .fields
                    .get(target)
                    .ok_or_else(|| format!("unknown field target '{target}'"))?;
                if field.disabled {
                    return Err(format!("field '{target}' is disabled"));
                }
                validate_value(target, value, field, false)?;
                validate_values(values, &state.fields, false)
            }
            ExtensionViewEvent::Submit { target, values } => {
                match state.actions.get(target) {
                    Some((ViewActionKind::Submit, false)) => {}
                    Some((_, true)) => return Err(format!("action '{target}' is disabled")),
                    Some((ViewActionKind::Cancel, _)) => {
                        return Err(format!("action '{target}' is not a submit action"));
                    }
                    None => return Err(format!("unknown action target '{target}'")),
                }
                validate_values(values, &state.fields, true)
            }
            ExtensionViewEvent::Cancel => Ok(()),
        }
    }
}

fn validate_node(node: &ViewNode, depth: usize, state: &mut Validation) -> Result<(), String> {
    if depth > VIEW_MAX_DEPTH {
        return Err(format!("view tree exceeds maximum depth {VIEW_MAX_DEPTH}"));
    }
    state.nodes += 1;
    if state.nodes > VIEW_MAX_NODES {
        return Err(format!("view tree exceeds maximum {VIEW_MAX_NODES} nodes"));
    }

    match node {
        ViewNode::Stack { children, .. }
        | ViewNode::Inline { children, .. }
        | ViewNode::Section { children, .. } => validate_children(children, depth, state),
        ViewNode::Grid {
            columns, children, ..
        } => {
            if !(1..=3).contains(columns) {
                return Err("grid columns must be between 1 and 3".into());
            }
            validate_children(children, depth, state)
        }
        ViewNode::Divider => Ok(()),
        ViewNode::Heading { text, .. } => state.text("heading", text, 160),
        ViewNode::Text { text, .. } => state.text("text", text, 8192),
        ViewNode::Badge { text, .. } => state.text("badge", text, 64),
        ViewNode::Metadata { label, value } => {
            state.text("metadata label", label, 64)?;
            state.text("metadata value", value, 2048)
        }
        ViewNode::TextField {
            id,
            label,
            value,
            placeholder,
            required,
            disabled,
            max_length,
        } => {
            state.id(id)?;
            state.text("field label", label, 96)?;
            if label.trim().is_empty() {
                return Err(format!("field '{id}' has an empty label"));
            }
            if let Some(placeholder) = placeholder {
                state.text("field placeholder", placeholder, 160)?;
            }
            let max = validated_text_limit(*max_length, 1024, 4096, id)?;
            state.text("field value", value, VIEW_MAX_TOTAL_TEXT_BYTES)?;
            if value.chars().count() > max {
                return Err(format!("field '{id}' exceeds its {max}-character limit"));
            }
            if *required && *disabled && value.trim().is_empty() {
                return Err(format!("required disabled field '{id}' has no value"));
            }
            insert_field(
                state,
                id,
                FieldSpec {
                    kind: FieldKind::Text { max },
                    required: *required,
                    disabled: *disabled,
                    options: HashSet::new(),
                },
            )
        }
        ViewNode::TextArea {
            id,
            label,
            value,
            placeholder,
            rows,
            required,
            disabled,
            max_length,
        } => {
            state.id(id)?;
            if !(2..=12).contains(rows) {
                return Err("text area rows must be between 2 and 12".into());
            }
            state.text("field label", label, 96)?;
            if label.trim().is_empty() {
                return Err(format!("field '{id}' has an empty label"));
            }
            if let Some(placeholder) = placeholder {
                state.text("field placeholder", placeholder, 160)?;
            }
            let max = validated_text_limit(*max_length, 8192, 32768, id)?;
            state.text("field value", value, VIEW_MAX_TOTAL_TEXT_BYTES)?;
            if value.chars().count() > max {
                return Err(format!("field '{id}' exceeds its {max}-character limit"));
            }
            if *required && *disabled && value.trim().is_empty() {
                return Err(format!("required disabled field '{id}' has no value"));
            }
            insert_field(
                state,
                id,
                FieldSpec {
                    kind: FieldKind::Text { max },
                    required: *required,
                    disabled: *disabled,
                    options: HashSet::new(),
                },
            )
        }
        ViewNode::Select {
            id,
            label,
            value,
            options,
            required,
            disabled,
        } => {
            state.id(id)?;
            state.text("field label", label, 96)?;
            if label.trim().is_empty() {
                return Err(format!("field '{id}' has an empty label"));
            }
            if options.is_empty() || options.len() > VIEW_MAX_OPTIONS_PER_SELECT {
                return Err(format!(
                    "select '{id}' must contain 1-{VIEW_MAX_OPTIONS_PER_SELECT} options"
                ));
            }
            state.options += options.len();
            if state.options > VIEW_MAX_TOTAL_OPTIONS {
                return Err(format!(
                    "view exceeds the {VIEW_MAX_TOTAL_OPTIONS}-option total limit"
                ));
            }
            let mut allowed = HashSet::with_capacity(options.len());
            for option in options {
                state.text("select option value", &option.value, 128)?;
                state.text("select option label", &option.label, 128)?;
                if option.value.is_empty()
                    || option.label.trim().is_empty()
                    || !allowed.insert(option.value.clone())
                {
                    return Err(format!(
                        "select '{id}' has an empty label or empty/duplicate option value"
                    ));
                }
            }
            if !value.is_empty() && !allowed.contains(value) {
                return Err(format!("select '{id}' value is not one of its options"));
            }
            if *required && *disabled && value.is_empty() {
                return Err(format!("required disabled field '{id}' has no value"));
            }
            insert_field(
                state,
                id,
                FieldSpec {
                    kind: FieldKind::Select,
                    required: *required,
                    disabled: *disabled,
                    options: allowed,
                },
            )
        }
        ViewNode::Checkbox {
            id,
            label,
            checked,
            required,
            disabled,
        } => {
            state.id(id)?;
            state.text("field label", label, 160)?;
            if label.trim().is_empty() {
                return Err(format!("field '{id}' has an empty label"));
            }
            if *required && *disabled && !checked {
                return Err(format!("required disabled field '{id}' is not checked"));
            }
            insert_field(
                state,
                id,
                FieldSpec {
                    kind: FieldKind::Checkbox,
                    required: *required,
                    disabled: *disabled,
                    options: HashSet::new(),
                },
            )
        }
    }
}

fn validate_children(
    children: &[ViewNode],
    depth: usize,
    state: &mut Validation,
) -> Result<(), String> {
    if children.len() > VIEW_MAX_CHILDREN {
        return Err(format!(
            "a container accepts at most {VIEW_MAX_CHILDREN} children"
        ));
    }
    for child in children {
        validate_node(child, depth + 1, state)?;
    }
    Ok(())
}

fn insert_field(state: &mut Validation, id: &str, field: FieldSpec) -> Result<(), String> {
    if state.fields.len() >= VIEW_MAX_FIELDS {
        return Err(format!("a view accepts at most {VIEW_MAX_FIELDS} fields"));
    }
    state.fields.insert(id.to_string(), field);
    Ok(())
}

fn validate_values(
    values: &BTreeMap<String, ViewValue>,
    fields: &HashMap<String, FieldSpec>,
    enforce_required: bool,
) -> Result<(), String> {
    if values.len() > VIEW_MAX_FIELDS {
        return Err(format!("an event accepts at most {VIEW_MAX_FIELDS} values"));
    }
    for (id, value) in values {
        let field = fields
            .get(id)
            .ok_or_else(|| format!("unknown field value '{id}'"))?;
        validate_value(id, value, field, enforce_required)?;
    }
    if enforce_required {
        for (id, field) in fields.iter().filter(|(_, field)| field.required) {
            let Some(value) = values.get(id) else {
                return Err(format!("required field '{id}' is missing"));
            };
            validate_value(id, value, field, true)?;
        }
    }
    Ok(())
}

fn validate_value(
    id: &str,
    value: &ViewValue,
    field: &FieldSpec,
    enforce_required: bool,
) -> Result<(), String> {
    match (field.kind, value) {
        (FieldKind::Text { max }, ViewValue::Text(text)) => {
            if text.chars().count() > max {
                return Err(format!("field '{id}' exceeds its {max}-character limit"));
            }
            if enforce_required && field.required && text.trim().is_empty() {
                return Err(format!("required field '{id}' is empty"));
            }
            Ok(())
        }
        (FieldKind::Select, ViewValue::Text(selected)) => {
            if !selected.is_empty() && !field.options.contains(selected) {
                return Err(format!("field '{id}' contains an unknown option"));
            }
            if enforce_required && field.required && selected.is_empty() {
                return Err(format!("required field '{id}' is empty"));
            }
            Ok(())
        }
        (FieldKind::Checkbox, ViewValue::Checked(checked)) => {
            if enforce_required && field.required && !checked {
                return Err(format!("required field '{id}' is not checked"));
            }
            Ok(())
        }
        _ => Err(format!("field '{id}' received the wrong value type")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> ExtensionView {
        ExtensionView {
            version: VIEW_SCHEMA_VERSION,
            title: "Create issue".into(),
            description: Some("Review the draft before GitHub receives it.".into()),
            root: ViewNode::Stack {
                gap: ViewGap::Md,
                children: vec![
                    ViewNode::TextField {
                        id: "issue-title".into(),
                        label: "Title".into(),
                        value: "Audio switches lose decoder state".into(),
                        placeholder: None,
                        required: true,
                        disabled: false,
                        max_length: Some(256),
                    },
                    ViewNode::Select {
                        id: "priority".into(),
                        label: "Priority".into(),
                        value: "high".into(),
                        options: vec![ViewOption {
                            value: "high".into(),
                            label: "High".into(),
                        }],
                        required: true,
                        disabled: false,
                    },
                ],
            },
            actions: vec![ViewAction {
                id: "create".into(),
                label: "Create issue".into(),
                intent: ViewActionIntent::Primary,
                kind: ViewActionKind::Submit,
                disabled: false,
            }],
        }
    }

    #[test]
    fn a_bounded_editable_view_and_submit_are_valid() {
        let view = view();
        view.validate().unwrap();
        view.validate_event(&ExtensionViewEvent::Submit {
            target: "create".into(),
            values: BTreeMap::from([
                ("issue-title".into(), ViewValue::Text("Fixed title".into())),
                ("priority".into(), ViewValue::Text("high".into())),
            ]),
        })
        .unwrap();
    }

    #[test]
    fn duplicate_ids_unknown_options_and_unknown_actions_fail_closed() {
        let mut duplicate = view();
        duplicate.actions[0].id = "issue-title".into();
        assert!(duplicate.validate().unwrap_err().contains("duplicate"));

        let view = view();
        let bad_option = ExtensionViewEvent::Submit {
            target: "create".into(),
            values: BTreeMap::from([
                ("issue-title".into(), ViewValue::Text("Title".into())),
                ("priority".into(), ViewValue::Text("impossible".into())),
            ]),
        };
        assert!(view.validate_event(&bad_option).is_err());
        assert!(view
            .validate_event(&ExtensionViewEvent::Submit {
                target: "forged".into(),
                values: BTreeMap::new(),
            })
            .is_err());
    }

    #[test]
    fn depth_and_payload_limits_are_enforced() {
        let mut node = ViewNode::Text {
            text: "bottom".into(),
            tone: ViewTone::Neutral,
        };
        for _ in 0..VIEW_MAX_DEPTH {
            node = ViewNode::Stack {
                gap: ViewGap::Md,
                children: vec![node],
            };
        }
        let mut deep = view();
        deep.root = node;
        assert!(deep.validate().unwrap_err().contains("depth"));

        let mut large = view();
        large.description = Some("x".repeat(321));
        assert!(large.validate().unwrap_err().contains("320-byte"));
    }

    #[test]
    fn impossible_fields_and_ambiguous_cancel_actions_are_rejected() {
        let mut impossible = view();
        if let ViewNode::Stack { children, .. } = &mut impossible.root {
            if let ViewNode::TextField {
                value,
                required,
                disabled,
                ..
            } = &mut children[0]
            {
                value.clear();
                *required = true;
                *disabled = true;
            }
        }
        assert!(impossible
            .validate()
            .unwrap_err()
            .contains("required disabled"));

        let mut bad_limit = view();
        if let ViewNode::Stack { children, .. } = &mut bad_limit.root {
            if let ViewNode::TextField { max_length, .. } = &mut children[0] {
                *max_length = Some(0);
            }
        }
        assert!(bad_limit.validate().unwrap_err().contains("maxLength"));

        let mut ambiguous_cancel = view();
        ambiguous_cancel.actions[0].kind = ViewActionKind::Cancel;
        assert!(ambiguous_cancel
            .validate()
            .unwrap_err()
            .contains("cancel intent and kind together"));
    }

    #[test]
    fn nested_wire_fields_use_the_author_facing_camel_case_contract() {
        let encoded = serde_json::to_value(view()).unwrap();
        let field = &encoded["root"]["children"][0];
        assert_eq!(field["maxLength"], serde_json::json!(256));
        assert!(field.get("max_length").is_none());
        serde_json::from_value::<ExtensionView>(encoded).unwrap();
    }
}
