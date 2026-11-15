//! Background work. Every computation runs off the GUI thread; results come back tagged with the input they were
//! computed for, so a result that arrives after the lens changed is shown as the last valid one and recomputed.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use instaoptics_core::tolerance::Sensitivity;
use instaoptics_core::{AnalysisResult, Overview};
use instaoptics_engine::jobs::{MonteCarloOutcome, OptimizeOutcome};

use crate::panels::Panel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobKey {
    Overview,
    Analysis(Panel),
    BeamAnalytic,
    Optimize,
    Sensitivity,
    MonteCarlo,
    ImageSim,
}

pub enum Output {
    Overview(Result<Overview, String>),
    Analysis(Result<AnalysisResult, String>),
    /// Optimizer progress: cycle, merit, the current lens.
    OptimizeStep(usize, f64, instaoptics_core::LensSystem),
    Optimize(Result<OptimizeOutcome, String>),
    Sensitivity(Result<Sensitivity, String>),
    MonteCarlo(Result<MonteCarloOutcome, String>, bool),
    ImageSim(Result<(Vec<f32>, usize, usize, f64), String>),
}

pub struct Done {
    pub key: JobKey,
    pub tag: u64,
    pub ms: f64,
    pub output: Output,
}

pub struct Worker {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    ctx: egui::Context,
    /// The input tag of the job running for each key.
    running: HashMap<JobKey, u64>,
    cancel: HashMap<JobKey, Arc<AtomicBool>>,
}

/// A stable tag for a computation's input.
pub fn tag(parts: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// Sends progress from a running job.
#[derive(Clone)]
pub struct Progress {
    tx: Sender<Done>,
    ctx: egui::Context,
    key: JobKey,
    tag: u64,
}

impl Progress {
    pub fn send(&self, output: Output) {
        let _ = self.tx.send(Done { key: self.key, tag: self.tag, ms: 0.0, output });
        self.ctx.request_repaint();
    }
}

impl Worker {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, ctx, running: HashMap::new(), cancel: HashMap::new() }
    }

    pub fn is_running(&self, key: JobKey) -> bool {
        self.running.contains_key(&key)
    }

    pub fn running_tag(&self, key: JobKey) -> Option<u64> {
        self.running.get(&key).copied()
    }

    /// Starts a job unless one for the same key and input is already running. A job for the same key with another
    /// input keeps running; its result is ignored by tag when it arrives.
    pub fn spawn(&mut self, key: JobKey, tag: u64, job: impl FnOnce(&Progress, &AtomicBool) -> Output + Send + 'static) {
        if self.running.get(&key) == Some(&tag) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(old) = self.cancel.insert(key, cancel.clone()) {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.running.insert(key, tag);
        let progress = Progress { tx: self.tx.clone(), ctx: self.ctx.clone(), key, tag };
        std::thread::spawn(move || {
            let start = Instant::now();
            let output = job(&progress, &cancel);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let _ = progress.tx.send(Done { key, tag, ms, output });
            progress.ctx.request_repaint();
        });
    }

    pub fn cancel(&mut self, key: JobKey) {
        if let Some(c) = self.cancel.get(&key) {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Finished results and progress since the last frame.
    pub fn poll(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        while let Ok(done) = self.rx.try_recv() {
            let finished = !matches!(done.output, Output::OptimizeStep(..) | Output::MonteCarlo(_, false));
            if finished && self.running.get(&done.key) == Some(&done.tag) {
                self.running.remove(&done.key);
                self.cancel.remove(&done.key);
            }
            out.push(done);
        }
        out
    }
}
