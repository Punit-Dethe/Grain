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
            let initial_micros = started.elapsed().as_micros();
            let prior = specs(&session);
            let hint = deferred_tool_context(&session);
            let directory: serde_json::Value = hint
                .as_ref()
                .map(|text| serde_json::from_str(text.split_once('\n').unwrap().1).unwrap())
                .unwrap_or_else(|| serde_json::json!({"extensions":[]}));
            let visible: HashSet<String> = directory["extensions"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|extension| {
                    extension["tools"].as_array().unwrap().iter().map(|tool| {
                        format!(
                            "{}:{}",
                            extension["extension_id"].as_str().unwrap(),
                            tool["id"].as_str().unwrap()
                        )
                    })
                })
                .collect();
            // An oracle chooses known required names: this proves host availability
            // and schema packing, NOT that an LLM selects the correct tool.
            let queries: Vec<_> = requires
                .iter()
                .filter(|group| !group.iter().any(|id| canonical.contains(id)))
                .filter_map(|group| group.iter().find(|id| visible.contains(*id)).cloned())
                .collect();
            let mut recovery = Vec::new();
            for batch in queries.chunks(4) {
                let request = capability_agent::parse_search_request(
                    &serde_json::json!({"queries":batch}).to_string(),
                )
                .unwrap();
                let ids = ranked_ids(&mut session, &request.queries, None, false);
                recovery.push(publish_ranked(&mut session, ids, TOOL_PAGE_SIZE));
            }
            let recovered: Vec<_> = session.actions.keys().cloned().collect();
            let recovery_coverage =
                grain_core::capability_eval::required_set_coverage(&recovered, &requires).unwrap();
            let next = specs(&session);
            for spec in prior {
                assert!(next
                    .iter()
                    .any(|next| next.name == spec.name && next.parameters == spec.parameters));
            }
            assert!(
                next.iter().map(schema_bytes).sum::<usize>() <= capability_agent::MAX_SCHEMA_BYTES
            );
            rows.push(serde_json::json!({"case":case.id,"split":case.split,"k":k,"required":case.requires,"required_canonical":requires,"offered":offered,"offered_canonical":canonical,"coverage":coverage,"schema_bytes":bytes,"first_turn_micros":initial_micros,"selection":selection,"hint_bytes":hint.as_ref().map_or(0,String::len),"hint_names":visible.len(),"recovery_queries":queries,"recovery":recovery,"coverage_after_exact_recovery":recovery_coverage}));
        }
    }
    std::fs::write(report_path, serde_json::to_vec_pretty(&serde_json::json!({"scope":"offline actual supported catalogue; oracle-selected exact recovery; no model/dispatch", "rows":rows})).unwrap()).unwrap();
}
