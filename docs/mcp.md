# MCP

`instaoptics-cli mcp` speaks the [Model Context Protocol](https://modelcontextprotocol.io) over stdio.

```sh
instaoptics-cli mcp                                   # its own lens, no window
instaoptics --control 7391 &                          # the desktop app with its control channel
instaoptics-cli mcp --connect 127.0.0.1:7391          # work on the lens open in the app
```

```json
{ "mcpServers": { "instaoptics": { "command": "instaoptics-cli", "args": ["mcp", "--connect", "127.0.0.1:7391"] } } }
```

| Tool | What it does |
|---|---|
| `execute {command, params}` | Any command from `list_commands`, e.g. `surface.set`, `system.set`, `variable`, `optimize`, `open`, `save` |
| `list_commands {filter?}` | Ids, labels, menus, shortcuts and parameters |
| `inspect` | Every surface, aperture, fields, wavelengths, paraxial data, file and undo state |
| `analyze {request}` | Analysis numbers, e.g. `{"kind":"spot"}`, `{"kind":"seidel"}` |
| `render {view, width?, height?, theme?}` | A PNG of the layout or an analysis, returned as an image |
| `screenshot`, `open_window {window}` | Only when connected to the app |

A tool that runs and fails returns `isError: true` with the message. A typical session: `inspect`, then `execute` with `surface.set` or `system.set`, then `analyze` to check the numbers and `render` to look.
