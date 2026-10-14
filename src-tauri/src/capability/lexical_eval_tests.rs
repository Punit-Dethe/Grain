//! Opt-in catalogue experiment: run production projection/ranking/publication.
//! Inputs and output are explicit ignored evidence, not a second test engine.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    id: String,
    split: String,
    query: String,
    requires: Vec<Vec<String>>,
}

#[test]
#[ignore = "requires explicit exported catalogue and labelled request files"]
fn exported_catalogue_required_sets_under_schema_budget() {
    let catalog_path = std::env::var("GRAIN_LEXICAL_CATALOG").expect("catalogue path");
    let cases_path = std::env::var("GRAIN_LEXICAL_CASES").expect("cases path");
    let report_path = std::env::var("GRAIN_LEXICAL_REPORT").expect("report path");
    let export: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog_path).unwrap()).unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(cases_path).unwrap()).unwrap();
    assert!(!cases.is_empty());
    let mut catalogs = Vec::new();
    let mut directory = Vec::new();
    for catalog in export["result"]["catalogs"].as_array().unwrap() {
        let id = catalog["extension_id"].as_str().unwrap().to_string();
        let tools: Vec<rmcp::model::Tool> =
            serde_json::from_value(catalog["tools"].clone()).unwrap();
        if tools.is_empty() {
            continue;
        }
        let name = catalog["metadata"][0]["provider_name"].as_str().unwrap();
        let entries: Vec<_> = tools
            .iter()
            .map(|tool| {
                let action = mcp_action_input(&id, name, tool);
                let selected = SessionAction {
                    spec: ToolSpec {
                        name: capability_agent::tool_name(&action.canonical_id),
                        description: capability_agent::sanitize(
                            tool.description.as_deref().unwrap_or(&action.title),
                            1200,
                        ),
                        parameters: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                    },
                    origin: ActionOrigin::Mcp {
                        provider_id: id.clone(),
                        provider_name: name.into(),
                        tool_name: tool.name.to_string(),
                    },
                };
                (action, selected)
            })
            .collect();
        directory.push(grain_core::capability_index::ExtensionDirectoryEntry {
            extension_id: id.clone(),
            name: name.into(),
            description: String::new(),
            action_count: entries.len(),
        });
        catalogs.push((id, entries));
    }
    assert!(!catalogs.is_empty());
    let known: HashSet<_> = catalogs
        .iter()
        .flat_map(|(_, entries)| {
            entries
                .iter()
                .map(|(action, _)| action.canonical_id.clone())
        })
        .collect();
    let resolve = |label: &String| {
        if known.contains(label) {
            return label.clone();
        }
        let candidates: Vec<_> = catalogs
            .iter()
            .flat_map(|(_, entries)| entries)
            .filter(|(action, _)| action.action_id == *label)
            .collect();
        assert_eq!(
            candidates.len(),
            1,
            "absent/ambiguous tool label {label}; use a full canonical ID"
        );
        candidates[0].0.canonical_id.clone()
    };
    let mut rows = Vec::new();
    for k in [4, 6, 8] {
        for case in &cases {
            let started = Instant::now();
            let mut session = empty_session(&directory, &[]);
            for (id, entries) in &catalogs {
                remember_catalog(
                    &mut session,
                    id.clone(),
                    Catalog {
                        digest: "offline-fixture".into(),
                        entries: entries.clone(),
                    },
                )
                .unwrap();
            }
            let ids = ranked_ids(&mut session, &[case.query.clone()], None, false);
            let selection = publish_ranked_with_budget(&mut session, ids, k, INITIAL_SCHEMA_BYTES);
            let canonical: Vec<_> = session.actions.keys().cloned().collect();
            let offered: Vec<_> = canonical
                .iter()
                .map(|id| id.rsplit_once(':').unwrap().1.to_string())
                .collect();
            let requires: Vec<Vec<_>> = case
                .requires
                .iter()
                .map(|group| group.iter().map(&resolve).collect())
                .collect();
            let coverage =
                grain_core::capability_eval::required_set_coverage(&canonical, &requires).unwrap();
            let bytes = specs(&session).iter().map(schema_bytes).sum::<usize>();
            assert!(bytes <= capability_agent::MAX_SCHEMA_BYTES);
            rows.push(serde_json::json!({"case":case.id,"split":case.split,"k":k,"required":case.requires,"required_canonical":requires,"offered":offered,"offered_canonical":canonical,"coverage":coverage,"schema_bytes":bytes,"first_turn_micros":started.elapsed().as_micros(),"selection":selection}));
        }
    }
    std::fs::write(report_path, serde_json::to_vec_pretty(&serde_json::json!({"scope":"offline actual supported catalogue; no model/dispatch", "rows":rows})).unwrap()).unwrap();
}
