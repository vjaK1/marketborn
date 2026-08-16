//! Marketborn desktop shell.
//!
//! A dedicated simulation thread owns the [`World`]. The UI thread never
//! touches simulation state: inbound messages flow through an mpsc channel
//! (speed changes, save requests, player commands), outbound state flows as
//! throttled `snapshot` events (≤ 10 Hz) per the protocol in
//! `docs/ARCHITECTURE.md`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use sim_core::{TickError, World, WorldConfig, WorldSnapshot};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info, warn};

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100); // 10 Hz
const DEFAULT_SEED: u64 = 42;

enum ShellMsg {
    SetSpeed(u8),
    Save(Sender<Result<String, String>>),
    /// Inspector detail query: fetch one agent by id (ARCHITECTURE.md
    /// on-demand protocol — the 10 Hz snapshot stays lean).
    AgentDetail(u32, Sender<Option<Value>>),
    /// Contract view detail query: terms, negotiation log, history.
    ContractDetail(u32, Sender<Option<Value>>),
    /// A player command (the policy levers), queued at the next tick
    /// boundary — the only channel that mutates the world.
    QueueCommand(sim_core::PlayerCommand, Sender<Result<(u64, u64), String>>),
}

struct Shared {
    snapshot: Arc<Mutex<Option<Value>>>,
    tx: Mutex<Sender<ShellMsg>>,
}

/// Ticks-per-second pacing per speed level; `None` = paused.
/// Level 4 is "max": tick back-to-back, snapshots still throttled.
fn tick_interval(speed: u8) -> Option<Duration> {
    match speed {
        0 => None,
        1 => Some(Duration::from_millis(500)), // 2 ticks/s
        2 => Some(Duration::from_millis(100)), // 10 ticks/s
        3 => Some(Duration::from_millis(20)),  // 50 ticks/s
        _ => Some(Duration::ZERO),
    }
}

#[tauri::command]
fn get_snapshot(shared: State<'_, Shared>) -> Option<Value> {
    shared.snapshot.lock().ok().and_then(|s| s.clone())
}

#[tauri::command]
fn set_speed(level: u8, shared: State<'_, Shared>) -> Result<(), String> {
    let tx = shared.tx.lock().map_err(|_| "shell channel poisoned")?;
    tx.send(ShellMsg::SetSpeed(level))
        .map_err(|_| "simulation thread is gone".to_string())
}

#[tauri::command]
fn get_agent_detail(id: u32, shared: State<'_, Shared>) -> Result<Option<Value>, String> {
    let (reply_tx, reply_rx) = channel();
    {
        let tx = shared.tx.lock().map_err(|_| "shell channel poisoned")?;
        tx.send(ShellMsg::AgentDetail(id, reply_tx))
            .map_err(|_| "simulation thread is gone".to_string())?;
    }
    reply_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "detail query timed out".to_string())
}

#[tauri::command]
fn get_contract_detail(id: u32, shared: State<'_, Shared>) -> Result<Option<Value>, String> {
    let (reply_tx, reply_rx) = channel();
    {
        let tx = shared.tx.lock().map_err(|_| "shell channel poisoned")?;
        tx.send(ShellMsg::ContractDetail(id, reply_tx))
            .map_err(|_| "simulation thread is gone".to_string())?;
    }
    reply_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "detail query timed out".to_string())
}

#[tauri::command]
fn queue_command(
    command: sim_core::PlayerCommand,
    shared: State<'_, Shared>,
) -> Result<Value, String> {
    let (reply_tx, reply_rx) = channel();
    {
        let tx = shared.tx.lock().map_err(|_| "shell channel poisoned")?;
        tx.send(ShellMsg::QueueCommand(command, reply_tx))
            .map_err(|_| "simulation thread is gone".to_string())?;
    }
    let (seq, tick) = reply_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "command queue timed out".to_string())??;
    Ok(serde_json::json!({ "seq": seq, "tick": tick }))
}

#[tauri::command]
fn save_game(shared: State<'_, Shared>) -> Result<String, String> {
    let (reply_tx, reply_rx) = channel();
    {
        let tx = shared.tx.lock().map_err(|_| "shell channel poisoned")?;
        tx.send(ShellMsg::Save(reply_tx))
            .map_err(|_| "simulation thread is gone".to_string())?;
    }
    reply_rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "save timed out".to_string())?
}

fn publish_snapshot(app: &AppHandle, slot: &Arc<Mutex<Option<Value>>>, world: &World) {
    let snap = WorldSnapshot::capture(world);
    match serde_json::to_value(&snap) {
        Ok(value) => {
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(value.clone());
            }
            if let Err(e) = app.emit("snapshot", &value) {
                warn!("snapshot emit failed: {e}");
            }
        }
        Err(e) => error!("snapshot serialization failed: {e}"),
    }
}

fn save_world(app: &AppHandle, world: &World) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?
        .join("saves");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create save dir: {e}"))?;
    let path = dir.join("quicksave.mbsave");
    sim_persist::save(world, &path).map_err(|e| format!("save failed: {e}"))?;
    Ok(path.display().to_string())
}

fn sim_thread(app: AppHandle, rx: Receiver<ShellMsg>, slot: Arc<Mutex<Option<Value>>>) {
    let mut world = World::from_config(WorldConfig::default_with_seed(DEFAULT_SEED));
    info!(
        "world ready: seed {}, {} agents, {} businesses",
        DEFAULT_SEED,
        world.state.agents.len(),
        world.state.businesses.len()
    );
    let mut speed: u8 = 1;
    let mut next_tick = Instant::now();
    let mut last_snapshot = Instant::now();
    publish_snapshot(&app, &slot, &world);

    let handle_msg =
        |msg: ShellMsg, world: &mut World, speed: &mut u8, next_tick: &mut Instant| -> bool {
            match msg {
                ShellMsg::SetSpeed(level) => {
                    *speed = level.min(4);
                    *next_tick = Instant::now();
                    info!("speed set to {}", *speed);
                }
                ShellMsg::Save(reply) => {
                    let result = save_world(&app, world);
                    match &result {
                        Ok(path) => info!("saved to {path}"),
                        Err(e) => error!("{e}"),
                    }
                    let _ = reply.send(result);
                }
                ShellMsg::AgentDetail(id, reply) => {
                    let detail = sim_core::AgentDetail::capture(world, sim_core::AgentId(id))
                        .and_then(|d| serde_json::to_value(&d).ok());
                    let _ = reply.send(detail);
                }
                ShellMsg::ContractDetail(id, reply) => {
                    let detail = sim_core::ContractDetail::capture(world, sim_core::ContractId(id))
                        .and_then(|d| serde_json::to_value(&d).ok());
                    let _ = reply.send(detail);
                }
                ShellMsg::QueueCommand(cmd, reply) => {
                    let at = world.state.tick + 1;
                    let result = world
                        .queue_command(at, cmd)
                        .map(|seq| (seq, at))
                        .map_err(|e| e.to_string());
                    match &result {
                        Ok((seq, tick)) => info!("command #{seq} queued for tick {tick}"),
                        Err(e) => warn!("command refused: {e}"),
                    }
                    let _ = reply.send(result);
                }
            }
            true
        };

    loop {
        // Drain without blocking while running; block briefly while paused.
        match tick_interval(speed) {
            None => {
                if last_snapshot.elapsed() >= SNAPSHOT_INTERVAL {
                    publish_snapshot(&app, &slot, &world);
                    last_snapshot = Instant::now();
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(msg) => {
                        handle_msg(msg, &mut world, &mut speed, &mut next_tick);
                        publish_snapshot(&app, &slot, &world);
                        last_snapshot = Instant::now();
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            Some(interval) => {
                while let Ok(msg) = rx.try_recv() {
                    handle_msg(msg, &mut world, &mut speed, &mut next_tick);
                }
                if tick_interval(speed).is_none() {
                    continue;
                }
                let now = Instant::now();
                if now < next_tick {
                    std::thread::sleep((next_tick - now).min(Duration::from_millis(10)));
                } else {
                    match world.tick() {
                        Ok(_) => {}
                        Err(TickError::Invariant(v)) => {
                            error!("simulation halted by invariant:\n{v}");
                            speed = 0;
                        }
                        Err(e) => {
                            error!("simulation halted: {e}");
                            speed = 0;
                        }
                    }
                    next_tick = if interval.is_zero() {
                        now
                    } else {
                        now + interval
                    };
                }
                if last_snapshot.elapsed() >= SNAPSHOT_INTERVAL {
                    publish_snapshot(&app, &slot, &world);
                    last_snapshot = Instant::now();
                }
            }
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    tauri::Builder::default()
        .setup(|app| {
            let (tx, rx) = channel::<ShellMsg>();
            let slot: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
            app.manage(Shared {
                snapshot: slot.clone(),
                tx: Mutex::new(tx),
            });
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("marketborn-sim".into())
                .spawn(move || sim_thread(handle, rx, slot))?;
            info!("Marketborn shell up; simulation thread spawned");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_speed,
            save_game,
            get_agent_detail,
            get_contract_detail,
            queue_command
        ])
        .run(tauri::generate_context!())
        .expect("failed to launch Marketborn");
}
