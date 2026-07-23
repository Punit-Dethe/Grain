use grain_sdk::{ClientHello, ClientRequest, HostCallResult, HostFrame, GRAIN_API_VERSION};
use serde_json::{json, Value};
use tungstenite::Message;

fn send(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    value: &impl serde::Serialize,
) {
    let text = serde_json::to_string(value).expect("serialize protocol frame");
    socket
        .send(Message::Text(text.into()))
        .expect("send protocol frame");
}

fn main() {
    let id = std::env::var("GRAIN_EXTENSION_ID").expect("Grain must spawn this companion");
    let token = std::env::var("GRAIN_EVENTS_TOKEN").expect("missing one-run token");
    let url =
        std::env::var("GRAIN_EVENTS_URL").unwrap_or_else(|_| "ws://127.0.0.1:7124".to_string());
    let activation: Value = std::env::var("GRAIN_ACTIVATION_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Value::Null);
    eprintln!("activation: {activation}");

    let (mut socket, _) = tungstenite::connect(&url).expect("connect to Grain");
    send(
        &mut socket,
        &ClientHello {
            token,
            client: id,
            grain_api: GRAIN_API_VERSION.to_string(),
        },
    );
    send(
        &mut socket,
        &HostFrame::Request(ClientRequest {
            id: 1,
            method: "storage.set".to_string(),
            params: json!({ "key": "companion-started", "value": true }),
        }),
    );

    while let Ok(message) = socket.read() {
        let Message::Text(text) = message else {
            if message.is_close() {
                break;
            }
            continue;
        };
        if let Ok(frame) = serde_json::from_str::<HostFrame>(&text) {
            match frame {
                HostFrame::Response(response) => {
                    eprintln!("host response {}: {:?}", response.id, response.err);
                }
                HostFrame::Call(call) => {
                    eprintln!("host call {}: {}", call.call_id, call.method);
                    if call.call_id != 0 {
                        send(
                            &mut socket,
                            &HostFrame::CallResult(HostCallResult {
                                call_id: call.call_id,
                                ok: Some(Value::Null),
                                err: None,
                            }),
                        );
                    }
                }
                _ => {}
            }
        } else {
            // DaemonEvents use their own mutually-exclusive JSON shapes.
            eprintln!("event: {text}");
        }
    }
}
