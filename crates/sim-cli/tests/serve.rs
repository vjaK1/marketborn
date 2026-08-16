//! The websocket transport, driven end to end by a real client: connect,
//! receive the snapshot push, change speed, watch the world advance,
//! queue a policy command, fetch an inspector detail, save to disk.

use serde_json::{json, Value};
use sim_cli::serve::{start, ServeConfig};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::WebSocket;

type Client = WebSocket<MaybeTlsStream<TcpStream>>;

fn connect(port: u16) -> Client {
    let (socket, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}")).expect("connect");
    if let MaybeTlsStream::Plain(s) = socket.get_ref() {
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    }
    socket
}

fn read_json(socket: &mut Client) -> Value {
    loop {
        let msg = socket.read().expect("read frame");
        if let Ok(text) = msg.to_text() {
            if !text.is_empty() {
                return serde_json::from_str(text).expect("valid JSON frame");
            }
        }
    }
}

/// Skip snapshots until the reply with `req` arrives.
fn read_reply(socket: &mut Client, req: u64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let v = read_json(socket);
        if v["kind"] == "reply" && v["req"] == json!(req) {
            return v;
        }
    }
    panic!("no reply for req {req} within the deadline");
}

/// Skip frames until a snapshot satisfies `pred`.
fn wait_snapshot(socket: &mut Client, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let v = read_json(socket);
        if v["kind"] == "snapshot" && pred(&v["data"]) {
            return v;
        }
    }
    panic!("no matching snapshot within the deadline");
}

fn send(socket: &mut Client, body: Value) {
    socket
        .send(tungstenite::Message::Text(body.to_string().into()))
        .expect("send");
}

#[test]
fn the_websocket_transport_speaks_the_full_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let handle = start(ServeConfig {
        seed: 42,
        population: 29,
        hash_every: 50,
        port: 0, // ephemeral
        save_dir: dir.path().to_path_buf(),
        load: None,
    })
    .expect("bind");
    let mut ws = connect(handle.port);

    // A snapshot arrives on connect, unprompted.
    let first = wait_snapshot(&mut ws, |_| true);
    assert!(
        first["data"]["stats"]["population"] == json!(29),
        "the snapshot carries the world: {first}"
    );

    // Full speed: the world advances and the pushes keep coming.
    send(&mut ws, json!({"kind": "set_speed", "level": 4, "req": 1}));
    assert_eq!(read_reply(&mut ws, 1)["ok"], json!(true));
    wait_snapshot(&mut ws, |d| d["tick"].as_u64().unwrap_or(0) > 0);

    // Pause for a deterministic tail.
    send(&mut ws, json!({"kind": "set_speed", "level": 0, "req": 2}));
    assert_eq!(read_reply(&mut ws, 2)["ok"], json!(true));

    // A policy command queues at the next tick boundary.
    send(
        &mut ws,
        json!({
            "kind": "queue_command",
            "command": {"SetSalesTax": {"rate_bp": 500}},
            "req": 3
        }),
    );
    let queued = read_reply(&mut ws, 3);
    assert_eq!(queued["ok"], json!(true));
    assert!(queued["data"]["tick"].as_u64().unwrap() > 0);

    // A malformed command is refused with an error, not a hang.
    send(
        &mut ws,
        json!({
            "kind": "queue_command",
            "command": {"NoSuchLever": {}},
            "req": 4
        }),
    );
    let refused = read_reply(&mut ws, 4);
    assert_eq!(refused["ok"], json!(false));

    // The inspector's on-demand detail protocol.
    send(&mut ws, json!({"kind": "agent_detail", "id": 0, "req": 5}));
    let detail = read_reply(&mut ws, 5);
    assert_eq!(detail["ok"], json!(true));
    assert!(
        detail["data"].is_object(),
        "agent 0 must have a detail payload: {detail}"
    );

    // Save writes where the server was told to.
    send(&mut ws, json!({"kind": "save", "req": 6}));
    let saved = read_reply(&mut ws, 6);
    assert_eq!(saved["ok"], json!(true), "{saved}");
    assert!(dir.path().join("quicksave.mbsave").exists());

    // --- Named slots: save, advance, load back — the world rewinds. ---
    send(&mut ws, json!({"kind": "save", "slot": "alpha", "req": 7}));
    assert_eq!(read_reply(&mut ws, 7)["ok"], json!(true));
    let tick_saved = wait_snapshot(&mut ws, |_| true)["data"]["tick"]
        .as_u64()
        .unwrap();
    send(&mut ws, json!({"kind": "set_speed", "level": 4, "req": 8}));
    assert_eq!(read_reply(&mut ws, 8)["ok"], json!(true));
    wait_snapshot(&mut ws, |d| {
        d["tick"].as_u64().unwrap_or(0) > tick_saved + 5
    });
    send(&mut ws, json!({"kind": "set_speed", "level": 0, "req": 9}));
    assert_eq!(read_reply(&mut ws, 9)["ok"], json!(true));
    send(&mut ws, json!({"kind": "load", "slot": "alpha", "req": 10}));
    let loaded = read_reply(&mut ws, 10);
    assert_eq!(loaded["ok"], json!(true), "{loaded}");
    assert_eq!(loaded["data"]["tick"], json!(tick_saved));
    wait_snapshot(&mut ws, |d| d["tick"] == json!(tick_saved));

    // The slot listing knows both saves; a hostile slot name is refused.
    send(&mut ws, json!({"kind": "list_saves", "req": 11}));
    let listing = read_reply(&mut ws, 11);
    assert_eq!(listing["ok"], json!(true));
    let slots: Vec<String> = listing["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["slot"].as_str().unwrap().to_string())
        .collect();
    assert!(slots.contains(&"alpha".to_string()), "{slots:?}");
    assert!(slots.contains(&"quicksave".to_string()), "{slots:?}");
    send(
        &mut ws,
        json!({"kind": "save", "slot": "../evil", "req": 12}),
    );
    assert_eq!(read_reply(&mut ws, 12)["ok"], json!(false));

    // A second client gets its own snapshot immediately.
    let mut ws2 = connect(handle.port);
    wait_snapshot(&mut ws2, |d| d["stats"]["population"] == json!(29));
}
