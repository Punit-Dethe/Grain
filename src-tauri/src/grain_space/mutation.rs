//! [GRAIN] Transactional Mutation Engine (MEMORY-SYSTEM-PLAN.md Phase 3).
//!
//! Enforces:
//! 1. Opaque, cryptographically authenticated, short-lived `TargetToken`s binding
//!    vault path, document ID, base revision, content hash, and operation type.
//! 2. Durable SQLite operation ledger (`memory_operations` and `vault_meta`) with
//!    exact-once idempotency and projection-dirty crash recovery.
//! 3. Targeted append, correction, and block-edit operations with revision comparison.
//! 4. Append-specific safe rebase preserving concurrent external edits without clobbering.
//! 5. Privacy-preserving deletion that purges operation records when notes are deleted.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

use super::block_codec;
use super::note::{MemoryBlock, MemoryBlockKind, Note};
use super::vault::Vault;

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

// -- Ephemeral Process-Local Key for Token Signing ------------------------------

static TOKEN_SECRET: OnceLock<[u8; 32]> = OnceLock::new();

fn get_token_secret() -> &'static [u8; 32] {
    TOKEN_SECRET.get_or_init(|| {
        let u1 = uuid::Uuid::new_v4();
        let u2 = uuid::Uuid::new_v4();
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(u1.as_bytes());
        key[16..].copy_from_slice(u2.as_bytes());
        key
    })
}

/// Standard RFC 2104 HMAC-SHA256 implementation using `sha2::Sha256`.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize().into()
}

// -- Data Structures & Types ---------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Append,
    Correct,
    BlockEdit,
}

impl OperationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationKind::Append => "append",
            OperationKind::Correct => "correct",
            OperationKind::BlockEdit => "block_edit",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "append" => Some(OperationKind::Append),
            "correct" => Some(OperationKind::Correct),
            "block_edit" => Some(OperationKind::BlockEdit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Prepared,
    Committed,
    Rejected,
    Stale,
    Failed,
}

impl OperationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationState::Prepared => "prepared",
            OperationState::Committed => "committed",
            OperationState::Rejected => "rejected",
            OperationState::Stale => "stale",
            OperationState::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "prepared" => Some(OperationState::Prepared),
            "committed" => Some(OperationState::Committed),
            "rejected" => Some(OperationState::Rejected),
            "stale" => Some(OperationState::Stale),
            "failed" => Some(OperationState::Failed),
            _ => None,
        }
    }
}

/// Durable operation record in `memory_operations`.
/// Note: raw note text is NEVER persisted in the ledger to satisfy privacy rules (§6.4, §10.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryOperation {
    pub operation_id: String,
    pub idempotency_key: String,
    pub document_id: String,
    pub base_revision: u64,
    pub committed_revision: Option<u64>,
    pub operation_kind: OperationKind,
    pub proposed_content_hash: String,
    pub state: OperationState,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub error_code: Option<String>,
}

/// Opaque, signed token binding vault, note, revision, and operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub struct TargetToken {
    pub token_id: String,
    pub vault_path: String,
    pub document_id: String,
    pub base_revision: u64,
    pub base_content_hash: String,
    pub allowed_operation: String,
    pub target_block_id: Option<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

/// Result returned upon a successful mutation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub struct MutationResult {
    pub operation_id: String,
    pub document_id: String,
    pub revision: u64,
    pub block_id: String,
    pub state: OperationState,
    pub rebased: bool,
}

/// Structured error codes for mutation failures.
#[derive(Debug, Clone)]
pub enum MutationError {
    InvalidTokenSignature,
    ExpiredToken { issued_at: i64, expires_at: i64 },
    CrossVaultToken {
        expected_vault: String,
        actual_vault: String,
    },
    WrongOperation {
        allowed_operation: String,
        requested_operation: String,
    },
    StaleRevision {
        document_id: String,
        expected_revision: u64,
        actual_revision: u64,
    },
    IdempotencyConflict { key: String },
    DocumentNotFound(String),
    UnadoptedDocument(String),
    TargetBlockNotFound(String),
    Storage(String),
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutationError::InvalidTokenSignature => {
                write!(f, "Invalid token signature: token was tampered with or corrupted")
            }
            MutationError::ExpiredToken { issued_at, expires_at } => {
                write!(f, "Target token has expired (issued at {issued_at}, expired at {expires_at})")
            }
            MutationError::CrossVaultToken { expected_vault, actual_vault } => {
                write!(f, "Token vault mismatch: token bound to '{expected_vault}', active vault is '{actual_vault}'")
            }
            MutationError::WrongOperation { allowed_operation, requested_operation } => {
                write!(f, "Token operation mismatch: token allows '{allowed_operation}', but '{requested_operation}' was attempted")
            }
            MutationError::StaleRevision { document_id, expected_revision, actual_revision } => {
                write!(f, "Document '{document_id}' is stale: base revision {expected_revision} != current {actual_revision}")
            }
            MutationError::IdempotencyConflict { key } => {
                write!(f, "Idempotency key conflict: key '{key}' was previously used with a different content hash")
            }
            MutationError::DocumentNotFound(id) => write!(f, "Target document '{id}' not found"),
            MutationError::UnadoptedDocument(id) => write!(f, "Document '{id}' is an unadopted foreign document and is read-only for agent mutations"),
            MutationError::TargetBlockNotFound(block_id) => write!(f, "Target block '{block_id}' not found in document"),
            MutationError::Storage(msg) => write!(f, "Internal storage error: {msg}"),
        }
    }
}

impl std::error::Error for MutationError {}

// -- Database Schema & Ledger ---------------------------------------------------

/// Ensure all required mutation tables exist in the vault index SQLite database.
pub fn ensure_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_operations (
            operation_id           TEXT PRIMARY KEY,
            idempotency_key        TEXT UNIQUE,
            document_id            TEXT NOT NULL,
            base_revision          INTEGER NOT NULL,
            committed_revision     INTEGER,
            operation_kind         TEXT NOT NULL,
            proposed_content_hash  TEXT NOT NULL,
            state                  TEXT NOT NULL,
            created_at             INTEGER NOT NULL,
            completed_at           INTEGER,
            error_code             TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_mem_ops_doc ON memory_operations(document_id);
        CREATE INDEX IF NOT EXISTS idx_mem_ops_idem ON memory_operations(idempotency_key);

        CREATE TABLE IF NOT EXISTS vault_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Set projection dirty flag for crash recovery.
pub fn mark_projection_dirty(conn: &Connection, dirty: bool) -> Result<()> {
    let val = if dirty { "1" } else { "0" };
    conn.execute(
        "INSERT INTO vault_meta (key, value) VALUES ('projection_dirty', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![val],
    )?;
    Ok(())
}

/// Check if index projection is marked dirty from a previous crash.
pub fn is_projection_dirty(conn: &Connection) -> Result<bool> {
    let res: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = 'projection_dirty'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(res.as_deref() == Some("1"))
}

/// Record a prepared operation in the ledger.
pub fn prepare_operation(conn: &Connection, op: &MemoryOperation) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_operations (
            operation_id, idempotency_key, document_id, base_revision,
            committed_revision, operation_kind, proposed_content_hash,
            state, created_at, completed_at, error_code
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            op.operation_id,
            op.idempotency_key,
            op.document_id,
            op.base_revision as i64,
            op.committed_revision.map(|r| r as i64),
            op.operation_kind.as_str(),
            op.proposed_content_hash,
            op.state.as_str(),
            op.created_at,
            op.completed_at,
            op.error_code,
        ],
    )?;
    Ok(())
}

/// Mark an operation committed in the ledger with its resulting revision.
pub fn commit_operation(
    conn: &Connection,
    operation_id: &str,
    committed_revision: u64,
    completed_at: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE memory_operations
         SET state = 'committed', committed_revision = ?1, completed_at = ?2
         WHERE operation_id = ?3",
        params![committed_revision as i64, completed_at, operation_id],
    )?;
    Ok(())
}

/// Mark an operation as failed/stale/rejected in the ledger.
pub fn fail_operation(
    conn: &Connection,
    operation_id: &str,
    state: OperationState,
    error_code: Option<&str>,
    completed_at: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE memory_operations
         SET state = ?1, error_code = ?2, completed_at = ?3
         WHERE operation_id = ?4",
        params![state.as_str(), error_code, completed_at, operation_id],
    )?;
    Ok(())
}

/// Lookup an existing operation by idempotency key.
pub fn get_operation_by_idempotency_key(
    conn: &Connection,
    key: &str,
) -> Result<Option<MemoryOperation>> {
    let mut stmt = conn.prepare(
        "SELECT operation_id, idempotency_key, document_id, base_revision,
                committed_revision, operation_kind, proposed_content_hash,
                state, created_at, completed_at, error_code
         FROM memory_operations WHERE idempotency_key = ?1",
    )?;

    let op = stmt
        .query_row(params![key], |r| {
            let base_rev: i64 = r.get(3)?;
            let comm_rev: Option<i64> = r.get(4)?;
            let kind_str: String = r.get(5)?;
            let state_str: String = r.get(7)?;
            Ok(MemoryOperation {
                operation_id: r.get(0)?,
                idempotency_key: r.get(1)?,
                document_id: r.get(2)?,
                base_revision: base_rev as u64,
                committed_revision: comm_rev.map(|v| v as u64),
                operation_kind: OperationKind::from_str(&kind_str).unwrap_or(OperationKind::Append),
                proposed_content_hash: r.get(6)?,
                state: OperationState::from_str(&state_str).unwrap_or(OperationState::Failed),
                created_at: r.get(8)?,
                completed_at: r.get(9)?,
                error_code: r.get(10)?,
            })
        })
        .optional()?;

    Ok(op)
}

/// Purge all operation records associated with a document (called on document deletion).
pub fn purge_operations_for_document(conn: &Connection, document_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM memory_operations WHERE document_id = ?1",
        params![document_id],
    )?;
    Ok(())
}

// -- Target Token Cryptographic Primitives --------------------------------------

fn canonical_path_str(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn compute_token_signature(
    token_id: &str,
    vault_path: &str,
    document_id: &str,
    base_revision: u64,
    base_content_hash: &str,
    allowed_operation: &str,
    target_block_id: Option<&str>,
    issued_at: i64,
    expires_at: i64,
) -> String {
    let payload = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}",
        token_id,
        vault_path,
        document_id,
        base_revision,
        base_content_hash,
        allowed_operation,
        target_block_id.unwrap_or(""),
        issued_at,
        expires_at
    );
    let secret = get_token_secret();
    let mac = hmac_sha256(secret, payload.as_bytes());
    hex_encode(&mac)
}

/// Issue an authenticated, short-lived `TargetToken` for a document.
pub fn issue_target_token(
    v: &Vault,
    note: &Note,
    allowed_operation: OperationKind,
    target_block_id: Option<String>,
    ttl_secs: Option<i64>,
) -> Result<TargetToken> {
    if note.id.starts_with('f') && note.schema_version < 3 {
        return Err(MutationError::UnadoptedDocument(note.id.clone()).into());
    }

    let token_id = uuid::Uuid::new_v4().to_string();
    let vault_path = canonical_path_str(&v.root);
    let now = chrono::Utc::now().timestamp_millis();
    let ttl_ms = ttl_secs.unwrap_or(600) * 1000;
    let expires_at = now + ttl_ms;

    let signature = compute_token_signature(
        &token_id,
        &vault_path,
        &note.id,
        note.revision,
        &note.content_hash,
        allowed_operation.as_str(),
        target_block_id.as_deref(),
        now,
        expires_at,
    );

    Ok(TargetToken {
        token_id,
        vault_path,
        document_id: note.id.clone(),
        base_revision: note.revision,
        base_content_hash: note.content_hash.clone(),
        allowed_operation: allowed_operation.as_str().to_string(),
        target_block_id,
        issued_at: now,
        expires_at,
        signature,
    })
}

/// Validate a target token against vault context, required operation, and signature.
pub fn validate_target_token(
    v: &Vault,
    token: &TargetToken,
    required_operation: OperationKind,
) -> Result<(), MutationError> {
    // 1. Signature check (fails closed if tampered)
    let expected_sig = compute_token_signature(
        &token.token_id,
        &token.vault_path,
        &token.document_id,
        token.base_revision,
        &token.base_content_hash,
        &token.allowed_operation,
        token.target_block_id.as_deref(),
        token.issued_at,
        token.expires_at,
    );

    if token.signature != expected_sig {
        return Err(MutationError::InvalidTokenSignature);
    }

    // 2. Expiration check
    let now = chrono::Utc::now().timestamp_millis();
    if now > token.expires_at {
        return Err(MutationError::ExpiredToken {
            issued_at: token.issued_at,
            expires_at: token.expires_at,
        });
    }

    // 3. Vault scoping check
    let current_vault = canonical_path_str(&v.root);
    if token.vault_path != current_vault {
        return Err(MutationError::CrossVaultToken {
            expected_vault: current_vault,
            actual_vault: token.vault_path.clone(),
        });
    }

    // 4. Allowed operation check
    if token.allowed_operation != required_operation.as_str() {
        return Err(MutationError::WrongOperation {
            allowed_operation: token.allowed_operation.clone(),
            requested_operation: required_operation.as_str().to_string(),
        });
    }

    Ok(())
}

// -- Transactional Mutation Execution -------------------------------------------

/// Execute an append operation protected by target token, revision check, and idempotency key.
pub fn execute_transactional_append(
    v: &Vault,
    token: &TargetToken,
    idempotency_key: &str,
    text: &str,
    block_kind: MemoryBlockKind,
    speaker: Option<String>,
    source_ref: Option<String>,
) -> Result<MutationResult> {
    // 1. Token validation
    validate_target_token(v, token, OperationKind::Append)?;

    // 2. Compute proposed content hash
    let proposed_hash = block_codec::compute_content_hash(text);

    // 3. Open database connection
    super::vault::ensure_vault(v)?;
    let conn = super::vault::open_index_pub(v)?;

    // 4. Check idempotency ledger
    if let Some(existing) = get_operation_by_idempotency_key(&conn, idempotency_key)? {
        if existing.proposed_content_hash != proposed_hash {
            return Err(MutationError::IdempotencyConflict {
                key: idempotency_key.to_string(),
            }
            .into());
        }
        if existing.state == OperationState::Committed {
            let rev = existing.committed_revision.unwrap_or(existing.base_revision);
            return Ok(MutationResult {
                operation_id: existing.operation_id,
                document_id: existing.document_id,
                revision: rev,
                block_id: format!("{}-append", token.document_id),
                state: OperationState::Committed,
                rebased: false,
            });
        }
    }

    // 5. Read target note
    let mut note = match super::vault::get_note(v, &token.document_id) {
        Ok(n) => n,
        Err(e) => {
            return Err(MutationError::DocumentNotFound(format!("{}: {e:#}", token.document_id)).into());
        }
    };

    let operation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let prep_op = MemoryOperation {
        operation_id: operation_id.clone(),
        idempotency_key: idempotency_key.to_string(),
        document_id: token.document_id.clone(),
        base_revision: token.base_revision,
        committed_revision: None,
        operation_kind: OperationKind::Append,
        proposed_content_hash: proposed_hash.clone(),
        state: OperationState::Prepared,
        created_at: now,
        completed_at: None,
        error_code: None,
    };
    prepare_operation(&conn, &prep_op)?;

    // 6. Revision & Concurrency check with Safe Rebase (§8.2 rule 8)
    let rebased = if note.revision != token.base_revision || note.content_hash != token.base_content_hash {
        // Document changed externally: pure appends to the end are safe to rebase
        // as long as the document still exists and has not become malformed.
        true
    } else {
        false
    };

    // 7. Find next sequence number
    let next_seq = note
        .blocks
        .iter()
        .map(|b| b.sequence)
        .max()
        .map(|s| s + 1)
        .unwrap_or(0);

    let block_id = format!("{}-b{}", note.id, next_seq);

    let new_block = MemoryBlock {
        id: block_id.clone(),
        document_id: note.id.clone(),
        kind: block_kind,
        text: text.trim().to_string(),
        sequence: next_seq,
        recorded_at: now,
        event_start: None,
        event_end: None,
        source_ref,
        speaker,
        supersedes_block_id: None,
    };

    // Format new block text
    let block_text = block_codec::emit_block(&new_block);
    if note.body.trim().is_empty() {
        note.body = block_text;
    } else {
        note.body = format!("{}\n\n{}", note.body.trim_end(), block_text);
    }

    let new_revision = note.revision + 1;
    note.revision = new_revision;
    note.schema_version = 3;
    note.updated_at = Some(now);
    note.content_hash = block_codec::compute_content_hash(&note.body);
    note.blocks.push(new_block);

    // 8. Atomic save to disk with dirty projection tracking
    mark_projection_dirty(&conn, true)?;
    if let Err(e) = super::vault::save_note(v, &note) {
        let _ = fail_operation(&conn, &operation_id, OperationState::Failed, Some(&e.to_string()), now);
        return Err(MutationError::Storage(e.to_string()).into());
    }
    mark_projection_dirty(&conn, false)?;

    // 9. Commit ledger entry
    commit_operation(&conn, &operation_id, new_revision, now)?;

    Ok(MutationResult {
        operation_id,
        document_id: note.id,
        revision: new_revision,
        block_id,
        state: OperationState::Committed,
        rebased,
    })
}

/// Execute a correction operation linking to a superseded block ID.
pub fn execute_transactional_correction(
    v: &Vault,
    token: &TargetToken,
    idempotency_key: &str,
    text: &str,
    supersedes_block_id: &str,
) -> Result<MutationResult> {
    validate_target_token(v, token, OperationKind::Correct)?;

    let proposed_hash = block_codec::compute_content_hash(text);
    super::vault::ensure_vault(v)?;
    let conn = super::vault::open_index_pub(v)?;

    if let Some(existing) = get_operation_by_idempotency_key(&conn, idempotency_key)? {
        if existing.proposed_content_hash != proposed_hash {
            return Err(MutationError::IdempotencyConflict {
                key: idempotency_key.to_string(),
            }
            .into());
        }
        if existing.state == OperationState::Committed {
            let rev = existing.committed_revision.unwrap_or(existing.base_revision);
            return Ok(MutationResult {
                operation_id: existing.operation_id,
                document_id: existing.document_id,
                revision: rev,
                block_id: format!("{}-correction", token.document_id),
                state: OperationState::Committed,
                rebased: false,
            });
        }
    }

    let mut note = match super::vault::get_note(v, &token.document_id) {
        Ok(n) => n,
        Err(e) => {
            return Err(MutationError::DocumentNotFound(format!("{}: {e:#}", token.document_id)).into());
        }
    };

    // Verify the superseded block exists in the document
    let has_target = note.blocks.iter().any(|b| b.id == supersedes_block_id);
    if !has_target && !note.blocks.is_empty() {
        return Err(MutationError::TargetBlockNotFound(supersedes_block_id.to_string()).into());
    }

    let operation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let prep_op = MemoryOperation {
        operation_id: operation_id.clone(),
        idempotency_key: idempotency_key.to_string(),
        document_id: token.document_id.clone(),
        base_revision: token.base_revision,
        committed_revision: None,
        operation_kind: OperationKind::Correct,
        proposed_content_hash: proposed_hash.clone(),
        state: OperationState::Prepared,
        created_at: now,
        completed_at: None,
        error_code: None,
    };
    prepare_operation(&conn, &prep_op)?;

    let rebased = note.revision != token.base_revision;

    let next_seq = note
        .blocks
        .iter()
        .map(|b| b.sequence)
        .max()
        .map(|s| s + 1)
        .unwrap_or(0);

    let block_id = format!("{}-b{}", note.id, next_seq);

    let correction_block = MemoryBlock {
        id: block_id.clone(),
        document_id: note.id.clone(),
        kind: MemoryBlockKind::Correction,
        text: text.trim().to_string(),
        sequence: next_seq,
        recorded_at: now,
        event_start: None,
        event_end: None,
        source_ref: None,
        speaker: None,
        supersedes_block_id: Some(supersedes_block_id.to_string()),
    };

    let block_text = block_codec::emit_block(&correction_block);
    if note.body.trim().is_empty() {
        note.body = block_text;
    } else {
        note.body = format!("{}\n\n{}", note.body.trim_end(), block_text);
    }

    let new_revision = note.revision + 1;
    note.revision = new_revision;
    note.schema_version = 3;
    note.updated_at = Some(now);
    note.content_hash = block_codec::compute_content_hash(&note.body);
    note.blocks.push(correction_block);

    mark_projection_dirty(&conn, true)?;
    if let Err(e) = super::vault::save_note(v, &note) {
        let _ = fail_operation(&conn, &operation_id, OperationState::Failed, Some(&e.to_string()), now);
        return Err(MutationError::Storage(e.to_string()).into());
    }
    mark_projection_dirty(&conn, false)?;

    commit_operation(&conn, &operation_id, new_revision, now)?;

    Ok(MutationResult {
        operation_id,
        document_id: note.id,
        revision: new_revision,
        block_id,
        state: OperationState::Committed,
        rebased,
    })
}

/// Execute an explicit block replacement. Strictly requires base revision match (§8.3).
pub fn execute_transactional_block_edit(
    v: &Vault,
    token: &TargetToken,
    idempotency_key: &str,
    target_block_id: &str,
    new_text: &str,
) -> Result<MutationResult> {
    validate_target_token(v, token, OperationKind::BlockEdit)?;

    let proposed_hash = block_codec::compute_content_hash(new_text);
    super::vault::ensure_vault(v)?;
    let conn = super::vault::open_index_pub(v)?;

    if let Some(existing) = get_operation_by_idempotency_key(&conn, idempotency_key)? {
        if existing.proposed_content_hash != proposed_hash {
            return Err(MutationError::IdempotencyConflict {
                key: idempotency_key.to_string(),
            }
            .into());
        }
        if existing.state == OperationState::Committed {
            let rev = existing.committed_revision.unwrap_or(existing.base_revision);
            return Ok(MutationResult {
                operation_id: existing.operation_id,
                document_id: existing.document_id,
                revision: rev,
                block_id: target_block_id.to_string(),
                state: OperationState::Committed,
                rebased: false,
            });
        }
    }

    let mut note = match super::vault::get_note(v, &token.document_id) {
        Ok(n) => n,
        Err(e) => {
            return Err(MutationError::DocumentNotFound(format!("{}: {e:#}", token.document_id)).into());
        }
    };

    // Block edit strictly enforces revision comparison
    if note.revision != token.base_revision || note.content_hash != token.base_content_hash {
        let now = chrono::Utc::now().timestamp_millis();
        let op_id = uuid::Uuid::new_v4().to_string();
        let _ = fail_operation(&conn, &op_id, OperationState::Stale, Some("stale_revision"), now);
        return Err(MutationError::StaleRevision {
            document_id: note.id,
            expected_revision: token.base_revision,
            actual_revision: note.revision,
        }
        .into());
    }

    // Find and update block
    let block_idx = note
        .blocks
        .iter()
        .position(|b| b.id == target_block_id)
        .ok_or_else(|| MutationError::TargetBlockNotFound(target_block_id.to_string()))?;

    note.blocks[block_idx].text = new_text.trim().to_string();
    note.blocks[block_idx].recorded_at = chrono::Utc::now().timestamp_millis();

    // Re-emit body with updated blocks
    note.body = block_codec::emit_blocks(&note.blocks);

    let operation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let new_revision = note.revision + 1;
    note.revision = new_revision;
    note.schema_version = 3;
    note.updated_at = Some(now);
    note.content_hash = block_codec::compute_content_hash(&note.body);

    mark_projection_dirty(&conn, true)?;
    if let Err(e) = super::vault::save_note(v, &note) {
        let _ = fail_operation(&conn, &operation_id, OperationState::Failed, Some(&e.to_string()), now);
        return Err(MutationError::Storage(e.to_string()).into());
    }
    mark_projection_dirty(&conn, false)?;

    commit_operation(&conn, &operation_id, new_revision, now)?;

    Ok(MutationResult {
        operation_id,
        document_id: note.id,
        revision: new_revision,
        block_id: target_block_id.to_string(),
        state: OperationState::Committed,
        rebased: false,
    })
}

// -- Unit Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_space::vault::{open_index_pub, save_note};

    fn temp_vault(tag: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("grain_mutation_test_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("vault")).unwrap();
        std::fs::create_dir_all(dir.join("appdata")).unwrap();
        Vault {
            root: dir.join("vault"),
            folder: "Grain".to_string(),
            index_base: dir.join("appdata"),
            native: false,
        }
    }

    fn cleanup(v: &Vault) {
        let _ = std::fs::remove_dir_all(v.root.parent().unwrap());
    }

    fn sample_note() -> Note {
        let mut note = Note::raw("Initial body sentence.".into());
        note.id = "grain_test_doc_01".into();
        note.title = "Sample Note".into();
        note.schema_version = 3;
        note.revision = 1;
        note.content_hash = block_codec::compute_content_hash(&note.body);
        note
    }

    #[test]
    fn test_target_token_issue_and_validates_cleanly() {
        let v = temp_vault("token_ok");
        let note = sample_note();
        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();

        assert_eq!(token.document_id, note.id);
        assert_eq!(token.base_revision, 1);
        assert_eq!(token.allowed_operation, "append");
        assert!(validate_target_token(&v, &token, OperationKind::Append).is_ok());
        cleanup(&v);
    }

    #[test]
    fn test_tampered_token_fails_closed() {
        let v = temp_vault("token_tamper");
        let note = sample_note();
        let mut token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();

        // Tamper with base revision
        token.base_revision = 99;
        assert!(matches!(
            validate_target_token(&v, &token, OperationKind::Append),
            Err(MutationError::InvalidTokenSignature)
        ));
        cleanup(&v);
    }

    #[test]
    fn test_expired_token_fails_closed() {
        let v = temp_vault("token_expired");
        let note = sample_note();
        // Negative TTL produces an expired token
        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(-10)).unwrap();

        assert!(matches!(
            validate_target_token(&v, &token, OperationKind::Append),
            Err(MutationError::ExpiredToken { .. })
        ));
        cleanup(&v);
    }

    #[test]
    fn test_cross_vault_token_fails_closed() {
        let v1 = temp_vault("v1");
        let v2 = temp_vault("v2");
        let note = sample_note();
        let token = issue_target_token(&v1, &note, OperationKind::Append, None, Some(60)).unwrap();

        assert!(matches!(
            validate_target_token(&v2, &token, OperationKind::Append),
            Err(MutationError::CrossVaultToken { .. })
        ));
        cleanup(&v1);
        cleanup(&v2);
    }

    #[test]
    fn test_wrong_operation_token_fails_closed() {
        let v = temp_vault("wrong_op");
        let note = sample_note();
        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();

        assert!(matches!(
            validate_target_token(&v, &token, OperationKind::Correct),
            Err(MutationError::WrongOperation { .. })
        ));
        cleanup(&v);
    }

    #[test]
    fn test_transactional_append_exact_revision() {
        let v = temp_vault("append_exact");
        let note = sample_note();
        save_note(&v, &note).unwrap();

        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();
        let res = execute_transactional_append(
            &v,
            &token,
            "idem_key_001",
            "Appended paragraph text.",
            MemoryBlockKind::Append,
            Some("Alice".into()),
            None,
        )
        .unwrap();

        assert_eq!(res.revision, 2);
        assert!(!res.rebased);
        assert_eq!(res.state, OperationState::Committed);

        // Verify on-disk note
        let updated = super::super::vault::get_note(&v, &note.id).unwrap();
        assert_eq!(updated.revision, 2);
        assert!(updated.body.contains("Appended paragraph text."));
        assert_eq!(updated.blocks.len(), 2);
        cleanup(&v);
    }

    #[test]
    fn test_idempotent_append_returns_identical_result_without_duplicate_block() {
        let v = temp_vault("append_idem");
        let note = sample_note();
        save_note(&v, &note).unwrap();

        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();
        let res1 = execute_transactional_append(
            &v,
            &token,
            "idem_key_retry",
            "First attempt text.",
            MemoryBlockKind::Append,
            None,
            None,
        )
        .unwrap();

        // Repeated call with same idempotency key and same content
        let res2 = execute_transactional_append(
            &v,
            &token,
            "idem_key_retry",
            "First attempt text.",
            MemoryBlockKind::Append,
            None,
            None,
        )
        .unwrap();

        assert_eq!(res1.operation_id, res2.operation_id);
        assert_eq!(res1.revision, res2.revision);

        let updated = super::super::vault::get_note(&v, &note.id).unwrap();
        assert_eq!(updated.blocks.len(), 2, "must not duplicate block on retry");
        cleanup(&v);
    }

    #[test]
    fn test_idempotent_append_with_different_content_fails_closed() {
        let v = temp_vault("append_idem_conflict");
        let note = sample_note();
        save_note(&v, &note).unwrap();

        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();
        execute_transactional_append(
            &v,
            &token,
            "idem_key_conflict",
            "Original content.",
            MemoryBlockKind::Append,
            None,
            None,
        )
        .unwrap();

        // Same idempotency key with conflicting content must fail
        let err = execute_transactional_append(
            &v,
            &token,
            "idem_key_conflict",
            "Different conflicting content!",
            MemoryBlockKind::Append,
            None,
            None,
        );

        assert!(err.is_err());
        cleanup(&v);
    }

    #[test]
    fn test_append_safe_rebase_on_external_concurrent_edit() {
        let v = temp_vault("append_rebase");
        let note = sample_note();
        save_note(&v, &note).unwrap();

        // Token issued at revision 1
        let token = issue_target_token(&v, &note, OperationKind::Append, None, Some(60)).unwrap();

        // Simulate external concurrent edit: revision bumps to 2
        let mut concurrent = note.clone();
        concurrent.body = format!("{}\nExternal concurrent line.", concurrent.body);
        concurrent.revision = 2;
        save_note(&v, &concurrent).unwrap();

        // Pure append safely rebases on top of concurrent edit
        let res = execute_transactional_append(
            &v,
            &token,
            "idem_rebase",
            "Rebased append.",
            MemoryBlockKind::Append,
            None,
            None,
        )
        .unwrap();

        assert!(res.rebased, "must be flagged as rebased");
        assert_eq!(res.revision, 3);

        let on_disk = super::super::vault::get_note(&v, &note.id).unwrap();
        assert_eq!(on_disk.revision, 3);
        assert!(on_disk.body.contains("External concurrent line."));
        assert!(on_disk.body.contains("Rebased append."));
        cleanup(&v);
    }

    #[test]
    fn test_block_edit_strict_revision_rejection() {
        let v = temp_vault("block_edit");
        let mut note = sample_note();
        let b0 = MemoryBlock {
            id: format!("{}-b0", note.id),
            document_id: note.id.clone(),
            kind: MemoryBlockKind::Body,
            text: "Original block text.".into(),
            sequence: 0,
            recorded_at: 1000,
            event_start: None,
            event_end: None,
            source_ref: None,
            speaker: None,
            supersedes_block_id: None,
        };
        note.body = block_codec::emit_block(&b0);
        note.blocks = vec![b0];
        note.content_hash = block_codec::compute_content_hash(&note.body);
        save_note(&v, &note).unwrap();

        let token = issue_target_token(
            &v,
            &note,
            OperationKind::BlockEdit,
            Some(format!("{}-b0", note.id)),
            Some(60),
        )
        .unwrap();

        // External concurrent modification
        let mut concurrent = note.clone();
        concurrent.revision = 2;
        save_note(&v, &concurrent).unwrap();

        // Block edit strictly rejects stale revisions
        let err = execute_transactional_block_edit(
            &v,
            &token,
            "idem_edit_stale",
            &format!("{}-b0", note.id),
            "Replacement text",
        );

        assert!(err.is_err(), "block edit must reject stale base revision");
        cleanup(&v);
    }

    #[test]
    fn test_correction_creates_superseding_block() {
        let v = temp_vault("correct");
        let mut note = sample_note();
        let b0 = MemoryBlock {
            id: "b_wrong".into(),
            document_id: note.id.clone(),
            kind: MemoryBlockKind::Body,
            text: "Initial erroneous statement.".into(),
            sequence: 0,
            recorded_at: 1000,
            event_start: None,
            event_end: None,
            source_ref: None,
            speaker: None,
            supersedes_block_id: None,
        };
        note.body = block_codec::emit_block(&b0);
        note.blocks = vec![b0];
        save_note(&v, &note).unwrap();

        let token = issue_target_token(&v, &note, OperationKind::Correct, Some("b_wrong".into()), Some(60)).unwrap();
        let res = execute_transactional_correction(
            &v,
            &token,
            "idem_corr_1",
            "Corrected fact: actual truth here.",
            "b_wrong",
        )
        .unwrap();

        assert_eq!(res.revision, 2);
        let updated = super::super::vault::get_note(&v, &note.id).unwrap();
        assert_eq!(updated.blocks.len(), 2);
        assert_eq!(updated.blocks[1].kind, MemoryBlockKind::Correction);
        assert_eq!(updated.blocks[1].supersedes_block_id.as_deref(), Some("b_wrong"));
        cleanup(&v);
    }

    #[test]
    fn test_deletion_purges_ledger_records() {
        let v = temp_vault("delete_purge");
        let conn = open_index_pub(&v).unwrap();
        let op = MemoryOperation {
            operation_id: "op_to_purge".into(),
            idempotency_key: "idem_purge".into(),
            document_id: "doc_to_delete".into(),
            base_revision: 1,
            committed_revision: Some(2),
            operation_kind: OperationKind::Append,
            proposed_content_hash: "hash".into(),
            state: OperationState::Committed,
            created_at: 1000,
            completed_at: Some(1010),
            error_code: None,
        };
        prepare_operation(&conn, &op).unwrap();

        assert!(get_operation_by_idempotency_key(&conn, "idem_purge").unwrap().is_some());
        purge_operations_for_document(&conn, "doc_to_delete").unwrap();
        assert!(get_operation_by_idempotency_key(&conn, "idem_purge").unwrap().is_none());
        cleanup(&v);
    }

    #[test]
    fn test_crash_recovery_projection_dirty_resync() {
        let v = temp_vault("crash_dirty");
        let conn = open_index_pub(&v).unwrap();
        // 1. Mark dirty as would happen during a crash
        mark_projection_dirty(&conn, true).unwrap();
        assert!(is_projection_dirty(&conn).unwrap());

        // 2. Reconcile detects dirty projection and clears it upon re-sync
        super::super::vault::list_notes(&v).unwrap();
        let conn2 = open_index_pub(&v).unwrap();
        assert!(!is_projection_dirty(&conn2).unwrap(), "reconcile must clear dirty flag after resync");
        cleanup(&v);
    }

    #[test]
    fn test_unadopted_foreign_document_refuses_target_token() {
        let v = temp_vault("foreign_refuse");
        let mut foreign = sample_note();
        foreign.id = "f_unadopted_01".to_string();
        foreign.schema_version = 1; // legacy / foreign

        let res = issue_target_token(&v, &foreign, OperationKind::Append, None, Some(60));
        assert!(res.is_err());
        let err_str = res.err().unwrap().to_string();
        assert!(err_str.contains("unadopted foreign document"), "must reject unadopted foreign docs: {err_str}");
        cleanup(&v);
    }
}
