//! [GRAIN] The entity graph (KNOWLEDGE-ARCHITECTURE-PLAN.md D4).
//!
//! LightRAG's contribution is that a knowledge graph is affordable when you
//! never rebuild it — new documents merge in by set union
//! (`V̂ᵗᵒᵗᵃˡ = V̂ ∪ V̂'`) rather than triggering a global re-index. Its other
//! contribution is easy to miss: **the graph is not a model.** No neural
//! network, no GPU, no resident process. It is three ordinary SQLite tables
//! walked by ordinary SQL, which is why this whole layer costs 0 MB of idle RAM
//! and works on a machine that never downloaded the embedding model.
//!
//! The set-union merge is one `ON CONFLICT DO UPDATE`. `entities.norm UNIQUE`
//! **is** LightRAG's `Dedupe`.
//!
//! Two kinds of connection, deliberately:
//!
//! - **Co-mention** — two entities named in the same note. NOT stored as edges:
//!   `note_entities` already encodes it, and a join is cheaper than the O(n²)
//!   edge rows materializing it would cost. This is the neighbourhood LightRAG's
//!   one-hop retrieval actually walks, and because it derives from frontmatter it
//!   survives a full rebuild from the user's files.
//! - **Typed relations** — the LLM's `{from, pred, to}` triples. Stored in
//!   `edges`, higher-value but NOT in the note file, so a rebuild-from-files
//!   loses the predicates and keeps the neighbourhood. That is the intended
//!   degradation: the graph gets thinner, never wrong.
//!
//! Nothing here is a dependency of retrieval. With no LLM configured there are
//! no entities, every query falls back to the lexical and vector legs, and the
//! feature behaves exactly as it did before this module existed.

use anyhow::Result;
use rusqlite::{params, Connection};

use super::capture::{entity_norm, Relation};

/// Ceiling on how many entities one walk may visit. Bounds the cost of a
/// pathological note that names twelve entities each appearing once, and keeps
/// the graph leg's contribution to a query at "a couple of indexed queries".
const MAX_WALK_ENTITIES: usize = 64;

/// Create the graph tables. Called from the same `open_index` that creates
/// `notes_meta`/`notes_fts`, so the graph lives in the SAME per-backend index
/// file — switching vaults can never mix two corpora's entities, and the
/// existing "rebuild index" recovery already covers it.
pub fn ensure_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entities (
            id      INTEGER PRIMARY KEY,
            norm    TEXT NOT NULL UNIQUE,
            name    TEXT NOT NULL,
            kind    TEXT NOT NULL DEFAULT 'topic',
            seen    INTEGER NOT NULL DEFAULT 0,
            last_ms INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS note_entities (
            note_id   TEXT NOT NULL,
            entity_id INTEGER NOT NULL,
            PRIMARY KEY (note_id, entity_id)
        ) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS note_entities_by_entity
            ON note_entities(entity_id);
        CREATE TABLE IF NOT EXISTS edges (
            src     INTEGER NOT NULL,
            dst     INTEGER NOT NULL,
            pred    TEXT NOT NULL,
            weight  INTEGER NOT NULL DEFAULT 0,
            last_ms INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (src, dst, pred)
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Insert-or-merge one entity, returning its row id. `seen` accumulates mentions
/// — it is the cheapest possible confidence signal and what orders the walk, so
/// a name that keeps coming up outranks a one-off.
///
/// The stored `name` is the FIRST spelling we saw: later mentions bump the count
/// without rewriting the display name, so a note's own capitalisation wins and
/// the graph doesn't flip-flop between "JWT" and "jwt".
fn upsert_entity(conn: &Connection, name: &str, kind: &str, now_ms: i64) -> Result<i64> {
    let norm = entity_norm(name);
    if norm.is_empty() {
        return Ok(0);
    }
    conn.execute(
        "INSERT INTO entities (norm, name, kind, seen, last_ms)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(norm) DO UPDATE SET
            seen = seen + 1,
            last_ms = MAX(last_ms, excluded.last_ms),
            -- A later capture may know the kind when the first didn't.
            kind = CASE WHEN entities.kind = 'topic' THEN excluded.kind ELSE entities.kind END",
        params![norm, name.trim(), kind, now_ms],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM entities WHERE norm = ?1",
        params![norm],
        |r| r.get(0),
    )?)
}

/// Point a note at the entities it names. Called from the index write choke
/// point, so this runs for a fresh capture AND for a rebuild-from-files — the
/// neighbourhood is always derivable from what is on disk.
///
/// Replaces the note's rows wholesale: an edit that drops an entity must drop
/// the link, or retrieval keeps walking into a note that no longer mentions it.
pub fn set_note_entities(
    conn: &Connection,
    note_id: &str,
    names: &[String],
    now_ms: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM note_entities WHERE note_id = ?1",
        params![note_id],
    )?;
    for name in names {
        let id = upsert_entity(conn, name, "topic", now_ms)?;
        if id == 0 {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO note_entities (note_id, entity_id) VALUES (?1, ?2)",
            params![note_id, id],
        )?;
    }
    Ok(())
}

/// Record the LLM's typed relations. `weight` accumulates so a relation stated
/// repeatedly across notes outranks one mentioned once — the ordering the walk
/// uses. Both endpoints are already guaranteed to be entities the note recorded
/// (validated in `capture::clean_relations`), so this cannot create a dangling
/// edge.
pub fn record_relations(conn: &Connection, relations: &[Relation], now_ms: i64) -> Result<()> {
    for rel in relations {
        let src = upsert_entity(conn, &rel.from, "topic", now_ms)?;
        let dst = upsert_entity(conn, &rel.to, "topic", now_ms)?;
        if src == 0 || dst == 0 || src == dst {
            continue;
        }
        conn.execute(
            "INSERT INTO edges (src, dst, pred, weight, last_ms)
             VALUES (?1, ?2, ?3, 1, ?4)
             ON CONFLICT(src, dst, pred) DO UPDATE SET
                weight = weight + 1,
                last_ms = MAX(last_ms, excluded.last_ms)",
            params![src, dst, rel.pred, now_ms],
        )?;
    }
    Ok(())
}

/// Forget a note's links. Entity rows themselves are LEFT ALONE: `seen` is a
/// corpus-level count, and decrementing it on delete would make the graph's
/// ordering depend on deletion history. Orphaned entities are harmless — they
/// match nothing, because every lookup goes through `note_entities`.
pub fn forget_note(conn: &Connection, note_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM note_entities WHERE note_id = ?1",
        params![note_id],
    )?;
    Ok(())
}

/// Wipe the graph — part of the index rebuild, which then repopulates the
/// neighbourhood from the notes' frontmatter.
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute_batch("DELETE FROM note_entities; DELETE FROM edges; DELETE FROM entities;")?;
    Ok(())
}

/// One retrieval hit from the graph leg, in walk order.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphHit {
    pub note_id: String,
    /// True when the note mentions an entity the QUERY named directly
    /// (LightRAG's low-level `k^(l)`); false when it was reached by walking
    /// (`k^(g)`). The caller keeps the two as separate ranked lists so fusion
    /// can weigh a direct hit above a neighbour.
    pub direct: bool,
}

/// Entities whose name matches one of the query's terms. LightRAG's low-level
/// keyword step, done as exact-norm and prefix matching rather than by asking a
/// model — the query's own words are already the keywords.
fn seed_entities(conn: &Connection, terms: &[String]) -> Result<Vec<i64>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stmt = conn.prepare(
        // Exact norm first, then a prefix match, so "auth" finds "auth.py"
        // without "a" dragging in the whole corpus (terms are pre-filtered).
        "SELECT id FROM entities
         WHERE norm = ?1 OR norm LIKE ?1 || '%'
         ORDER BY (norm = ?1) DESC, seen DESC
         LIMIT 8",
    )?;
    for term in terms {
        let norm = entity_norm(term);
        // A one/two-character term prefix-matches half the graph; skip it and
        // let the lexical leg handle short tokens.
        if norm.len() < 3 {
            continue;
        }
        let rows = stmt.query_map(params![norm], |r| r.get::<_, i64>(0))?;
        for id in rows {
            let id = id?;
            if seen.insert(id) {
                ids.push(id);
            }
        }
        if ids.len() >= MAX_WALK_ENTITIES {
            break;
        }
    }
    Ok(ids)
}

/// The dual-level graph walk (D5). Returns `(direct, neighbour)` note-id lists,
/// each already ordered best-first, or two empty lists when the query names
/// nothing in the graph — which is the common case and must be cheap.
///
/// - **Low level (`k^(l)`)**: notes mentioning a seed entity.
/// - **High level (`k^(g)`)**: notes mentioning the seeds' NEIGHBOURHOOD —
///   entities co-mentioned with a seed, plus anything a typed edge points at.
///   This is the leg that connects two notes sharing no vocabulary, which is the
///   one case GraphRAG-Bench finds graphs decisively win.
///
/// `hops` is capped at 2; anything deeper is noise at personal scale and turns
/// the walk into a corpus scan.
pub fn walk(conn: &Connection, terms: &[String], limit: usize) -> Result<Vec<GraphHit>> {
    let seeds = seed_entities(conn, terms)?;
    if seeds.is_empty() {
        return Ok(Vec::new());
    }
    let seed_list = id_list(&seeds);

    // Low level: notes naming a seed, most-connected seed first, then recency.
    let direct: Vec<String> = {
        let sql = format!(
            "SELECT ne.note_id
               FROM note_entities ne
               JOIN entities e ON e.id = ne.entity_id
              WHERE ne.entity_id IN ({seed_list})
              GROUP BY ne.note_id
              ORDER BY COUNT(*) DESC, MAX(e.seen) DESC
              LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // Hop 1: the seeds' neighbourhood — co-mentioned entities (free from
    // note_entities) plus typed-edge targets in both directions.
    let neighbours: Vec<i64> = {
        let sql = format!(
            "SELECT id, w FROM (
                SELECT ne2.entity_id AS id, COUNT(*) AS w
                  FROM note_entities ne1
                  JOIN note_entities ne2 ON ne2.note_id = ne1.note_id
                 WHERE ne1.entity_id IN ({seed_list})
                   AND ne2.entity_id NOT IN ({seed_list})
                 GROUP BY ne2.entity_id
                UNION ALL
                SELECT CASE WHEN src IN ({seed_list}) THEN dst ELSE src END AS id,
                       weight * 2 AS w
                  FROM edges
                 WHERE src IN ({seed_list}) OR dst IN ({seed_list})
             )
             GROUP BY id ORDER BY SUM(w) DESC LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![MAX_WALK_ENTITIES as i64], |r| r.get::<_, i64>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut hits: Vec<GraphHit> = direct
        .into_iter()
        .map(|note_id| GraphHit {
            note_id,
            direct: true,
        })
        .collect();

    if !neighbours.is_empty() {
        let seen: std::collections::HashSet<&str> =
            hits.iter().map(|h| h.note_id.as_str()).collect();
        let sql = format!(
            "SELECT ne.note_id
               FROM note_entities ne
               JOIN entities e ON e.id = ne.entity_id
              WHERE ne.entity_id IN ({})
              GROUP BY ne.note_id
              ORDER BY COUNT(*) DESC, MAX(e.seen) DESC
              LIMIT ?1",
            id_list(&neighbours)
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![(limit * 2) as i64], |r| r.get::<_, String>(0))?;
        let mut extra = Vec::new();
        for note_id in rows {
            let note_id = note_id?;
            if seen.contains(note_id.as_str()) {
                continue;
            }
            extra.push(GraphHit {
                note_id,
                direct: false,
            });
            if extra.len() >= limit {
                break;
            }
        }
        hits.extend(extra);
    }
    Ok(hits)
}

/// Render ids as a SQL list. Safe by construction: these are `i64`s we read out
/// of the database, never user text, so there is nothing to inject. Inlined
/// because SQLite has no array parameter and rebuilding the statement per query
/// is cheaper than a temp table.
fn id_list(ids: &[i64]) -> String {
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_tables(&conn).unwrap();
        conn
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The set-union merge: re-capturing the same entity bumps its count instead
    /// of creating a second node, and the first spelling is the one kept.
    #[test]
    fn entities_deduplicate_by_norm_and_accumulate() {
        let conn = db();
        set_note_entities(&conn, "n1", &names(&["JWT", "auth.py"]), 100).unwrap();
        set_note_entities(&conn, "n2", &names(&["jwt", "  JWT  "]), 200).unwrap();

        let (name, seen): (String, i64) = conn
            .query_row(
                "SELECT name, seen FROM entities WHERE norm = 'jwt'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "JWT", "the first spelling wins");
        assert_eq!(seen, 3, "every mention counts");
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2, "no duplicate node for a case variant");
    }

    /// Re-indexing a note that dropped an entity must drop the link, or the walk
    /// keeps reaching a note that no longer mentions it.
    #[test]
    fn reindexing_replaces_a_notes_links() {
        let conn = db();
        set_note_entities(&conn, "n1", &names(&["JWT", "auth.py"]), 100).unwrap();
        set_note_entities(&conn, "n1", &names(&["JWT"]), 200).unwrap();
        let hits = walk(&conn, &names(&["auth.py"]), 10).unwrap();
        assert!(hits.is_empty(), "{hits:?}");
    }

    /// The whole point of the graph leg: two notes that share NO words are
    /// connected through an entity a third note co-mentions. This is the query
    /// the lexical and vector legs both miss.
    #[test]
    fn the_walk_connects_notes_that_share_no_vocabulary() {
        let conn = db();
        // "Session cookies" and "buffer size" have nothing in common textually…
        set_note_entities(&conn, "old", &names(&["Session Cookie", "auth.py"]), 100).unwrap();
        set_note_entities(&conn, "new", &names(&["auth.py", "buffer size"]), 200).unwrap();

        // …but a query naming one reaches the other through auth.py.
        let hits = walk(&conn, &names(&["session", "cookie"]), 10).unwrap();
        let direct: Vec<&str> = hits
            .iter()
            .filter(|h| h.direct)
            .map(|h| h.note_id.as_str())
            .collect();
        let reached: Vec<&str> = hits
            .iter()
            .filter(|h| !h.direct)
            .map(|h| h.note_id.as_str())
            .collect();
        assert_eq!(direct, vec!["old"], "low level: the note that names it");
        assert_eq!(reached, vec!["new"], "high level: reached by walking");
    }

    /// A typed relation is an enrichment, weighted above bare co-mention.
    #[test]
    fn typed_relations_merge_and_accumulate_weight() {
        let conn = db();
        set_note_entities(&conn, "n1", &names(&["JWT", "auth.py"]), 100).unwrap();
        let rels = vec![Relation {
            from: "JWT".into(),
            pred: "used_for".into(),
            to: "auth.py".into(),
        }];
        record_relations(&conn, &rels, 100).unwrap();
        record_relations(&conn, &rels, 200).unwrap();
        let (count, weight): (i64, i64) = conn
            .query_row("SELECT COUNT(*), MAX(weight) FROM edges", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(count, 1, "set-union merge, not a second edge");
        assert_eq!(weight, 2);
    }

    /// The common case must be cheap and silent: a query naming nothing in the
    /// graph returns nothing, and short tokens never prefix-match the corpus.
    #[test]
    fn a_query_that_names_nothing_returns_nothing() {
        let conn = db();
        set_note_entities(&conn, "n1", &names(&["JWT"]), 100).unwrap();
        assert!(walk(&conn, &names(&["pasta", "recipe"]), 10)
            .unwrap()
            .is_empty());
        assert!(
            walk(&conn, &names(&["j", "jw"]), 10).unwrap().is_empty(),
            "a 1-2 char term must not prefix-match the graph"
        );
        assert!(walk(&conn, &[], 10).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_note_forgets_its_links_but_keeps_the_corpus_count() {
        let conn = db();
        set_note_entities(&conn, "n1", &names(&["JWT"]), 100).unwrap();
        set_note_entities(&conn, "n2", &names(&["JWT"]), 100).unwrap();
        forget_note(&conn, "n1").unwrap();
        let hits = walk(&conn, &names(&["jwt"]), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note_id, "n2");
        let seen: i64 = conn
            .query_row("SELECT seen FROM entities WHERE norm='jwt'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(seen, 2, "a corpus count must not depend on delete history");
    }
}
