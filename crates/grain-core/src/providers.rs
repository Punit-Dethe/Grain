//! Stateless AI provider selection and persisted ordering.
use crate::settings::{AppSettings, PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID};
use std::collections::HashSet;

/// A saved credential configures an HTTP provider; local endpoints and Apple
/// Intelligence may run without one once a model is selected.
pub fn is_configured(settings: &AppSettings, provider: &PostProcessProvider) -> bool {
    settings
        .post_process_api_keys
        .get(&provider.id)
        .is_some_and(|key| !key.trim().is_empty())
        || ((provider.id == "custom" || provider.id == APPLE_INTELLIGENCE_PROVIDER_ID)
            && settings
                .post_process_models
                .get(&provider.id)
                .is_some_and(|m| !m.trim().is_empty()))
}

/// Filter without sorting: saved provider order is the fallback order.
pub fn fallback_pool(settings: &AppSettings) -> impl Iterator<Item = &PostProcessProvider> {
    settings.post_process_providers.iter().filter(|p| {
        p.enabled
            && is_configured(settings, p)
            && settings
                .post_process_models
                .get(&p.id)
                .is_some_and(|m| !m.trim().is_empty())
    })
}

/// Reject stale, duplicate or unknown ids before touching the provider records.
pub fn reorder(settings: &mut AppSettings, ids: &[String]) -> Result<(), String> {
    let current: HashSet<&str> = settings
        .post_process_providers
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    let requested: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if ids.len() != settings.post_process_providers.len()
        || current.len() != ids.len()
        || requested != current
    {
        return Err("Provider list changed. Reload and try again.".into());
    }
    let reordered = ids
        .iter()
        .map(|id| {
            settings
                .post_process_providers
                .iter()
                .find(|p| &p.id == id)
                .unwrap()
                .clone()
        })
        .collect();
    settings.post_process_providers = reordered;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn configured() -> AppSettings {
        let mut s = get_default_settings();
        s.post_process_providers.truncate(3);
        for p in &s.post_process_providers {
            s.post_process_models.insert(p.id.clone(), "model".into());
            s.post_process_api_keys.insert(p.id.clone(), "key".into());
        }
        s
    }

    #[test]
    fn fallback_preserves_priority_and_skips_disabled_or_unconfigured_entries() {
        let mut s = configured();
        let ids: Vec<_> = s
            .post_process_providers
            .iter()
            .rev()
            .map(|p| p.id.clone())
            .collect();
        reorder(&mut s, &ids).unwrap();
        assert_eq!(
            fallback_pool(&s).map(|p| p.id.clone()).collect::<Vec<_>>(),
            ids
        );
        s.post_process_providers[0].enabled = false;
        s.post_process_api_keys.0.remove(&ids[1]);
        assert_eq!(
            fallback_pool(&s).map(|p| p.id.clone()).collect::<Vec<_>>(),
            vec![ids[2].clone()]
        );
        s.post_process_models.insert(ids[2].clone(), " ".into());
        assert_eq!(fallback_pool(&s).count(), 0);
    }

    #[test]
    fn invalid_reorders_cannot_drop_duplicate_or_resurrect_providers() {
        let mut s = configured();
        let ids: Vec<_> = s
            .post_process_providers
            .iter()
            .map(|p| p.id.clone())
            .collect();
        for request in [
            vec![],
            vec![ids[0].clone(); 3],
            vec![ids[0].clone(), ids[1].clone(), "unknown".into()],
        ] {
            assert!(reorder(&mut s, &request).is_err());
            assert_eq!(
                s.post_process_providers
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>(),
                ids
            );
        }
    }
}
