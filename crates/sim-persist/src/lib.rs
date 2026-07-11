//! SQLite persistence for Marketborn.
//!
//! A save file is a full snapshot written in one transaction:
//! - `world` — the authoritative postcard blob of the whole [`World`]
//!   (state + input log + journal), sufficient to resume deterministically;
//! - `meta` — human-readable key/values (schema version, seed, config JSON,
//!   tick, state hash);
//! - `commands`, `events`, `manifest` — queryable side tables for tooling
//!   (`sim-cli diff`) and the event archive.
//!
//! SQLite is never touched inside the tick loop; saves happen on demand from
//! outside `sim-core`.

#![forbid(unsafe_code)]

use rusqlite::{params, Connection, OptionalExtension};
use sim_core::{QueuedCommand, World, WorldConfig};
use std::path::Path;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("world blob serialization error: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid save file: {0}")]
    Invalid(String),
    #[error("unsupported schema version {found} (this build supports {supported})")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("replay failed: {0}")]
    Replay(String),
}

#[derive(Clone, Debug)]
pub struct SaveMeta {
    pub schema_version: u32,
    pub tick: u64,
    pub master_seed: u64,
    pub config: WorldConfig,
    pub world_hash: String,
    pub app_version: String,
}

#[derive(Clone, Debug)]
pub struct EventArchiveRow {
    pub seq: u64,
    pub tick: u64,
    pub kind: String,
    pub json: String,
}

#[derive(Clone, Debug)]
pub struct CommandArchiveRow {
    pub seq: u64,
    pub tick: u64,
    pub applied: bool,
    pub json: String,
}

const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE world (id INTEGER PRIMARY KEY CHECK (id = 1), data BLOB NOT NULL);
CREATE TABLE commands (
    seq INTEGER PRIMARY KEY,
    tick INTEGER NOT NULL,
    applied INTEGER NOT NULL,
    data TEXT NOT NULL
);
CREATE TABLE events (
    seq INTEGER PRIMARY KEY,
    tick INTEGER NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE TABLE manifest (tick INTEGER PRIMARY KEY, hash TEXT NOT NULL);
";

/// Write a complete save. Overwrites `path` if it exists, clearing any
/// stale SQLite sidecar files (`-journal`/`-wal`/`-shm`) left by an
/// interrupted earlier write so they cannot roll back the fresh database.
pub fn save(world: &World, path: &Path) -> Result<(), PersistError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        let sidecar = std::path::PathBuf::from(sidecar);
        if sidecar.exists() {
            std::fs::remove_file(&sidecar)?;
        }
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;

    let blob = postcard::to_allocvec(world)?;
    tx.execute("INSERT INTO world (id, data) VALUES (1, ?1)", params![blob])?;

    let world_hash = world
        .state_hash()
        .map_err(|e| PersistError::Invalid(e.to_string()))?;
    let meta = [
        ("schema_version", SCHEMA_VERSION.to_string()),
        ("tick", world.state.tick.to_string()),
        ("master_seed", world.state.config.master_seed.to_string()),
        ("config", serde_json::to_string(&world.state.config)?),
        ("world_hash", world_hash),
        ("app_version", env!("CARGO_PKG_VERSION").to_string()),
    ];
    for (k, v) in meta {
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }

    for qc in &world.inputs.command_log {
        let applied = !world.inputs.pending.iter().any(|p| p.seq == qc.seq);
        tx.execute(
            "INSERT INTO commands (seq, tick, applied, data) VALUES (?1, ?2, ?3, ?4)",
            params![
                qc.seq as i64,
                qc.tick as i64,
                applied as i64,
                serde_json::to_string(qc)?
            ],
        )?;
    }

    for record in &world.journal.events {
        tx.execute(
            "INSERT INTO events (seq, tick, kind, data) VALUES (?1, ?2, ?3, ?4)",
            params![
                record.seq as i64,
                record.tick as i64,
                record.event.kind(),
                serde_json::to_string(&record.event)?
            ],
        )?;
    }

    for (tick, hash) in &world.journal.manifest {
        tx.execute(
            "INSERT INTO manifest (tick, hash) VALUES (?1, ?2)",
            params![*tick as i64, hash],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn check_schema(conn: &Connection) -> Result<u32, PersistError> {
    let found: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let found = found
        .ok_or_else(|| PersistError::Invalid("missing schema_version".into()))?
        .parse::<u32>()
        .map_err(|_| PersistError::Invalid("malformed schema_version".into()))?;
    if found > SCHEMA_VERSION {
        return Err(PersistError::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(found)
}

fn open_existing(path: &Path) -> Result<Connection, PersistError> {
    if !path.exists() {
        return Err(PersistError::Invalid(format!(
            "no such save file: {}",
            path.display()
        )));
    }
    Ok(Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

/// Load the authoritative world blob.
pub fn load(path: &Path) -> Result<World, PersistError> {
    let conn = open_existing(path)?;
    check_schema(&conn)?;
    let blob: Vec<u8> = conn.query_row("SELECT data FROM world WHERE id = 1", [], |r| r.get(0))?;
    let world: World = postcard::from_bytes(&blob)?;
    Ok(world)
}

pub fn read_meta(path: &Path) -> Result<SaveMeta, PersistError> {
    let conn = open_existing(path)?;
    let schema_version = check_schema(&conn)?;
    let get = |key: &str| -> Result<String, PersistError> {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .ok_or_else(|| PersistError::Invalid(format!("missing meta key '{key}'")))
    };
    let config: WorldConfig = serde_json::from_str(&get("config")?)?;
    Ok(SaveMeta {
        schema_version,
        tick: get("tick")?
            .parse()
            .map_err(|_| PersistError::Invalid("malformed tick".into()))?,
        master_seed: get("master_seed")?
            .parse()
            .map_err(|_| PersistError::Invalid("malformed master_seed".into()))?,
        config,
        world_hash: get("world_hash")?,
        app_version: get("app_version")?,
    })
}

pub fn read_manifest(path: &Path) -> Result<Vec<(u64, String)>, PersistError> {
    let conn = open_existing(path)?;
    check_schema(&conn)?;
    let mut stmt = conn.prepare("SELECT tick, hash FROM manifest ORDER BY tick")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Clamp a u64 tick into SQLite's i64 domain instead of wrapping.
fn sql_tick(t: u64) -> i64 {
    t.min(i64::MAX as u64) as i64
}

pub fn read_events_range(
    path: &Path,
    from_tick: u64,
    to_tick: u64,
) -> Result<Vec<EventArchiveRow>, PersistError> {
    let conn = open_existing(path)?;
    check_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT seq, tick, kind, data FROM events WHERE tick >= ?1 AND tick <= ?2 ORDER BY seq",
    )?;
    let rows = stmt
        .query_map(params![sql_tick(from_tick), sql_tick(to_tick)], |r| {
            Ok(EventArchiveRow {
                seq: r.get::<_, i64>(0)? as u64,
                tick: r.get::<_, i64>(1)? as u64,
                kind: r.get(2)?,
                json: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn read_commands_range(
    path: &Path,
    from_tick: u64,
    to_tick: u64,
) -> Result<Vec<CommandArchiveRow>, PersistError> {
    let conn = open_existing(path)?;
    check_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT seq, tick, applied, data FROM commands \
         WHERE tick >= ?1 AND tick <= ?2 ORDER BY seq",
    )?;
    let rows = stmt
        .query_map(params![sql_tick(from_tick), sql_tick(to_tick)], |r| {
            Ok(CommandArchiveRow {
                seq: r.get::<_, i64>(0)? as u64,
                tick: r.get::<_, i64>(1)? as u64,
                applied: r.get::<_, i64>(2)? != 0,
                json: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Rebuild the saved run from first principles: regenerate the initial world
/// from the saved config, requeue the full command log, and tick forward to
/// the saved tick. The caller compares hashes/manifests against the save.
pub fn replay_from_save(path: &Path) -> Result<World, PersistError> {
    let meta = read_meta(path)?;
    let conn = open_existing(path)?;
    let mut stmt = conn.prepare("SELECT data FROM commands ORDER BY seq")?;
    let log = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|json| serde_json::from_str::<QueuedCommand>(&json))
        .collect::<Result<Vec<_>, _>>()?;
    let mut world = World::from_config_and_log(meta.config, log);
    world
        .run_ticks(meta.tick)
        .map_err(|e| PersistError::Replay(e.to_string()))?;
    Ok(world)
}
