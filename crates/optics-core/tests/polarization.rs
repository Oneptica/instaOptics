//! Polarization ray tracing: uncoated glass, coated surfaces, mirrors.

use optics_core::polarization::{PolInput, coating_curves, polarization_map};
use optics_core::system::{Surface, cooke_triplet};
use optics_core::{TraceContext, paraxial_data, resolved};

#[test]
fn uncoated_singlet_transmission_matches_fresnel() {
    let mut lens = cooke_triplet();
    lens.fields = vec![0.0];
    let lens = resolved(&lens).unwrap();
    let paraxial = paraxial_data(&lens).unwrap();
    let context = TraceContext::new(&lens, &paraxial);
    let map = polarization_map(&context, 0.0, lens.primary_wavelength(), 9, PolInput::Unpolarized).unwrap();
    // Six glass/air surfaces near normal incidence lose roughly 4–6% each.
    assert!(map.mean_transmission > 0.6 && map.mean_transmission < 0.9, "{}", map.mean_transmission);
    assert!(map.max_diattenuation < 0.2);
}

#[test]
fn single_coating_raises_transmission() {
    let mut lens = cooke_triplet();
    lens.fields = vec![0.0];
    let bare = {
        let r = resolved(&lens).unwrap();
        let p = paraxial_data(&r).unwrap();
        let c = TraceContext::new(&r, &p);
        polarization_map(&c, 0.0, r.primary_wavelength(), 9, PolInput::Unpolarized).unwrap().mean_transmission
    };
    for s in lens.surfaces.iter_mut() {
        s.coating = Some("MgF2".into());
    }
    let r = resolved(&lens).unwrap();
    let p = paraxial_data(&r).unwrap();
    let c = TraceContext::new(&r, &p);
    let coated = polarization_map(&c, 0.0, r.primary_wavelength(), 9, PolInput::Unpolarized).unwrap().mean_transmission;
    assert!(coated > bare, "{coated} vs {bare}");
}

#[test]
fn mirror_reflects_everything() {
    let mut lens = cooke_triplet();
    lens.wavelengths = vec![0.55];
    lens.primary_wavelength = 0;
    lens.fields = vec![0.0];
    lens.surfaces = vec![Surface { conic: -1.0, ..Surface::new(-200.0, -100.0, "MIRROR") }];
    let r = resolved(&lens).unwrap();
    let p = paraxial_data(&r).unwrap();
    let c = TraceContext::new(&r, &p);
    let map = polarization_map(&c, 0.0, 0.55, 9, PolInput::Linear { angle: 0.0 }).unwrap();
    assert!((map.mean_transmission - 1.0).abs() < 1e-6, "{}", map.mean_transmission);
}

#[test]
fn curves_conserve_energy() {
    let mut lens = cooke_triplet();
    lens.surfaces[0].coating = Some("MgF2".into());
    let r = resolved(&lens).unwrap();
    let curves = coating_curves(&r, 0, r.primary_wavelength(), 60.0, 20).unwrap();
    for i in 0..curves.angles.len() {
        assert!((curves.r_s[i] + curves.t_s[i] - 1.0).abs() < 1e-6, "s at {}", curves.angles[i]);
        assert!((curves.r_p[i] + curves.t_p[i] - 1.0).abs() < 1e-6, "p at {}", curves.angles[i]);
    }
}
