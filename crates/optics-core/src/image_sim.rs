//! Image simulation: renders a scene through the lens onto a sensor by splatting traced ray offsets.
//! The scene is the ideal (paraxial, distortion-free) image on the sensor. Each source pixel sends its energy along
//! the rays of the two nearest field samples, rotated to the pixel's azimuth. This captures blur, distortion, lateral
//! colour, vignetting and cos⁴ fall-off; a Gaussian approximation of the Airy core adds diffraction.

use rayon::prelude::*;
use serde::Deserialize;

use crate::analysis::hexapolar;
use crate::paraxial::{paraxial_data, trace as paraxial_trace};
use crate::system::{LensError, LensSystem};
use crate::trace::TraceContext;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationSettings {
    /// Sensor pixel pitch in µm; 0 fits the largest field to the image half-diagonal.
    #[serde(default)]
    pub pixel_pitch: f64,
    #[serde(default = "default_rings")]
    pub rings: usize,
    #[serde(default = "default_field_samples")]
    pub field_samples: usize,
    #[serde(default = "default_true")]
    pub diffraction: bool,
    /// Gamma used to linearize the 8-bit input.
    #[serde(default = "default_gamma")]
    pub gamma: f64,
}

fn default_rings() -> usize {
    6
}
fn default_field_samples() -> usize {
    32
}
fn default_true() -> bool {
    true
}
fn default_gamma() -> f64 {
    2.2
}

/// Paraxial image height per unit tan(field angle).
fn height_per_tan(system: &LensSystem, entrance_pupil_z: f64) -> Result<f64, LensError> {
    let n = system.medium_indices(system.primary_wavelength())?;
    let last = system.last();
    let chief = paraxial_trace(system, &n, -entrance_pupil_z, 1.0);
    Ok(chief.heights[last] + chief.angles[last] * system.surfaces[last].thickness)
}

/// Sensor pitch (µm) that puts the largest field at the image half-diagonal.
pub fn fitting_pitch(system: &LensSystem, width: usize, height: usize) -> Result<f64, LensError> {
    system.validate()?;
    let system = &crate::paraxial::resolved(system)?;
    let paraxial = paraxial_data(system)?;
    let half_diagonal = (width as f64).hypot(height as f64) / 2.0;
    Ok((height_per_tan(system, paraxial.entrance_pupil_z)? * system.max_field().to_radians().tan()).abs() / half_diagonal * 1000.0)
}

struct ChannelKernels {
    /// Ideal image height of each field sample (mm).
    heights: Vec<f64>,
    /// Per sample: (dx, dy) in mm relative to the ideal point, for a field along +y.
    offsets: Vec<Vec<[f32; 2]>>,
    /// Per-ray weight per sample: cos⁴θ / rays in the pupil (vignetted rays are simply missing).
    weights: Vec<f64>,
    airy_sigma: f64,
}

/// Red, green and blue channels use the longest, primary and shortest wavelengths.
fn channel_wavelengths(system: &LensSystem) -> [f64; 3] {
    let longest = system.wavelengths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let shortest = system.wavelengths.iter().cloned().fold(f64::INFINITY, f64::min);
    [longest, system.primary_wavelength(), shortest]
}

fn prepare_kernels(system: &LensSystem, settings: &SimulationSettings, max_height: f64) -> Result<Vec<ChannelKernels>, LensError> {
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    let scale = height_per_tan(system, paraxial.entrance_pupil_z)?;
    let field_top = scale.abs() * system.max_field().to_radians().tan();
    let top = field_top.min(max_height);
    let pupil = hexapolar(settings.rings.clamp(1, 20));
    let samples = settings.field_samples.clamp(1, 256);
    channel_wavelengths(system).into_par_iter().map(|wavelength| {
        let n = system.medium_indices(wavelength)?;
        let mut kernels = ChannelKernels { heights: Vec::new(), offsets: Vec::new(), weights: Vec::new(), airy_sigma: 0.42 * wavelength * 1e-3 * paraxial.working_f_number };
        for j in 0..samples {
            let height = if samples == 1 { 0.0 } else { top * j as f64 / (samples - 1) as f64 };
            let angle = (height / scale.abs()).atan();
            let ideal = scale * angle.tan();
            let mut list = Vec::with_capacity(pupil.len());
            for &[px, py] in &pupil {
                let ray = context.trace_ray(angle.to_degrees(), px, py, wavelength, &n);
                if !ray.ok() {
                    continue;
                }
                let end = ray.end();
                // An inverted image is the upright one rotated by 180°, so express offsets for an upright ideal image.
                let sign = if scale < 0.0 { -1.0 } else { 1.0 };
                list.push([(sign * end[0]) as f32, (sign * (end[1] - ideal)) as f32]);
            }
            kernels.heights.push(ideal.abs());
            kernels.offsets.push(list);
            kernels.weights.push(angle.cos().powi(4) / pupil.len() as f64);
        }
        Ok(kernels)
    }).collect()
}

fn gaussian_blur(channel: &mut [f32], width: usize, height: usize, sigma: f64) {
    if sigma < 0.3 {
        return;
    }
    let radius = (sigma * 3.0).ceil() as isize;
    let kernel: Vec<f32> = (-radius..=radius).map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp() as f32).collect();
    let sum: f32 = kernel.iter().sum();
    let mut temp = vec![0.0f32; channel.len()];
    temp.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let mut acc = 0.0;
            for k in -radius..=radius {
                let xx = (x as isize + k).clamp(0, width as isize - 1) as usize;
                acc += channel[y * width + xx] * kernel[(k + radius) as usize];
            }
            *out = acc / sum;
        }
    });
    channel.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let mut acc = 0.0;
            for k in -radius..=radius {
                let yy = (y as isize + k).clamp(0, height as isize - 1) as usize;
                acc += temp[yy * width + x] * kernel[(k + radius) as usize];
            }
            *out = acc / sum;
        }
    });
}

/// Splats one channel. Rows are split into bands, each accumulating into its own buffer, then summed.
fn splat(scene: &[f32], c: usize, width: usize, height: usize, kernel: &ChannelKernels, pitch: f64) -> Vec<f32> {
    let (cx, cy) = ((width as f64 - 1.0) / 2.0, (height as f64 - 1.0) / 2.0);
    let top = *kernel.heights.last().unwrap_or(&0.0);
    let samples = kernel.heights.len();
    let step = if samples > 1 { top / (samples - 1) as f64 } else { 1.0 };
    let bands = rayon::current_num_threads().max(1) * 2;
    let rows_per_band = height.div_ceil(bands);
    let partial: Vec<Vec<f32>> = (0..bands).into_par_iter().map(|band| {
        let mut target = vec![0.0f32; width * height];
        for v in band * rows_per_band..((band + 1) * rows_per_band).min(height) {
            for u in 0..width {
                let value = scene[(v * width + u) * 3 + c];
                if value == 0.0 {
                    continue;
                }
                let (x, y) = ((u as f64 - cx) * pitch, (cy - v as f64) * pitch);
                let h = x.hypot(y);
                if h > top * 1.0001 {
                    continue; // outside the designed field of view
                }
                let position = (h / step).min((samples - 1) as f64);
                let j0 = position.floor() as usize;
                let blend = position - j0 as f64;
                let (cos, sin) = if h > 0.0 { (y / h, x / h) } else { (1.0, 0.0) };
                for (j, share) in [(j0, 1.0 - blend), ((j0 + 1).min(samples - 1), blend)] {
                    if share <= 0.0 {
                        continue;
                    }
                    let weight = (value as f64 * kernel.weights[j] * share) as f32;
                    for &[dx, dy] in &kernel.offsets[j] {
                        // Rotate the meridional offset (field along +y) to this pixel's azimuth.
                        let (dx, dy) = (dx as f64, dy as f64);
                        let px = cx + (x + dx * cos + dy * sin) / pitch;
                        let py = cy - (y - dx * sin + dy * cos) / pitch;
                        let (x0, y0) = (px.floor(), py.floor());
                        if x0 < 0.0 || y0 < 0.0 || x0 >= (width - 1) as f64 || y0 >= (height - 1) as f64 {
                            continue;
                        }
                        let (fx, fy) = ((px - x0) as f32, (py - y0) as f32);
                        let i = y0 as usize * width + x0 as usize;
                        target[i] += weight * (1.0 - fx) * (1.0 - fy);
                        target[i + 1] += weight * fx * (1.0 - fy);
                        target[i + width] += weight * (1.0 - fx) * fy;
                        target[i + width + 1] += weight * fx * fy;
                    }
                }
            }
        }
        target
    }).collect();
    let mut out = vec![0.0f32; width * height];
    out.par_chunks_mut(width).enumerate().for_each(|(row, line)| {
        for part in &partial {
            for (o, p) in line.iter_mut().zip(&part[row * width..(row + 1) * width]) {
                *o += p;
            }
        }
    });
    out
}

pub struct Simulated {
    /// Linear RGB, 3 floats per pixel.
    pub data: Vec<f32>,
    /// Sensor pixel pitch actually used, µm.
    pub pitch: f64,
}

/// Renders an 8-bit RGBA scene through the lens.
pub fn simulate(system: &LensSystem, settings: &SimulationSettings, rgba: &[u8], width: usize, height: usize) -> Result<Simulated, LensError> {
    system.validate()?;
    let system = &crate::paraxial::resolved(system)?;
    if width < 2 || height < 2 || rgba.len() < width * height * 4 {
        return Err(LensError("The image is empty".into()));
    }
    let pitch_um = if settings.pixel_pitch > 0.0 { settings.pixel_pitch } else { fitting_pitch(system, width, height)? };
    if !(pitch_um > 0.0 && pitch_um.is_finite()) {
        return Err(LensError("Cannot fit the field of view to the image: set a pixel pitch".into()));
    }
    let pitch = pitch_um / 1000.0;
    let gamma = settings.gamma.max(0.1);
    let lut: Vec<f32> = (0..256).map(|v| (v as f64 / 255.0).powf(gamma) as f32).collect();
    let scene: Vec<f32> = rgba.par_chunks(4).take(width * height).flat_map_iter(|p| [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]]).collect();
    let half_diagonal = (width as f64).hypot(height as f64) / 2.0 * pitch;
    let kernels = prepare_kernels(system, settings, half_diagonal * 1.02)?;
    let channels: Vec<Vec<f32>> = kernels.iter().enumerate().map(|(c, kernel)| {
        let mut channel = splat(&scene, c, width, height, kernel, pitch);
        if settings.diffraction {
            gaussian_blur(&mut channel, width, height, kernel.airy_sigma / pitch);
        }
        channel
    }).collect();
    let mut data = vec![0.0f32; width * height * 3];
    data.par_chunks_mut(3).enumerate().for_each(|(i, pixel)| {
        for c in 0..3 {
            pixel[c] = channels[c][i];
        }
    });
    Ok(Simulated { data, pitch: pitch_um })
}
