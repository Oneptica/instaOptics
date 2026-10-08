//! Node-API bindings for the Electron compute process. Systems and results cross the boundary as JSON;
//! large numeric results will move to typed arrays as analyses are added.

use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Error, Result, Task};
use napi_derive::napi;
use optics_core::{LensSystem, glass, system};
use serde_json::json;

fn parse(system_json: &str) -> Result<LensSystem> {
    serde_json::from_str(system_json).map_err(|error| Error::from_reason(format!("Invalid lens system: {error}")))
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| Error::from_reason(error.to_string()))
}

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Paraxial data, apertures and layout geometry for a system.
#[napi]
pub fn overview(system_json: String, rays_per_field: u32) -> Result<String> {
    let lens = parse(&system_json)?;
    let overview = optics_core::overview(&lens, rays_per_field as usize).map_err(|error| Error::from_reason(error.0))?;
    to_json(&overview)
}

/// Work that runs on the libuv thread pool so the compute process keeps answering other requests.
pub struct Job(Option<Box<dyn FnOnce() -> Result<String> + Send>>);

impl Task for Job {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<String> {
        (self.0.take().expect("a job runs once"))()
    }

    fn resolve(&mut self, _env: Env, output: String) -> Result<String> {
        Ok(output)
    }
}

/// Runs one analysis off the JS thread; `request_json` is an `AnalysisRequest` such as {"kind":"spot","rings":6}.
#[napi(ts_return_type = "Promise<string>")]
pub fn analyze(system_json: String, request_json: String) -> AsyncTask<Job> {
    AsyncTask::new(Job(Some(Box::new(move || {
        let lens = parse(&system_json)?;
        let request: optics_core::AnalysisRequest =
            serde_json::from_str(&request_json).map_err(|error| Error::from_reason(format!("Invalid analysis request: {error}")))?;
        let result = optics_core::analyze(&lens, &request).map_err(|error| Error::from_reason(error.0))?;
        to_json(&result)
    }))))
}

/// One optimizer step off the JS thread; resolves to { system, merit, previous, damping, improved, variables }.
#[napi(ts_return_type = "Promise<string>")]
pub fn optimize_step(system_json: String, damping: f64) -> AsyncTask<Job> {
    AsyncTask::new(Job(Some(Box::new(move || {
        let lens = parse(&system_json)?;
        to_json(&optics_core::optimize::iterate(&lens, damping))
    }))))
}

fn tolerance_settings(json: &str) -> Result<optics_core::tolerance::ToleranceSettings> {
    serde_json::from_str(json).map_err(|error| Error::from_reason(format!("Invalid tolerance settings: {error}")))
}

/// Sensitivity table: each parameter at ±tolerance, with the RSS estimate.
#[napi(ts_return_type = "Promise<string>")]
pub fn tolerance_sensitivity(system_json: String, settings_json: String) -> AsyncTask<Job> {
    AsyncTask::new(Job(Some(Box::new(move || {
        let (lens, settings) = (parse(&system_json)?, tolerance_settings(&settings_json)?);
        lens.validate().map_err(|error| Error::from_reason(error.0))?;
        to_json(&optics_core::tolerance::sensitivity(&lens, &settings))
    }))))
}

/// Monte Carlo trials start..start+count; resolves to { nominal, values }.
#[napi(ts_return_type = "Promise<string>")]
pub fn tolerance_monte_carlo(system_json: String, settings_json: String, start: u32, count: u32) -> AsyncTask<Job> {
    AsyncTask::new(Job(Some(Box::new(move || {
        let (lens, settings) = (parse(&system_json)?, tolerance_settings(&settings_json)?);
        lens.validate().map_err(|error| Error::from_reason(error.0))?;
        let values = optics_core::tolerance::monte_carlo(&lens, &settings, start as usize, count as usize);
        to_json(&json!({ "nominal": optics_core::tolerance::nominal(&lens, &settings), "values": values }))
    }))))
}

/// Image simulation off the JS thread. The result is a little-endian buffer: f64 pixel pitch (µm), then linear RGB f32.
pub struct ImageJob(Option<Box<dyn FnOnce() -> Result<Vec<u8>> + Send>>);

impl Task for ImageJob {
    type Output = Vec<u8>;
    type JsValue = Buffer;

    fn compute(&mut self) -> Result<Vec<u8>> {
        (self.0.take().expect("a job runs once"))()
    }

    fn resolve(&mut self, _env: Env, output: Vec<u8>) -> Result<Buffer> {
        Ok(output.into())
    }
}

#[napi(ts_return_type = "Promise<Buffer>")]
pub fn simulate_image(system_json: String, settings_json: String, rgba: Buffer, width: u32, height: u32) -> AsyncTask<ImageJob> {
    let pixels: Vec<u8> = rgba.to_vec();
    AsyncTask::new(ImageJob(Some(Box::new(move || {
        let lens = parse(&system_json)?;
        let settings: optics_core::image_sim::SimulationSettings =
            serde_json::from_str(&settings_json).map_err(|error| Error::from_reason(format!("Invalid simulation settings: {error}")))?;
        let result = optics_core::image_sim::simulate(&lens, &settings, &pixels, width as usize, height as usize)
            .map_err(|error| Error::from_reason(error.0))?;
        let mut out = Vec::with_capacity(8 + result.data.len() * 4);
        out.extend_from_slice(&result.pitch.to_le_bytes());
        for value in result.data {
            out.extend_from_slice(&value.to_le_bytes());
        }
        Ok(out)
    }))))
}

/// Built-in sample systems as [{ id, system }].
#[napi]
pub fn samples() -> Result<String> {
    let list: Vec<_> = system::samples().into_iter().map(|(id, lens)| json!({ "id": id, "system": lens })).collect();
    to_json(&list)
}

/// Catalog glasses as [{ name, nd, vd }].
#[napi]
pub fn glass_catalog() -> Result<String> {
    let list: Vec<_> = glass::CATALOG.iter().map(|g| json!({ "name": g.name, "nd": g.nd, "vd": g.vd })).collect();
    to_json(&list)
}
