//! JSON-lines control channel on 127.0.0.1:PORT. Each request carries the session token from an owner-only file;
//! requests are handed to the UI thread and answered there, so they act exactly like menu commands.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

pub struct Request {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Result<Value, String>>,
}

pub struct Server {
    pub rx: Receiver<Request>,
    pub port: u16,
}

fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded from the operating system's random source; two of them give 128 unguessable bits.
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)
}

pub fn start(port: u16, ctx: egui::Context) -> Result<Server, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("control port {port}: {e}"))?;
    let token = new_token();
    let token_path = instaoptics_engine::control_token_path(port);
    write_private(&token_path, &token).map_err(|e| format!("{}: {e}", token_path.display()))?;
    log::info!("control channel on 127.0.0.1:{port}, token in {}", token_path.display());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (tx, ctx, token) = (tx.clone(), ctx.clone(), token.clone());
            std::thread::spawn(move || serve(stream, tx, ctx, token));
        }
    });
    Ok(Server { rx, port })
}

fn serve(stream: TcpStream, tx: Sender<Request>, ctx: egui::Context, token: String) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            let _ = writeln!(writer, "{}", json!({ "id": null, "ok": false, "error": "not a JSON object" }));
            return;
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let reply = if request.get("token").and_then(Value::as_str) != Some(token.as_str()) {
            Err("unauthorized".to_string())
        } else if let Some(method) = request.get("method").and_then(Value::as_str) {
            let (rtx, rrx) = channel();
            let params = request.get("params").cloned().unwrap_or(json!({}));
            if tx.send(Request { method: method.to_string(), params, reply: rtx }).is_err() {
                return;
            }
            ctx.request_repaint();
            rrx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| Err("timeout".into()))
        } else {
            Err("method is required".into())
        };
        let out = match reply {
            Ok(result) => json!({ "id": id, "ok": true, "result": result }),
            Err(error) => json!({ "id": id, "ok": false, "error": error }),
        };
        if writeln!(writer, "{out}").is_err() {
            return;
        }
    }
}

pub const METHODS: &[(&str, &str)] = &[
    ("engine.execute", "{command, params}: run a command, as the menus do"),
    ("engine.commands", "the command catalog"),
    ("ui.windows", "window ids and titles"),
    ("ui.open", "{window}: open or focus a window"),
    ("ui.screenshot", "{path?}: PNG of the window; pngBase64 without a path"),
    ("ui.resize", "{width, height}: resize the window (logical points)"),
    ("ui.state", "theme, open windows, selected surface, file and dirty state"),
    ("ping", "pong"),
    ("control.methods", "this list"),
];
