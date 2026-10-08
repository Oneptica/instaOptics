//! Cross-checks the core against reference values exported from the instaOptics web engine
//! (tests/fixtures/web-reference.json: paraxial data, the top marginal ray of the largest field, and automatic apertures).

use optics_core::{TraceContext, paraxial_data, system, trace};
use serde_json::Value;

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!((actual - expected).abs() <= tolerance * expected.abs().max(1.0), "{what}: {actual} vs {expected}");
}

#[test]
fn matches_web_engine() {
    let reference: Value = serde_json::from_str(include_str!("fixtures/web-reference.json")).unwrap();
    for (id, lens) in system::samples() {
        let expected = &reference[id];
        let number = |key: &str| expected[key].as_f64().unwrap();
        assert_eq!(lens.surfaces.last().unwrap().thickness, number("lastT"), "{id} image distance");

        let paraxial = paraxial_data(&lens).unwrap();
        close(paraxial.efl, number("efl"), 1e-12, &format!("{id} efl"));
        close(paraxial.bfl, number("bfl"), 1e-12, &format!("{id} bfl"));
        close(paraxial.f_number, number("fNumber"), 1e-12, &format!("{id} F/#"));
        close(paraxial.entrance_pupil_z, number("epz"), 1e-12, &format!("{id} entrance pupil"));
        close(paraxial.image_height, number("img"), 1e-12, &format!("{id} image height"));
        close(paraxial.exit_pupil_z, number("exp"), 1e-12, &format!("{id} exit pupil"));

        let context = TraceContext::new(&lens, &paraxial);
        let wavelength = lens.primary_wavelength();
        let n = lens.medium_indices(wavelength).unwrap();
        let ray = context.trace_ray(*lens.fields.last().unwrap(), 0.0, 1.0, wavelength, &n);
        assert!(ray.ok(), "{id} ray failed: {:?}", ray.failure);
        let end = ray.end();
        let expected_end: Vec<f64> = expected["ray"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for axis in 0..3 {
            close(end[axis], expected_end[axis], 1e-9, &format!("{id} ray end [{axis}]"));
        }
        close(ray.opl, number("opl"), 1e-9, &format!("{id} optical path"));

        let semi_diameters = trace::automatic_semi_diameters(&context).unwrap();
        let expected_sd: Vec<f64> = expected["sd"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(semi_diameters, expected_sd, "{id} automatic semi-diameters");
    }
}

#[test]
fn cooke_triplet_first_order() {
    let paraxial = paraxial_data(&system::cooke_triplet()).unwrap();
    assert!((paraxial.efl - 50.02).abs() < 0.01 && (paraxial.f_number - 5.0).abs() < 0.01);
}

#[test]
fn overview_serializes_for_the_ui() {
    let overview = optics_core::overview(&system::cooke_triplet(), 7).unwrap();
    assert_eq!(overview.layout.elements.len(), 3);
    assert_eq!(overview.layout.rays.len(), 21);
    assert!(overview.layout.rays.iter().all(|ray| ray.failure.is_none()));
    let json = serde_json::to_value(&overview).unwrap();
    assert!(json["paraxial"]["efl"].is_f64() && json["layout"]["stopZ"].is_f64());
}

#[test]
fn reports_unknown_materials() {
    let mut lens = system::singlet();
    lens.surfaces[0].material = "NOPE".into();
    assert_eq!(optics_core::overview(&lens, 3).unwrap_err().0, "Unknown material \"NOPE\" on surface 1");
}
