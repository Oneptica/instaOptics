//! Physical optics propagation against analytic Gaussian beam results.

use optics_core::pop::{PopSettings, PopResult, Source, gaussian_beam, simulate};
use optics_core::system::{LensSystem, Surface, singlet};

fn settings(source: Source, samples: usize, aberrations: bool) -> PopSettings {
    PopSettings { source, wavelength: 0, samples, aberrations, apertures: true }
}

fn radius_at(result: &PopResult, z: f64) -> f64 {
    let s = result.slices.iter().min_by(|a, b| (a.z - z).abs().total_cmp(&(b.z - z).abs())).unwrap();
    0.5 * (s.w_x + s.w_y)
}

fn free_space(thickness: f64) -> LensSystem {
    let mut lens = singlet();
    lens.wavelengths = vec![0.6328];
    lens.primary_wavelength = 0;
    lens.surfaces = vec![Surface::new(0.0, thickness, "AIR")];
    lens.stop_index = 0;
    lens
}

#[test]
fn gaussian_beam_spreads_as_predicted() {
    let lens = free_space(300.0);
    let (w0, wavelength) = (0.5, 0.6328e-3);
    let zr = std::f64::consts::PI * w0 * w0 / wavelength;
    let result = simulate(&lens, &settings(Source::Gaussian { radius: w0, waist: 0.0 }, 256, false)).unwrap();
    let expected = w0 * (1.0 + (300.0 / zr).powi(2)).sqrt();
    let got = radius_at(&result, 300.0);
    assert!((got / expected - 1.0).abs() < 3e-3, "w {got} vs {expected}");
    let last = result.slices.last().unwrap();
    assert!((last.power - 1.0).abs() < 5e-3, "power {}", last.power);
}

fn compare_with_abcd(lens: &LensSystem, w_in: f64, tolerance: f64) {
    let result = simulate(lens, &settings(Source::Gaussian { radius: w_in, waist: 0.0 }, 256, false)).unwrap();
    let trace = gaussian_beam(lens, 0, w_in, 0.0, 4000).unwrap();
    // Compare the radius at every probed plane against the analytic profile (interpolated).
    let mut worst: f64 = 0.0;
    for s in &result.slices {
        let k = trace.profile.partition_point(|p| p[0] < s.z).clamp(1, trace.profile.len() - 1);
        let (a, b) = (trace.profile[k - 1], trace.profile[k]);
        let t = if b[0] > a[0] { ((s.z - a[0]) / (b[0] - a[0])).clamp(0.0, 1.0) } else { 0.0 };
        let analytic = a[1] + t * (b[1] - a[1]);
        let got = 0.5 * (s.w_x + s.w_y);
        worst = worst.max((got / analytic - 1.0).abs());
    }
    assert!(worst < tolerance, "worst relative deviation {worst}");
}

#[test]
fn focused_beam_matches_abcd() {
    // EFL 100 mm, 2 mm input radius: the focus is about 9 µm, 200× smaller than the input.
    compare_with_abcd(&singlet(), 2.0, 0.03);
}

#[test]
fn mirror_system_matches_abcd() {
    let mut lens = singlet();
    lens.wavelengths = vec![1.064];
    lens.primary_wavelength = 0;
    lens.surfaces = vec![Surface { conic: -1.0, ..Surface::new(-200.0, -90.0, "MIRROR") }];
    compare_with_abcd(&lens, 3.0, 0.03);
}

#[test]
fn diverging_beam_with_waist_before_the_system() {
    let lens = singlet();
    let result = simulate(&lens, &settings(Source::Gaussian { radius: 0.3, waist: -60.0 }, 256, false)).unwrap();
    let trace = gaussian_beam(&lens, 0, 0.3, -60.0, 4000).unwrap();
    assert!(result.z_start < 0.0);
    let image = result.slices.last().unwrap();
    let analytic = trace.profile.last().unwrap()[1];
    assert!((0.5 * (image.w_x + image.w_y) / analytic - 1.0).abs() < 0.03, "{} vs {analytic}", image.w_x);
}

fn strehl_comparison(mut lens: LensSystem) -> (f64, f64) {
    use optics_core::{TraceContext, analysis, paraxial_data};
    // One wavelength, image plane at its paraxial focus.
    lens.wavelengths = vec![lens.wavelengths[lens.primary_wavelength]];
    lens.primary_wavelength = 0;
    let last = lens.surfaces.len() - 1;
    lens.surfaces[last].thickness = paraxial_data(&lens).unwrap().image_distance;
    let semi = lens.entrance_pupil_diameter / 2.0;
    let source = || Source::TopHat { radius: semi, curvature_radius: None };
    let ideal = simulate(&lens, &settings(source(), 256, false)).unwrap();
    let real = simulate(&lens, &settings(source(), 256, true)).unwrap();
    let pop = real.slices.last().unwrap().peak / ideal.slices.last().unwrap().peak;
    let paraxial = paraxial_data(&lens).unwrap();
    let context = TraceContext::new(&lens, &paraxial);
    let fft = analysis::psf(&context, &paraxial, 0.0, 64, 4, 32).unwrap().strehl;
    println!("POP Strehl {pop:.3}, FFT Strehl {fft:.3}, aberration {:?}", real.aberration);
    (pop, fft)
}

#[test]
fn aberrated_focus_matches_the_fft_psf() {
    let mut slow = singlet();
    slow.entrance_pupil_diameter = 5.0; // F/20: small spherical aberration
    let mut medium = singlet();
    medium.entrance_pupil_diameter = 8.0; // F/12.5
    for lens in [slow, medium, optics_core::system::achromat()] {
        let (pop, fft) = strehl_comparison(lens);
        assert!((pop - fft).abs() < 0.06, "POP Strehl {pop} vs FFT {fft}");
    }
}

#[test]
fn through_focus_asymmetry_matches_the_fft_psf() {
    use optics_core::{TraceContext, analysis, paraxial_data};
    // Spherical aberration makes the peak irradiance different on the two sides of the paraxial focus; the sign of the
    // wavefront phase decides which side is better, so POP and the ray-based PSF must agree on it.
    let mut lens = singlet();
    lens.entrance_pupil_diameter = 8.0;
    lens.wavelengths = vec![lens.wavelengths[lens.primary_wavelength]];
    lens.primary_wavelength = 0;
    let last = lens.surfaces.len() - 1;
    let focus = paraxial_data(&lens).unwrap().image_distance;
    let semi = lens.entrance_pupil_diameter / 2.0;
    let source = || Source::TopHat { radius: semi, curvature_radius: None };
    lens.surfaces[last].thickness = focus;
    let ideal_peak = simulate(&lens, &settings(source(), 256, false)).unwrap().slices.last().unwrap().peak;
    let mut results = Vec::new();
    for shift in [-0.8, 0.8] {
        let mut shifted = lens.clone();
        shifted.surfaces[last].thickness = focus + shift;
        let pop = simulate(&shifted, &settings(source(), 256, true)).unwrap().slices.last().unwrap().peak / ideal_peak;
        let paraxial = paraxial_data(&shifted).unwrap();
        let context = TraceContext::new(&shifted, &paraxial);
        let fft = analysis::psf(&context, &paraxial, 0.0, 64, 4, 32).unwrap().strehl;
        println!("shift {shift:+.1}: POP {pop:.3}, FFT {fft:.3}");
        results.push((pop, fft));
    }
    let (pop_asym, fft_asym) = (results[0].0 - results[1].0, results[0].1 - results[1].1);
    assert!(pop_asym.signum() == fft_asym.signum() && fft_asym.abs() > 0.02, "asymmetry POP {pop_asym}, FFT {fft_asym}");
}
