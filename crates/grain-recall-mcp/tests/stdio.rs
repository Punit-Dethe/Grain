//! Drives the built binary over stdio exactly as an AI app would.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{json, Value};

const MEETING: &str = "7d0c1d3a-5b8e-4f61-9a2c-3e4f5a6b7c8d";

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Client {
    fn start(data_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_grain-recall-mcp"))
            .arg("--data-dir")
            .arg(data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Self {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(10))
                .expect("server answered in time");
            let message: Value = serde_json::from_str(&line).expect("stdout is JSON-RPC only");
            if message["id"] == id {
                return message;
            }
        }
    }
}

fn data_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let meeting = dir.path().join("meetings").join(MEETING);
    std::fs::create_dir_all(meeting.join("transcripts")).unwrap();
    std::fs::write(dir.path().join("meetings").join(".mcp-enabled"), b"").unwrap();
    let manifest = json!({
        "version": 1, "id": MEETING, "title": "Roadmap sync", "created_unix_ms": 1_794_312_000_000u64,
        "state": "Recorded",
        "tracks": [{ "id": 0, "source": { "Microphone": { "device": "Mic" } }, "gaps": [],
            "chunks": [{ "seq": 0, "sample_rate": 16000, "channels": 1, "start_ns": 0,
                "frames": 160000, "sealed": true, "recovered": false }] }]
    });
    std::fs::write(meeting.join("manifest.json"), manifest.to_string()).unwrap();
    let words = |slot: i32, start: u64, text: &str| -> Vec<Value> {
        text.split(' ')
            .enumerate()
            .map(|(k, w)| {
                let at = start + k as u64 * 300;
                json!({ "text": w, "start_ms": at, "end_ms": at + 250, "speaker": { "Speaker": slot } })
            })
            .collect()
    };
    let mut all = words(0, 0, "Let us move the launch to March.");
    all.extend(words(1, 4_000, "Agreed, March works for marketing."));
    let revision = json!({
        "version": 1, "revision": 1, "session_id": MEETING, "attempt_id": "a",
        "created_unix_ms": 1_794_312_000_000u64, "diarizer": null, "asr": null,
        "speakers": [
            { "id": "t0-s0", "track": 0, "slot": 0, "label": "Speaker 1" },
            { "id": "t0-s1", "track": 0, "slot": 1, "label": "Speaker 2" }
        ],
        "tracks": [{ "track": 0, "status": "Complete", "text": "", "words": all, "activity": [] }]
    });
    std::fs::write(
        meeting.join("transcripts").join("000001.json"),
        revision.to_string(),
    )
    .unwrap();
    dir
}

#[test]
fn an_ai_app_can_list_search_and_read_meetings_over_stdio() {
    let dir = data_dir();
    let mut client = Client::start(dir.path());

    let init = client.request(
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "stdio-test", "version": "0" }
        }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "grain-recall");
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("Never invent"));
    client.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let tools = client.request(2, "tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);

    let found = client.request(
        3,
        "tools/call",
        json!({ "name": "search_meetings", "arguments": { "query": "march marketing" } }),
    );
    // Both lines match: "march" on the first, "marketing" right after it.
    let hits = found["result"]["structuredContent"]["hits"]
        .as_array()
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0]["meeting_id"], MEETING);
    assert_eq!(hits[1]["speaker"], "Speaker 2");
    assert_eq!(hits[1]["at"], "00:04");

    let page = client.request(
        4,
        "tools/call",
        json!({ "name": "get_transcript", "arguments": { "meeting_id": MEETING } }),
    );
    assert_eq!(
        page["result"]["structuredContent"]["text"],
        "[00:00] Speaker 1: Let us move the launch to March.\n[00:04] Speaker 2: Agreed, March works for marketing."
    );

    let resource = client.request(
        5,
        "resources/read",
        json!({ "uri": format!("grain://recall/meetings/{MEETING}") }),
    );
    assert!(resource["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("# Roadmap sync"));

    // Turning the switch off cuts a live session at the next call.
    std::fs::remove_file(dir.path().join("meetings").join(".mcp-enabled")).unwrap();
    let off = client.request(
        6,
        "tools/call",
        json!({ "name": "list_meetings", "arguments": {} }),
    );
    assert_eq!(off["result"]["isError"], true);

    // Closing stdin ends the server.
    drop(client.stdin.take());
    let status = client.child.wait().unwrap();
    assert!(status.success(), "exit status {status}");
}
