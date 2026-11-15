//! instaoptics-cli: headless instaOptics. Results go to stdout as JSON, messages to stderr.
//! Exit codes: 0 success, 1 the command failed, 2 usage error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use instaoptics_engine::{Session, commands};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "instaoptics-cli", version, about = "Headless instaOptics: open, convert, analyze and render lenses, run commands, serve MCP")]
struct Cli {
    /// Folder of the .agf glass catalogs (default: the desktop app's catalog folder).
    #[arg(long, global = true, env = "INSTAOPTICS_CATALOG_DIR")]
    catalog_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Paraxial data and the lens of a file (.iol or .zmx).
    Info { file: PathBuf },
    /// Convert between instaOptics (.iol) and Zemax (.zmx) by the file extensions.
    Convert { input: PathBuf, output: PathBuf },
    /// Run an analysis and print its result, e.g. --request '{"kind":"mtf"}'.
    Analyze {
        file: PathBuf,
        #[arg(long, default_value = r#"{"kind":"spot"}"#)]
        request: String,
    },
    /// Render the layout or an analysis to PNG.
    Render {
        file: PathBuf,
        /// "layout", an analysis kind (spot, mtf, psf, …) or a request object.
        #[arg(long, default_value = "layout")]
        view: String,
        #[arg(long, short)]
        out: PathBuf,
        #[arg(long, default_value_t = 1200)]
        width: u32,
        #[arg(long, default_value_t = 800)]
        height: u32,
        #[arg(long, default_value = "light")]
        theme: String,
    },
    /// Run commands on a lens: each --cmd is `id` or `id {json}`; prints one JSON line per result.
    Run {
        file: Option<PathBuf>,
        #[arg(long = "cmd")]
        cmds: Vec<String>,
        /// A file with one command per line (blank lines and lines starting with # are skipped).
        #[arg(long)]
        script: Option<PathBuf>,
        /// Save the lens afterwards (.iol or .zmx).
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Print the command catalog.
    Commands,
    /// Serve MCP on stdio: headless, or bridged to the running app with --connect HOST:PORT.
    Mcp {
        #[arg(long)]
        connect: Option<String>,
    },
}

/// The desktop app's catalog folder, so both see the same loaded .agf catalogs.
fn default_catalog_dir() -> Option<PathBuf> {
    craft_settings::paths::data_dir("instaoptics").map(|d| d.join("catalogs"))
}

fn open(session: &mut Session, file: &Path) -> Result<(), String> {
    let zmx = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("zmx"));
    let command = if zmx { "import.zmx" } else { "open" };
    let reply = commands::execute(session, command, &json!({ "path": file }))?;
    for w in reply.get("warnings").and_then(Value::as_array).into_iter().flatten() {
        eprintln!("note: {}", w.as_str().unwrap_or_default());
    }
    Ok(())
}

fn save(session: &mut Session, file: &Path) -> Result<(), String> {
    let zmx = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("zmx"));
    commands::execute(session, if zmx { "export.zmx" } else { "saveas" }, &json!({ "path": file }))?;
    eprintln!("wrote {}", file.display());
    Ok(())
}

fn parse_cmd(line: &str) -> Result<(String, Value), String> {
    let line = line.trim();
    let (id, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let params = if rest.trim().is_empty() { json!({}) } else { serde_json::from_str(rest).map_err(|e| format!("{id}: parameters are not JSON: {e}"))? };
    Ok((id.to_string(), params))
}

/// Writes a result line. A reader that closes stdout early (`| head`) drops the rest of the output but not the work,
/// so a write error is ignored instead of panicking as `println!` would.
fn out(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stdout(), "{text}");
}

fn print(value: &Value) {
    out(&serde_json::to_string_pretty(value).unwrap_or_default());
}

fn run(cli: Cli) -> Result<(), String> {
    let mut session = Session::new(cli.catalog_dir.or_else(default_catalog_dir));
    match cli.command {
        Command::Info { file } => {
            open(&mut session, &file)?;
            print(&commands::execute(&mut session, "inspect", &json!({}))?);
        }
        Command::Convert { input, output } => {
            open(&mut session, &input)?;
            save(&mut session, &output)?;
        }
        Command::Analyze { file, request } => {
            open(&mut session, &file)?;
            let request: Value = serde_json::from_str(&request).map_err(|e| format!("--request is not JSON: {e}"))?;
            print(&commands::execute(&mut session, "analyze", &json!({ "request": request }))?);
        }
        Command::Render { file, view, out, width, height, theme } => {
            open(&mut session, &file)?;
            let view = serde_json::from_str::<Value>(&view).unwrap_or(Value::String(view));
            commands::execute(&mut session, "render", &json!({ "view": view, "width": width, "height": height, "theme": theme, "path": out }))?;
            eprintln!("wrote {}", out.display());
        }
        Command::Run { file, cmds, script, save: target } => {
            if let Some(file) = &file {
                open(&mut session, file)?;
            }
            let mut lines = Vec::new();
            if let Some(script) = script {
                let text = std::fs::read_to_string(&script).map_err(|e| format!("{}: {e}", script.display()))?;
                lines.extend(text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string));
            }
            lines.extend(cmds);
            for line in lines {
                let (id, params) = parse_cmd(&line)?;
                let result = commands::execute(&mut session, &id, &params).map_err(|e| format!("{id}: {e}"))?;
                if !result.is_null() {
                    out(&result.to_string());
                }
            }
            if let Some(target) = target {
                save(&mut session, &target)?;
            }
        }
        Command::Commands => print(&commands::catalog()),
        Command::Mcp { connect } => match connect {
            Some(address) => instaoptics_mcp::serve(&mut instaoptics_mcp::Bridge::connect(&address)?, true)?,
            None => instaoptics_mcp::serve(&mut instaoptics_mcp::Headless { session }, false)?,
        },
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
