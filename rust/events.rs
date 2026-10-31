//! Private event-only CDP connection for chromium/json-event-compatibility.
//!
//! Terrane's document runtime enables serde_json/arbitrary_precision. Cargo
//! unifies that feature into headless_chrome, whose nested tagged/untagged event
//! deserializer then rejects fractional timestamps. A minimal Page.loadEventFired
//! with timestamp 123.456 reproduces the rejection. Commands still work, but the
//! high-level event listeners silently receive no such events.
//!
//! Observe the same local target through a flattened CDP session and parse events
//! as JSON values, without the incompatible enum buffering. This is a dependency
//! compatibility boundary, NOT clone avoidance or public raw-CDP functionality.
//! The source-owned Browser controls process lifetime; this thread owns only its
//! socket and bounded sender and exits on target/browser closure or disconnection.
//! The upstream listener API cannot replace its parser; keep this socket/thread
//! workaround together at the private browser boundary, not in public source API.
//!
//! Remove this module and direct tungstenite dependency when the upstream event
//! parser works with exact-number JSON features, proven by fractional-timestamp
//! events and the real-browser console/network regressions. Never weaken Terrane
//! document-number semantics to make a browser dependency deserialize events.

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use serde_json::{Value, json};
use std::{
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

// Normalize only the documented evidence; this is not a waterfall/HAR recorder.
fn record(message: &Value) -> Option<Value> {
    let parameters = message.get("params")?;
    Some(match message.get("method")?.as_str()? {
        "Runtime.consoleAPICalled" => json!({
            "kind":"console", "level":parameters["type"],
            "message":parameters["args"].as_array().map(|args| args.iter().map(|arg| {
                arg.get("value").map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
                    .or_else(|| arg["description"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| arg["type"].as_str().unwrap_or("undefined").to_owned())
            }).collect::<Vec<_>>().join(" ")).unwrap_or_default(),
            "timestamp":parameters["timestamp"]
        }),
        "Runtime.exceptionThrown" => json!({
            "kind":"javascript-error",
            "message":parameters["exceptionDetails"]["exception"].get("description").unwrap_or(&parameters["exceptionDetails"]["text"]),
            "timestamp":parameters["timestamp"]
        }),
        "Network.requestWillBeSent" => json!({
            "kind":"request", "url":parameters["request"]["url"],
            "request-id":parameters["requestId"], "timestamp":parameters["timestamp"]
        }),
        "Network.responseReceived" => json!({
            "kind":"response", "url":parameters["response"]["url"],
            "status":parameters["response"]["status"], "resource-type":parameters["type"],
            "request-id":parameters["requestId"], "timestamp":parameters["timestamp"]
        }),
        "Network.loadingFailed" => json!({
            "kind":"request-failed", "message":parameters["errorText"],
            "request-id":parameters["requestId"], "timestamp":parameters["timestamp"]
        }),
        _ => return None,
    })
}

fn emit(message: &Value, sender: &Sender<String>) -> bool {
    let Some(record) = record(message) else {
        return true;
    };
    // Diagnostic capture must never block the browser on a full queue. A full
    // snapshot is marked possibly incomplete by the diagnostics operation.
    !matches!(
        sender.try_send(record.to_string()),
        Err(TrySendError::Disconnected(_))
    )
}

// Browser CDP is local plaintext WS. Read timeouts allow setup deadlines without
// busy waiting; the worker also terminates when Chrome closes its connection.
fn read(socket: &mut Socket) -> Result<Option<Value>, String> {
    match socket.read() {
        Ok(Message::Text(text)) => serde_json::from_str(text.as_str())
            .map(Some)
            .map_err(|e| e.to_string()),
        Ok(Message::Binary(bytes)) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
        Ok(Message::Close(_)) => Err("browser diagnostic stream closed".into()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn call(
    socket: &mut Socket,
    id: u64,
    method: &str,
    parameters: Value,
    session: Option<&str>,
    sender: &Sender<String>,
    deadline: Instant,
) -> Result<Value, String> {
    let mut command = json!({"id":id,"method":method,"params":parameters});
    if let Some(session) = session {
        command["sessionId"] = json!(session);
    }
    socket
        .send(Message::Text(command.to_string().into()))
        .map_err(|e| e.to_string())?;
    loop {
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out configuring browser diagnostics: {method}"
            ));
        }
        let Some(message) = read(socket)? else {
            continue;
        };
        if message["id"].as_u64() == Some(id) {
            if let Some(error) = message.get("error") {
                return Err(format!("browser diagnostic setup failed: {error}"));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| "browser diagnostic setup returned no result".into());
        }
        if session.is_some() && message["sessionId"].as_str() == session {
            emit(&message, sender);
        }
    }
}

pub fn capture(
    url: &str,
    target: &str,
    capacity: usize,
    timeout_ms: u64,
) -> Result<Receiver<String>, String> {
    let (mut socket, _) = tungstenite::connect(url).map_err(|e| e.to_string())?;
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .map_err(|e| e.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_millis(timeout_ms)))
                .map_err(|e| e.to_string())?;
        }
        _ => return Err("expected the local browser's plaintext diagnostic WebSocket".into()),
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let (sender, receiver) = bounded(capacity);
    let attached = call(
        &mut socket,
        1,
        "Target.attachToTarget",
        json!({"targetId":target,"flatten":true}),
        None,
        &sender,
        deadline,
    )?;
    let session = attached["sessionId"]
        .as_str()
        .ok_or("diagnostic target attachment returned no session")?
        .to_owned();
    call(
        &mut socket,
        2,
        "Page.enable",
        json!({}),
        Some(&session),
        &sender,
        deadline,
    )?;
    call(
        &mut socket,
        3,
        "Network.enable",
        json!({}),
        Some(&session),
        &sender,
        deadline,
    )?;
    call(
        &mut socket,
        4,
        "Runtime.enable",
        json!({}),
        Some(&session),
        &sender,
        deadline,
    )?;
    std::thread::Builder::new()
        .name("chromium-diagnostics".into())
        .spawn(move || {
            loop {
                match read(&mut socket) {
                    Ok(Some(message)) => {
                        if message["method"] == "Target.detachedFromTarget"
                            && message["params"]["sessionId"].as_str() == Some(session.as_str())
                        {
                            break;
                        }
                        if message["sessionId"].as_str() == Some(session.as_str())
                            && !emit(&message, &sender)
                        {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = sender
                            .try_send(json!({"kind":"capture-error","message":error}).to_string());
                        break;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(receiver)
}
