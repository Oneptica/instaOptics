//! MCP server over stdio. It works on its own headless session (`instaoptics-cli mcp`) or forwards every call to the
//! running desktop app through its control channel (`instaoptics-cli mcp --connect 127.0.0.1:PORT`), so an agent can
//! edit the lens a person is watching.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use craft_jsonrpc::{Limits, RpcError};
use instaoptics_engine::{Session, commands};
use serde_json::{Value, json};

/// Something that answers control-channel methods: `engine.execute`, `engine.commands`, `ui.*`.
pub trait Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
}

pub struct Headless {
    pub session: Session,
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "engine.execute" => {
                let command = params.get("command").and_then(Value::as_str).ok_or("command is required")?.to_string();
                commands::execute(&mut self.session, &command, params.get("params").unwrap_or(&Value::Null))
            }
            "engine.commands" => Ok(commands::catalog()),
            "ping" => Ok(json!("pong")),
            m if m.starts_with("ui.") => Err(format!("{m} needs the desktop app: start it with --control PORT and use mcp --connect")),
            m => Err(format!("Unknown method {m}")),
        }
    }
}

/// A connection to the desktop app's control channel.
pub struct Bridge {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    token: String,
    next: u64,
}

impl Bridge {
    pub fn connect(address: &str) -> Result<Self, String> {
        let port: u16 = address.rsplit(':').next().and_then(|p| p.parse().ok()).ok_or_else(|| format!("{address} is not HOST:PORT"))?;
        let token_path = instaoptics_engine::control_token_path(port);
        let token = std::fs::read_to_string(&token_path).map_err(|e| format!("No control token at {} ({e}); is instaOptics running with --control {port}?", token_path.display()))?;
        let writer = TcpStream::connect(address).map_err(|e| format!("Cannot reach instaOptics at {address}: {e}"))?;
        let reader = BufReader::new(writer.try_clone().map_err(|e| e.to_string())?);
        Ok(Self { reader, writer, token: token.trim().to_string(), next: 1 })
    }
}

impl Backend for Bridge {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let line = json!({ "id": id, "method": method, "params": params, "token": self.token }).to_string();
        self.writer.write_all(format!("{line}\n").as_bytes()).map_err(|e| e.to_string())?;
        let mut reply = String::new();
        self.reader.read_line(&mut reply).map_err(|e| e.to_string())?;
        let reply: Value = serde_json::from_str(&reply).map_err(|e| format!("Bad reply from instaOptics: {e}"))?;
        if reply.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(reply.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(reply.get("error").and_then(Value::as_str).unwrap_or("instaOptics refused the request").to_string())
        }
    }
}

fn tools(bridged: bool) -> Value {
    let mut list = vec![
        json!({ "name": "execute", "description": "Run any instaOptics command with JSON parameters, e.g. {\"command\":\"surface.set\",\"params\":{\"surface\":2,\"thickness\":5}}. Surfaces are numbered from 1 as in the Lens Data editor. `list_commands` shows every command and its parameters.",
            "inputSchema": { "type": "object", "properties": { "command": { "type": "string" }, "params": { "type": "object" } }, "required": ["command"] } }),
        json!({ "name": "list_commands", "description": "The command catalog: id, label, menu, shortcut and parameters. `filter` matches part of an id or label.",
            "inputSchema": { "type": "object", "properties": { "filter": { "type": "string" } } } }),
        json!({ "name": "inspect", "description": "The open lens: every surface, aperture, fields, wavelengths, paraxial data (EFL, F/#, track), file and undo state.",
            "inputSchema": { "type": "object", "properties": {} } }),
        json!({ "name": "analyze", "description": "Run an analysis and return its numbers. `request` is e.g. {\"kind\":\"spot\"}, {\"kind\":\"mtf\",\"maxFrequency\":100}, {\"kind\":\"seidel\"}; missing parameters take their defaults (`execute` with `analyses` lists them).",
            "inputSchema": { "type": "object", "properties": { "request": { "type": "object" } }, "required": ["request"] } }),
        json!({ "name": "render", "description": "A PNG of the 2D layout (`view`: \"layout\") or of an analysis (`view`: \"spot\", \"mtf\", or a full request object). Returns an image.",
            "inputSchema": { "type": "object", "properties": { "view": {}, "width": { "type": "integer" }, "height": { "type": "integer" }, "theme": { "type": "string", "enum": ["dark", "light"] } } } }),
    ];
    if bridged {
        list.push(json!({ "name": "screenshot", "description": "A PNG of the instaOptics window.", "inputSchema": { "type": "object", "properties": {} } }));
        list.push(json!({ "name": "open_window", "description": "Open a window in the app, e.g. {\"window\":\"mtf\"}; `ui.windows` lists the ids.",
            "inputSchema": { "type": "object", "properties": { "window": { "type": "string" } }, "required": ["window"] } }));
    }
    Value::Array(list)
}

fn image_result(reply: &Value) -> Value {
    match reply.get("pngBase64").and_then(Value::as_str) {
        Some(data) => json!({ "content": [{ "type": "image", "data": data, "mimeType": "image/png" }], "isError": false }),
        None => craft_mcp::text_tool_result(&reply.to_string(), false),
    }
}

fn call_tool(backend: &mut dyn Backend, name: &str, args: Value) -> Result<Value, RpcError> {
    let exec = |backend: &mut dyn Backend, command: &str, params: Value| backend.call("engine.execute", json!({ "command": command, "params": params }));
    let result = match name {
        "execute" => {
            let command = args.get("command").and_then(Value::as_str).ok_or_else(|| RpcError::new(-32602, "command is required"))?.to_string();
            exec(backend, &command, args.get("params").cloned().unwrap_or(json!({})))
        }
        "list_commands" => backend.call("engine.commands", json!({})).map(|catalog| {
            let filter = args.get("filter").and_then(Value::as_str).unwrap_or("").to_lowercase();
            Value::Array(catalog.as_array().into_iter().flatten().filter(|c| c.to_string().to_lowercase().contains(&filter)).cloned().collect())
        }),
        "inspect" => exec(backend, "inspect", json!({})),
        "analyze" => {
            let request = args.get("request").cloned().ok_or_else(|| RpcError::new(-32602, "request is required"))?;
            exec(backend, "analyze", json!({ "request": request }))
        }
        "render" => return Ok(exec(backend, "render", args).map_or_else(|e| craft_mcp::text_tool_result(&e, true), |r| image_result(&r))),
        "screenshot" => return Ok(backend.call("ui.screenshot", json!({})).map_or_else(|e| craft_mcp::text_tool_result(&e, true), |r| image_result(&r))),
        "open_window" => backend.call("ui.open", args),
        _ => return Err(RpcError::new(-32602, format!("Unknown tool {name}"))),
    };
    Ok(match result {
        Ok(value) => craft_mcp::text_tool_result(&serde_json::to_string_pretty(&value).unwrap_or_default(), false),
        Err(e) => craft_mcp::text_tool_result(&e, true),
    })
}

/// Serves MCP on stdin/stdout until stdin closes.
pub fn serve(backend: &mut dyn Backend, bridged: bool) -> Result<(), String> {
    let mut session = craft_mcp::Session::new("instaoptics", env!("CARGO_PKG_VERSION"), craft_mcp::Policy::Compatible);
    session.set_instructions("instaOptics is an optical simulation program for sequential lens systems. Inspect the lens first, change it with `execute` (commands from `list_commands`), check results with `analyze` and look with `render`. Lengths are mm, wavelengths µm, angles degrees.".into());
    // Requests and notifications both reach the MCP session from separate callbacks.
    let mcp = std::cell::RefCell::new(session);
    let limits = Limits::default();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stdout().lock();
    loop {
        let line = match craft_jsonrpc::read_line(&mut input, limits.max_message_bytes) {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(()),
            Err(_) => {
                writeln!(out, "{}", craft_jsonrpc::parse_error_response()).map_err(|e| e.to_string())?;
                continue;
            }
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let reply = match craft_jsonrpc::parse_json(&line, limits) {
            Ok(value) => craft_jsonrpc::process(
                &value,
                limits,
                |request| {
                    if let Some(answer) = mcp.borrow_mut().request(request, false) {
                        return answer;
                    }
                    match request.method {
                        "tools/list" => Ok(craft_mcp::tool_list(tools(bridged))),
                        "tools/call" => {
                            let params = request.params.cloned().unwrap_or(Value::Null);
                            let (name, args) = craft_mcp::tool_call(&params)?;
                            call_tool(backend, name, args)
                        }
                        "resources/list" => Ok(json!({ "resources": [] })),
                        "prompts/list" => Ok(json!({ "prompts": [] })),
                        m => Err(RpcError::new(-32601, format!("Method not found: {m}"))),
                    }
                },
                |notification| mcp.borrow_mut().notification(notification),
            )
            .unwrap_or_else(|_| Some(craft_jsonrpc::parse_error_response())),
            Err(_) => Some(craft_jsonrpc::parse_error_response()),
        };
        if let Some(reply) = reply {
            writeln!(out, "{reply}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
}
