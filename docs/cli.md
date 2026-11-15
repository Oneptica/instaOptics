# instaoptics-cli

Headless instaOptics. `instaoptics-cli --help` and `instaoptics-cli help <subcommand>` describe every flag.

| Subcommand | What it does |
|---|---|
| `info FILE` | The lens, its paraxial data, file and history state as JSON |
| `convert IN OUT` | `.iol` and `.zmx` in either direction, chosen by extension |
| `analyze FILE --request JSON` | One analysis, e.g. `{"kind":"mtf","maxFrequency":100}`; missing parameters take their defaults |
| `render FILE --view VIEW --out PNG` | `layout`, an analysis kind (`spot`, `mtf`, `psf`, …) or a request object; `--width`, `--height`, `--theme light|dark` |
| `run [FILE] --cmd 'id {json}' … [--script FILE] [--save OUT]` | Commands in order; one JSON line per result that is not null |
| `commands` | The command catalog |
| `mcp [--connect HOST:PORT]` | MCP on stdio, see [mcp.md](mcp.md) |

Results go to stdout, messages (`note:`, `wrote …`, `error: …`) to stderr. Exit codes: 0 success, 1 the command failed, 2 usage error. `--catalog-dir` (or `INSTAOPTICS_CATALOG_DIR`) chooses the folder of `.agf` catalogs; by default it is the desktop app's.

A script file has one command per line, `id` or `id {json}`; blank lines and lines starting with `#` are skipped:

```
sample {"id":"cooke"}
variable {"surface":6,"parameter":"thickness"}
optimize {"cycles":30}
render {"view":"spot","path":"spot.png"}
```
