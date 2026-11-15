//! Long computations with progress and cancellation: optimization and Monte Carlo tolerancing. The desktop runs them
//! on a worker thread; the CLI and MCP run them to the end.

use std::sync::atomic::{AtomicBool, Ordering};

use instaoptics_core::LensSystem;
use instaoptics_core::tolerance::{self, ToleranceSettings};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeOutcome {
    pub system: LensSystem,
    pub initial_merit: f64,
    pub final_merit: f64,
    pub cycles: usize,
    pub variables: usize,
    pub stopped: bool,
    /// Merit after each cycle, starting with the initial merit.
    pub history: Vec<f64>,
}

/// Damped least squares, one step at a time so progress can be shown and the run stopped. `progress` receives the
/// cycle number, its merit and the current system.
pub fn optimize(system: &LensSystem, cycles: usize, cancel: &AtomicBool, mut progress: impl FnMut(usize, f64, &LensSystem)) -> Result<OptimizeOutcome, String> {
    system.validate().map_err(|e| e.0)?;
    let mut current = system.clone();
    let (mut damping, mut stalled, mut done) = (1e-3, 0, 0);
    let (mut initial, mut last, mut variables) = (f64::NAN, f64::NAN, 0);
    let mut history = Vec::new();
    while done < cycles && !cancel.load(Ordering::Relaxed) {
        let step = instaoptics_core::optimize::iterate(&current, damping);
        variables = step.variables;
        if variables == 0 {
            return Err("No variables: mark radii, thicknesses, conics or aspheric terms as variable in the Lens Data editor".into());
        }
        if !step.previous.is_finite() {
            return Err("The merit function cannot be evaluated for this system".into());
        }
        if initial.is_nan() {
            initial = step.previous;
            history.push(initial);
        }
        last = step.merit;
        if !step.improved {
            last = step.previous;
            break;
        }
        done += 1;
        damping = step.damping;
        history.push(step.merit);
        stalled = if (step.previous - step.merit) / step.previous < 1e-6 { stalled + 1 } else { 0 };
        current = step.system;
        progress(done, step.merit, &current);
        if stalled >= 3 {
            break;
        }
    }
    Ok(OptimizeOutcome { system: current, initial_merit: initial, final_merit: last, cycles: done, variables, stopped: cancel.load(Ordering::Relaxed), history })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonteCarloOutcome {
    pub nominal: f64,
    /// RMS spot radius of each trial (mm).
    pub values: Vec<f64>,
    pub total: usize,
}

/// Monte Carlo in chunks so progress streams and the run can stop; trials do not depend on the chunking.
pub fn monte_carlo(system: &LensSystem, settings: &ToleranceSettings, cancel: &AtomicBool, mut progress: impl FnMut(&MonteCarloOutcome)) -> Result<MonteCarloOutcome, String> {
    system.validate().map_err(|e| e.0)?;
    let total = settings.trials.clamp(1, 100_000);
    let mut out = MonteCarloOutcome { nominal: tolerance::nominal(system, settings), values: Vec::with_capacity(total), total };
    while out.values.len() < total && !cancel.load(Ordering::Relaxed) {
        let count = (total / 20).max(50).min(total - out.values.len());
        out.values.extend(tolerance::monte_carlo(system, settings, out.values.len(), count));
        progress(&out);
    }
    Ok(out)
}
