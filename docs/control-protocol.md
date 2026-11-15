# Control protocol

`instaoptics --control PORT` (or `INSTAOPTICS_CONTROL_PORT`) listens on `127.0.0.1:PORT` for JSON lines, one request and one reply per line:

```json
{"id": 1, "method": "engine.execute", "params": {"command": "surface.set", "params": {"surface": 2, "thickness": 5}}, "token": "…"}
{"id": 1, "ok": true, "result": true}
```

Every request carries the session token. Loopback TCP cannot tell local users apart, so the app writes a random token to an owner-only file, `instaoptics-control-PORT.token` in `$XDG_RUNTIME_DIR` (else `%LOCALAPPDATA%` or `~/.cache`), and refuses requests without it. `instaoptics-cli mcp --connect` reads it automatically. Errors come back as `{"id": …, "ok": false, "error": "…"}`.

| Method | Params | What it does |
|---|---|---|
| `engine.execute` | `{command, params}` | Run a command, exactly as the menus do |
| `engine.commands` | | The command catalog |
| `ui.windows` | | Window ids and titles |
| `ui.open` | `{window}` | Open or focus a window |
| `ui.screenshot` | `{path?}` | PNG of the window; `pngBase64` without a path |
| `ui.resize` | `{width, height}` | Resize the window |
| `ui.language` | `{language}` | `system`, `en`, `zh-CN`, `zh-TW`, `ja`, `ko`, `de`, `fr`, `es` or `ru` |
| `ui.state` | | Theme, open windows, selected surface, file and dirty state |
| `ping` | | `"pong"` |
| `control.methods` | | This list |
