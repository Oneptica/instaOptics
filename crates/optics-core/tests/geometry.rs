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

#[test]
fn folded_system_places_the_image_frame_after_the_fold() {
    use optics_core::layout::layout_3d;
    let mut lens = cooke_triplet();
    let last = lens.surfaces.len() - 1;
    let back = lens.surfaces[last].thickness;
    lens.surfaces[last].thickness = 20.0;
    let cb = |tilt: f64, thickness: f64| Surface {
        coordinate_break: Some(CoordinateBreak { decenter: [0.0, 0.0], tilt: [tilt, 0.0, 0.0] }),
        ..Surface::new(0.0, thickness, "AIR")
    };
    lens.surfaces.push(cb(45.0, 0.0));
    lens.surfaces.push(Surface::new(0.0, 0.0, "MIRROR"));
    lens.surfaces.push(cb(45.0, -(back - 20.0)));
    let paraxial = paraxial_data(&lens).unwrap();
    let context = TraceContext::new(&lens, &paraxial);
    let semi = vec![8.0; lens.surfaces.len()];
    let layout = layout_3d(&context, &semi, 8).unwrap();
    // The three glass elements keep the straight frame; the image plane sits 90° away from the first surface's axis.
    assert_eq!(layout.elements.len(), 3);
    assert!(layout.elements.iter().all(|e| (e.rotation[8] - 1.0).abs() < 1e-12));
    let axis_z = layout.image.rotation[8];
    assert!(axis_z.abs() < 1e-9, "image axis should be perpendicular to z, got {axis_z}");
    // The fold moves the image sideways by the remaining distance, in y.
    assert!((layout.image.origin[1].abs() - (back - 20.0)).abs() < 1e-6, "{:?}", layout.image.origin);
}

#[test]
fn glasses_defined_in_the_system_take_part_in_tracing() {
    use optics_core::glass::GlassDef;
    let reference = achromat();
    let mut custom = reference.clone();
    // N-BK7 and F2 redefined under other names with AGF Sellmeier 1 (K1 L1 K2 L2 K3 L3).
    custom.glasses = vec![
        GlassDef { name: "MY-CROWN".into(), formula: 2, coefficients: vec![1.03961212, 0.00600069867, 0.231792344, 0.0200179144, 1.01046945, 103.560653], ..Default::default() },
        GlassDef { name: "my-flint".into(), formula: 2, coefficients: vec![1.34533359, 0.00997743871, 0.209073176, 0.0470450767, 0.937357162, 111.886764], ..Default::default() },
    ];
    custom.surfaces[0].material = "MY-CROWN".into();
    custom.surfaces[1].material = "MY-FLINT".into(); // names are matched without regard to case
    let a = paraxial_data(&reference).unwrap();
    let b = paraxial_data(&custom).unwrap();
    assert!((a.efl - b.efl).abs() < 1e-9, "{} vs {}", a.efl, b.efl);
    let mut missing = custom.clone();
    missing.surfaces[0].material = "NOT-LOADED".into();
    assert!(missing.validate().is_err());
}
