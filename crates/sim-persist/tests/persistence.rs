//! Save/load/replay determinism: the persistence half of the contract.

use sim_core::{AccountId, AgentId, Money, PlayerCommand, World, WorldConfig};
use std::path::PathBuf;
use tempfile::TempDir;

fn stimulus(tick: u64) -> (u64, PlayerCommand) {
    (
        tick,
        PlayerCommand::AdjustMoneySupply {
            account: AccountId::Agent(AgentId(15)),
            delta: Money::from_cents(40_000),
            memo: "relief fund".into(),
        },
    )
}

/// A world with a command queued for tick 150 (exercises pending-queue
/// persistence when saving before that tick).
fn seeded_world() -> World {
    let mut w = World::from_config(WorldConfig::default_with_seed(1234));
    let (tick, cmd) = stimulus(150);
    w.queue_command(tick, cmd).unwrap();
    w
}

fn tmp(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

fn event_stream(w: &World) -> Vec<String> {
    w.journal
        .events
        .iter()
        .map(|r| format!("{}:{}:{:?}", r.seq, r.tick, r.event))
        .collect()
}

#[test]
fn save_load_roundtrip_preserves_everything() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "roundtrip.mbsave");
    let mut w = seeded_world();
    w.run_ticks(120).unwrap();
    sim_persist::save(&w, &path).unwrap();
    let loaded = sim_persist::load(&path).unwrap();
    assert_eq!(
        w.state_hash().unwrap(),
        loaded.state_hash().unwrap(),
        "state survives the roundtrip"
    );
    assert_eq!(event_stream(&w), event_stream(&loaded), "journal survives");
    assert_eq!(
        w.inputs.command_log, loaded.inputs.command_log,
        "command log survives"
    );
    assert_eq!(
        w.inputs.pending, loaded.inputs.pending,
        "pending queue survives"
    );
    assert_eq!(w.journal.manifest, loaded.journal.manifest);
}

#[test]
fn resumed_run_matches_uninterrupted_run() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "resume.mbsave");

    // Interrupted: run 100, save, load, continue to 250.
    let mut interrupted = seeded_world();
    interrupted.run_ticks(100).unwrap();
    sim_persist::save(&interrupted, &path).unwrap();
    let mut resumed = sim_persist::load(&path).unwrap();
    resumed.run_ticks(150).unwrap();

    // Uninterrupted: same seed, same command log, straight to 250.
    let mut straight = seeded_world();
    straight.run_ticks(250).unwrap();

    assert_eq!(resumed.state.tick, 250);
    assert_eq!(
        resumed.state_hash().unwrap(),
        straight.state_hash().unwrap(),
        "save/load must not perturb the timeline"
    );
    assert_eq!(resumed.journal.manifest, straight.journal.manifest);
    assert_eq!(
        event_stream(&resumed),
        event_stream(&straight),
        "event sequences must match across a save/load boundary"
    );
}

#[test]
fn replay_from_save_reproduces_the_saved_world() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "replay.mbsave");
    let mut w = seeded_world();
    w.run_ticks(200).unwrap();
    sim_persist::save(&w, &path).unwrap();

    let replayed = sim_persist::replay_from_save(&path).unwrap();
    assert_eq!(replayed.state.tick, 200);
    assert_eq!(
        replayed.state_hash().unwrap(),
        w.state_hash().unwrap(),
        "replay(initial config + command log) must reproduce the save"
    );
    let stored_manifest = sim_persist::read_manifest(&path).unwrap();
    assert_eq!(replayed.journal.manifest, stored_manifest);
}

#[test]
fn meta_and_side_tables_are_readable() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "meta.mbsave");
    let mut w = seeded_world();
    w.run_ticks(60).unwrap();
    sim_persist::save(&w, &path).unwrap();

    let meta = sim_persist::read_meta(&path).unwrap();
    assert_eq!(meta.schema_version, sim_persist::SCHEMA_VERSION);
    assert_eq!(meta.tick, 60);
    assert_eq!(meta.master_seed, 1234);
    assert_eq!(meta.config, w.state.config);
    assert_eq!(meta.world_hash, w.state_hash().unwrap());

    let manifest = sim_persist::read_manifest(&path).unwrap();
    assert!(manifest.iter().any(|(t, _)| *t == 50));

    let events = sim_persist::read_events_range(&path, 0, 60).unwrap();
    assert!(!events.is_empty());
    let commands = sim_persist::read_commands_range(&path, 0, u64::MAX).unwrap();
    assert_eq!(commands.len(), 1, "the queued stimulus is archived");
    assert!(
        !commands[0].applied,
        "tick-150 command is still pending at 60"
    );
}

#[test]
fn load_rejects_garbage_files() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "garbage.mbsave");
    std::fs::write(&path, b"not a sqlite database at all").unwrap();
    assert!(sim_persist::load(&path).is_err());
    assert!(sim_persist::read_meta(&path).is_err());
}

#[test]
fn load_rejects_newer_schema_versions() {
    let dir = TempDir::new().unwrap();
    let path = tmp(&dir, "future.mbsave");
    let w = seeded_world();
    sim_persist::save(&w, &path).unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
    }
    match sim_persist::load(&path) {
        Err(sim_persist::PersistError::UnsupportedSchema { found: 99, .. }) => {}
        other => panic!("expected UnsupportedSchema, got {other:?}"),
    }
}

#[test]
fn missing_file_is_a_clean_error() {
    let err =
        sim_persist::load(std::path::Path::new("definitely/not/a/real/save.mbsave")).unwrap_err();
    assert!(err.to_string().contains("no such save file"));
}
