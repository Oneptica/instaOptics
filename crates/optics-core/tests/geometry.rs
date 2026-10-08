//! Mirrors, coordinate breaks and aperture / field types.

use optics_core::system::{ApertureType, CoordinateBreak, FieldType, Surface, achromat, cooke_triplet};
use optics_core::{TraceContext, analysis, paraxial_data, resolved};

fn rms_spot(lens: &optics_core::LensSystem) -> Vec<f64> {
    let lens = resolved(lens).unwrap();
    let paraxial = paraxial_data(&lens).unwrap();
    let context = TraceContext::new(&lens, &paraxial);
    analysis::spot_diagram(&context, 6).unwrap().iter().map(|s| s.rms_radius).collect()
}

#[test]
fn parabolic_mirror_focuses_on_axis() {
    let mut lens = achromat();
    lens.wavelengths = vec![0.55];
    lens.primary_wavelength = 0;
    lens.fields = vec![0.0];
    lens.surfaces = vec![Surface { conic: -1.0, ..Surface::new(-200.0, -100.0, "MIRROR") }];
    let paraxial = paraxial_data(&lens).unwrap();
    assert!((paraxial.efl - 100.0).abs() < 1e-9, "efl {}", paraxial.efl);
    assert!(rms_spot(&lens)[0] < 1e-9);
}

#[test]
fn fold_mirror_does_not_change_the_image() {
    let reference = cooke_triplet();
    let mut folded = reference.clone();
    let last = folded.surfaces.len() - 1;
    let back = folded.surfaces[last].thickness;
    // Split the back focal distance with a 45° fold: break, mirror, break, then continue (negative thickness).
    folded.surfaces[last].thickness = 20.0;
    let cb = |tilt: f64, thickness: f64| Surface {
        coordinate_break: Some(CoordinateBreak { decenter: [0.0, 0.0], tilt: [tilt, 0.0, 0.0] }),
        ..Surface::new(0.0, thickness, "AIR")
    };
    folded.surfaces.push(cb(45.0, 0.0));
    folded.surfaces.push(Surface::new(0.0, 0.0, "MIRROR"));
    folded.surfaces.push(cb(45.0, -(back - 20.0)));
    let a = paraxial_data(&reference).unwrap();
    let b = paraxial_data(&folded).unwrap();
    assert!((a.efl - b.efl).abs() < 1e-9, "{} vs {}", a.efl, b.efl);
    let (sa, sb) = (rms_spot(&reference), rms_spot(&folded));
    for (x, y) in sa.iter().zip(&sb) {
        assert!((x - y).abs() < 1e-9 * x.max(1e-3), "{x} vs {y}");
    }
}

#[test]
fn aperture_and_field_types_convert() {
    let mut lens = cooke_triplet();
    lens.aperture_type = ApertureType::ImageFNumber;
    lens.aperture_value = Some(4.0);
    let r = resolved(&lens).unwrap();
    assert!((paraxial_data(&r).unwrap().f_number - 4.0).abs() < 1e-9);

    lens.aperture_type = ApertureType::FloatByStop;
    lens.aperture_value = Some(3.0);
    let r = resolved(&lens).unwrap();
    assert!((paraxial_data(&r).unwrap().stop_semi_diameter - 3.0).abs() < 1e-9);

    let mut lens = cooke_triplet();
    lens.field_type = FieldType::ImageHeight;
    lens.fields = vec![0.0, 10.0, 18.0];
    let r = resolved(&lens).unwrap();
    let mut top = r.clone();
    top.fields = vec![r.fields[2]];
    assert!((paraxial_data(&top).unwrap().image_height - 18.0).abs() < 1e-9);
}
