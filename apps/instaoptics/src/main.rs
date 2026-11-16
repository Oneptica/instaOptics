//! instaOptics desktop app.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "instaoptics", version, about = "instaOptics optical simulation")]
struct Args {
    /// A lens to open (.iol or .zmx).
    file: Option<PathBuf>,
    /// Serve the JSON control channel on 127.0.0.1:PORT (for scripts and `instaoptics-cli mcp --connect`).
    #[arg(long, env = "INSTAOPTICS_CONTROL_PORT")]
    control: Option<u16>,
}

struct App(instaoptics_ui_egui::Workbench);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn on_exit(&mut self) {
        self.0.on_exit();
    }
}

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../../../assets/icon.png")).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

fn main() -> eframe::Result {
    let args = Args::parse();
    let mut viewport = egui::ViewportBuilder::default().with_title("instaOptics").with_decorations(false).with_inner_size([1440.0, 900.0]).with_min_inner_size([800.0, 500.0]).with_app_id("instaoptics");
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("instaOptics", options, Box::new(move |cc| Ok(Box::new(App(instaoptics_ui_egui::Workbench::new(&cc.egui_ctx, args.file, args.control))))))
}
