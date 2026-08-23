//! Persistent, privacy-preserving Extension Mode routing feedback.
//!
//! Only two bounded counters per extension are stored. Requests/transcripts and
//! reasons never touch disk. Repeated explicit declines gently reduce topical
//! ranking; named matches remain exact user intent and are never penalised.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use grain_core::recommend::{Recommendation, Signal, TOPICAL_FLOOR};
use grain_core::AppContext;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const SCHEMA: u32 = 1;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_EXTENSIONS: usize = 512;
const MAX_DECISIONS: u32 = 100;
const MIN_DECISIONS: u32 = 5;
const MIN_DECLINES: u32 = 3;
const MAX_PENALTY: f32 = 0.12;
static IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct Stats {
    decisions: u32,
    declines: u32,
}

#[derive(Deserialize, Serialize)]
struct Misroutes {
    schema: u32,
    extensions: BTreeMap<String, Stats>,
}

impl Default for Misroutes {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            extensions: BTreeMap::new(),
        }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.try_state::<std::sync::Arc<AppContext>>()
        .map(|ctx| ctx.data_dir.join("extensions").join("misroutes.json"))
}

fn load(app: &AppHandle) -> Misroutes {
    use std::io::Read;

    let Some(path) = path(app) else {
        return Misroutes::default();
    };
    let Ok(file) = std::fs::File::open(path) else {
        return Misroutes::default();
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_FILE_BYTES
    {
        return Misroutes::default();
    }
    let Ok(mut data) = serde_json::from_slice::<Misroutes>(&bytes) else {
        return Misroutes::default();
    };
    if data.schema != SCHEMA {
        return Misroutes::default();
    }
    data.extensions.retain(|id, stats| {
        grain_sdk::validate_extension_id(id).is_ok()
            && stats.declines <= stats.decisions
            && stats.decisions <= MAX_DECISIONS
    });
    while data.extensions.len() > MAX_EXTENSIONS {
        let Some(id) = data.extensions.keys().next_back().cloned() else {
            break;
        };
        data.extensions.remove(&id);
    }
    data
}

fn save(app: &AppHandle, data: &Misroutes) -> Result<(), String> {
    use std::io::Write;

    let path = path(app).ok_or("app context unavailable")?;
    let parent = path.parent().ok_or("misroute path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(data).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("misroute counters exceeded their storage bound".into());
    }
    let temp = parent.join(format!(".misroutes-{}.tmp", uuid::Uuid::new_v4().simple()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            error.to_string()
        })?;
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

fn record(app: &AppHandle, id: &str, declined: bool) {
    if grain_sdk::validate_extension_id(id).is_err() {
        return;
    }
    let _guard = IO_LOCK.lock().unwrap();
    let mut data = load(app);
    if !data.extensions.contains_key(id) && data.extensions.len() >= MAX_EXTENSIONS {
        if let Some(victim) = data.extensions.keys().next().cloned() {
            data.extensions.remove(&victim);
        }
    }
    let stats = data.extensions.entry(id.to_string()).or_default();
    if stats.decisions >= MAX_DECISIONS {
        stats.decisions /= 2;
        stats.declines /= 2;
    }
    stats.decisions += 1;
    if declined {
        stats.declines += 1;
    }
    if let Err(error) = save(app, &data) {
        log::warn!("[GRAIN] extension mode: could not save misroute counters: {error}");
    }
}

pub fn record_decline(app: &AppHandle, id: &str) {
    record(app, id, true);
}

pub fn record_accepted_route(app: &AppHandle, id: &str) {
    record(app, id, false);
}

fn penalty(stats: Stats) -> f32 {
    if stats.decisions < MIN_DECISIONS || stats.declines < MIN_DECLINES {
        return 0.0;
    }
    let rate = stats.declines as f32 / stats.decisions as f32;
    ((rate - 0.4).max(0.0) * 0.2).min(MAX_PENALTY)
}

/// Adjust and re-sort one ranked set. Exact name matches retain their score and
/// precedence; only the semantic/topical leg learns from explicit declines.
pub fn apply(app: &AppHandle, ranked: &mut Vec<Recommendation>) {
    let _guard = IO_LOCK.lock().unwrap();
    let data = load(app);
    for recommendation in ranked.iter_mut() {
        if recommendation.signal == Signal::Topical {
            let amount = data
                .extensions
                .get(&recommendation.extension_id)
                .copied()
                .map(penalty)
                .unwrap_or_default();
            recommendation.score = (recommendation.score - amount).max(0.0);
        }
    }
    ranked.retain(|recommendation| {
        recommendation.signal == Signal::Named || recommendation.score >= TOPICAL_FLOOR
    });
    ranked.sort_by(|left, right| {
        signal_rank(right.signal)
            .cmp(&signal_rank(left.signal))
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.extension_id.cmp(&right.extension_id))
    });
    for index in 0..ranked.len() {
        let next = ranked[index + 1..]
            .iter()
            .find(|candidate| candidate.signal == ranked[index].signal);
        ranked[index].margin = next.map_or(ranked[index].score, |candidate| {
            ranked[index].score - candidate.score
        });
    }
}

fn signal_rank(signal: Signal) -> u8 {
    match signal {
        Signal::Named => 2,
        Signal::Topical => 1,
    }
}

pub fn purge(app: &AppHandle, id: &str) {
    if grain_sdk::validate_extension_id(id).is_err() {
        return;
    }
    let _guard = IO_LOCK.lock().unwrap();
    let mut data = load(app);
    if data.extensions.remove(id).is_some() {
        let _ = save(app, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_counter_file_uses_the_current_schema() {
        assert_eq!(Misroutes::default().schema, SCHEMA);
    }

    #[test]
    fn a_small_sample_never_changes_ranking() {
        assert_eq!(
            penalty(Stats {
                decisions: 4,
                declines: 4,
            }),
            0.0
        );
    }

    #[test]
    fn repeated_declines_are_bounded_and_adaptive() {
        let severe = penalty(Stats {
            decisions: 10,
            declines: 10,
        });
        let mixed = penalty(Stats {
            decisions: 10,
            declines: 6,
        });
        assert_eq!(severe, MAX_PENALTY);
        assert!(mixed > 0.0 && mixed < severe);
    }
}
