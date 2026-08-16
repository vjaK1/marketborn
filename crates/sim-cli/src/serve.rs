//! `sim-cli serve`: the websocket implementation of the transport protocol
//! (`docs/ARCHITECTURE.md`). The same shape the Tauri shell speaks —
//! inbound control messages and on-demand detail queries, outbound
//! throttled `snapshot` pushes (≤ 10 Hz) — carried as JSON text frames so
//! the React app runs in a plain browser and Playwright can drive it.
//!
//! Protocol. Client → server (any message may carry a `req` id for a
//! correlated reply):
//!
//! ```json
//! {"kind":"set_speed","level":2,"req":1}
//! {"kind":"save","req":2}
//! {"kind":"agent_detail","id":3,"req":3}
//! {"kind":"contract_detail","id":0,"req":4}
//! {"kind":"queue_command","command":{"SetSalesTax":{"rate_bp":500}},"req":5}
//! ```
//!
//! Server → client:
//!
//! ```json
//! {"kind":"snapshot","data":{...}}
//! {"kind":"reply","req":5,"ok":true,"data":{"seq":0,"tick":120}}
//! {"kind":"reply","req":5,"ok":false,"error":"..."}
//! ```
//!
//! `queue_command` accepts any `PlayerCommand` (serde's external tagging)
//! and queues it for the next tick boundary — the only channel that
//! mutates the world, identical to every other transport. A snapshot is
//! pushed to all clients after every handled message, so a driver sees
//! its effect without waiting out the throttle.
//!
//! Threading: one sim thread owns the [`World`] (sim-core itself stays
//! single-threaded); one accept thread; one thread per client that both
//! pumps its outbound queue and polls the socket under a short read
//! timeout. Registration and requests flow to the sim thread over mpsc;
//! replies ride the requesting client's outbound queue. A dead client is
//! pruned the first time a push fails.

use serde::Deserialize;
use serde_json::{json, Value};
use sim_core::{PlayerCommand, TickError, World, WorldConfig, WorldSnapshot};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100); // 10 Hz
const CLIENT_POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Debug)]
pub struct ServeConfig {
    pub seed: u64,
    pub population: u32,
    pub hash_every: u64,
    /// Port to bind on 127.0.0.1; 0 asks the OS for an ephemeral port
    /// (the bound port is reported in [`ServeHandle::port`]).
    pub port: u16,
    /// Directory the `save` message writes `quicksave.mbsave` into.
    pub save_dir: PathBuf,
}

/// A running server: the bound port and the accept-thread handle. Threads
/// run until the process exits (the CLI parks on `join`).
pub struct ServeHandle {
    pub port: u16,
    accept_thread: std::thread::JoinHandle<()>,
}

impl ServeHandle {
    /// Block forever serving clients (the CLI's foreground mode).
    pub fn join(self) {
        let _ = self.accept_thread.join();
    }
}

/// Ticks-per-second pacing per speed level; `None` = paused. Identical to
/// the desktop shell's table.
fn tick_interval(speed: u8) -> Option<Duration> {
    match speed {
        0 => None,
        1 => Some(Duration::from_millis(500)), // 2 ticks/s
        2 => Some(Duration::from_millis(100)), // 10 ticks/s
        3 => Some(Duration::from_millis(20)),  // 50 ticks/s
        _ => Some(Duration::ZERO),
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClientMsg {
    SetSpeed {
        level: u8,
        req: Option<u64>,
    },
    Save {
        req: Option<u64>,
    },
    AgentDetail {
        id: u32,
        req: Option<u64>,
    },
    BusinessDetail {
        id: u32,
        req: Option<u64>,
    },
    ContractDetail {
        id: u32,
        req: Option<u64>,
    },
    QueueCommand {
        command: PlayerCommand,
        req: Option<u64>,
    },
}

enum SimMsg {
    /// A new client's outbound queue: register for pushes and send it the
    /// current snapshot immediately.
    Register(Sender<String>),
    /// A parsed client request plus the requester's outbound queue for
    /// the reply.
    Client(ClientMsg, Sender<String>),
}

fn reply(req: Option<u64>, result: Result<Value, String>) -> Option<String> {
    let req = req?;
    let body = match result {
        Ok(data) => json!({"kind": "reply", "req": req, "ok": true, "data": data}),
        Err(error) => json!({"kind": "reply", "req": req, "ok": false, "error": error}),
    };
    Some(body.to_string())
}

/// Start serving on 127.0.0.1. Returns once the listener is bound; the
/// sim and accept threads keep running in the background.
pub fn start(cfg: ServeConfig) -> io::Result<ServeHandle> {
    let listener = TcpListener::bind(("127.0.0.1", cfg.port))?;
    let port = listener.local_addr()?.port();
    let (sim_tx, sim_rx) = channel::<SimMsg>();

    let world_cfg = WorldConfig {
        master_seed: cfg.seed,
        population: cfg.population,
        hash_every: cfg.hash_every,
    };
    let save_dir = cfg.save_dir.clone();
    std::thread::Builder::new()
        .name("marketborn-sim".into())
        .spawn(move || sim_thread(world_cfg, save_dir, sim_rx))?;

    let accept_thread = std::thread::Builder::new()
        .name("marketborn-accept".into())
        .spawn(move || accept_loop(listener, sim_tx))?;

    Ok(ServeHandle {
        port,
        accept_thread,
    })
}

fn accept_loop(listener: TcpListener, sim_tx: Sender<SimMsg>) {
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                warn!("accept failed: {e}");
                continue;
            }
        };
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "?".into());
        let socket = match tungstenite::accept(stream) {
            Ok(ws) => ws,
            Err(e) => {
                warn!("websocket handshake with {peer} failed: {e}");
                continue;
            }
        };
        info!("client connected: {peer}");
        let (out_tx, out_rx) = channel::<String>();
        if sim_tx.send(SimMsg::Register(out_tx.clone())).is_err() {
            return; // sim thread gone; stop accepting
        }
        let sim_tx2 = sim_tx.clone();
        if let Err(e) = std::thread::Builder::new()
            .name(format!("marketborn-client-{peer}"))
            .spawn(move || client_thread(socket, out_rx, out_tx, sim_tx2))
        {
            warn!("could not spawn client thread: {e}");
        }
    }
}

/// One thread per client: pump the outbound queue and poll the socket
/// under a short read timeout — no locks, no split sockets.
fn client_thread(
    mut socket: tungstenite::WebSocket<TcpStream>,
    out_rx: Receiver<String>,
    out_tx: Sender<String>,
    sim_tx: Sender<SimMsg>,
) {
    if let Err(e) = socket.get_ref().set_read_timeout(Some(CLIENT_POLL)) {
        warn!("could not set client read timeout: {e}");
        return;
    }
    loop {
        // Outbound first: snapshots and replies queued by the sim thread.
        while let Ok(text) = out_rx.try_recv() {
            if socket
                .send(tungstenite::Message::Text(text.into()))
                .is_err()
            {
                return;
            }
        }
        match socket.read() {
            Ok(msg) => {
                if msg.is_close() {
                    return;
                }
                let Ok(text) = msg.to_text() else { continue };
                match serde_json::from_str::<ClientMsg>(text) {
                    Ok(parsed) => {
                        if sim_tx.send(SimMsg::Client(parsed, out_tx.clone())).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        // Echo the request id when the envelope is readable,
                        // so a driver's pending request fails instead of
                        // hanging.
                        let req = serde_json::from_str::<Value>(text)
                            .ok()
                            .and_then(|v| v.get("req").and_then(Value::as_u64));
                        let body = json!({
                            "kind": "reply",
                            "req": req,
                            "ok": false,
                            "error": format!("bad message: {e}"),
                        });
                        if socket
                            .send(tungstenite::Message::Text(body.to_string().into()))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            Err(tungstenite::Error::Io(e))
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                // Poll window elapsed; loop back to the outbound queue.
            }
            Err(_) => return,
        }
    }
}

fn snapshot_json(world: &World) -> Option<String> {
    let snap = WorldSnapshot::capture(world);
    match serde_json::to_value(&snap) {
        Ok(value) => Some(json!({"kind": "snapshot", "data": value}).to_string()),
        Err(e) => {
            error!("snapshot serialization failed: {e}");
            None
        }
    }
}

fn publish(clients: &mut Vec<Sender<String>>, text: &str) {
    clients.retain(|c| c.send(text.to_string()).is_ok());
}

fn sim_thread(cfg: WorldConfig, save_dir: PathBuf, rx: Receiver<SimMsg>) {
    let mut world = World::from_config(cfg);
    info!(
        "world ready: seed {}, {} agents, {} businesses",
        world.state.config.master_seed,
        world.state.agents.len(),
        world.state.businesses.len()
    );
    let mut clients: Vec<Sender<String>> = Vec::new();
    let mut speed: u8 = 1;
    let mut next_tick = Instant::now();
    let mut last_snapshot = Instant::now();

    let handle = |msg: SimMsg,
                  world: &mut World,
                  clients: &mut Vec<Sender<String>>,
                  speed: &mut u8,
                  next_tick: &mut Instant| {
        match msg {
            SimMsg::Register(out) => {
                if let Some(snap) = snapshot_json(world) {
                    let _ = out.send(snap);
                }
                clients.push(out);
            }
            SimMsg::Client(cmsg, out) => {
                let response = match cmsg {
                    ClientMsg::SetSpeed { level, req } => {
                        *speed = level.min(4);
                        *next_tick = Instant::now();
                        info!("speed set to {}", *speed);
                        reply(req, Ok(Value::Null))
                    }
                    ClientMsg::Save { req } => {
                        let path = save_dir.join("quicksave.mbsave");
                        let result = sim_persist::save(world, &path)
                            .map(|_| Value::String(path.display().to_string()))
                            .map_err(|e| format!("save failed: {e}"));
                        reply(req, result)
                    }
                    ClientMsg::AgentDetail { id, req } => {
                        let detail = sim_core::AgentDetail::capture(world, sim_core::AgentId(id))
                            .and_then(|d| serde_json::to_value(&d).ok())
                            .unwrap_or(Value::Null);
                        reply(req, Ok(detail))
                    }
                    ClientMsg::BusinessDetail { id, req } => {
                        let detail =
                            sim_core::BusinessDetail::capture(world, sim_core::BusinessId(id))
                                .and_then(|d| serde_json::to_value(&d).ok())
                                .unwrap_or(Value::Null);
                        reply(req, Ok(detail))
                    }
                    ClientMsg::ContractDetail { id, req } => {
                        let detail =
                            sim_core::ContractDetail::capture(world, sim_core::ContractId(id))
                                .and_then(|d| serde_json::to_value(&d).ok())
                                .unwrap_or(Value::Null);
                        reply(req, Ok(detail))
                    }
                    ClientMsg::QueueCommand { command, req } => {
                        let at = world.state.tick + 1;
                        let result = world
                            .queue_command(at, command)
                            .map(|seq| json!({"seq": seq, "tick": at}))
                            .map_err(|e| e.to_string());
                        reply(req, result)
                    }
                };
                if let Some(text) = response {
                    let _ = out.send(text);
                }
                // Every handled message refreshes all clients — a driver
                // sees its effect without waiting out the throttle.
                if let Some(snap) = snapshot_json(world) {
                    publish(clients, &snap);
                }
            }
        }
    };

    loop {
        match tick_interval(speed) {
            None => match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    handle(msg, &mut world, &mut clients, &mut speed, &mut next_tick);
                    last_snapshot = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            },
            Some(interval) => {
                while let Ok(msg) = rx.try_recv() {
                    handle(msg, &mut world, &mut clients, &mut speed, &mut next_tick);
                    last_snapshot = Instant::now();
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
                    if let Some(snap) = snapshot_json(&world) {
                        publish(&mut clients, &snap);
                    }
                    last_snapshot = Instant::now();
                }
            }
        }
    }
}
