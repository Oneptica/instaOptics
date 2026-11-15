//! The US 8,879,901 objective (11 elements, f/2) imported from a .zmx and compared with optiland, an independent
//! ray tracer (fixtures from examples/validation/us8879901.py). The glasses are optiland's data refitted to the AGF
//! Sellmeier formula with |Δn| ≤ 6e-6, so agreement is limited by that fit, not by the tracing: a relative index
//! error of 6e-6 / (n − 1) ≤ 1.3e-5 bounds the relative error of first-order quantities, and the same error moves a
//! ray at the image by far less than 0.1 µm at this focal length.

/// Relative tolerance for first-order quantities, from the glass fit (see above).
const PARAXIAL: f64 = 2e-5;
/// Image intercept tolerance (mm).
const RAY: f64 = 1e-4;

use instaoptics_engine::{Session, execute};
use serde_json::{Value, json};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/us8879901");

fn session() -> Session {
    let mut s = Session::new(None);
    s.add_catalog(std::path::Path::new(&format!("{DIR}/US8879901.agf"))).unwrap();
    let reply = execute(&mut s, "import.zmx", &json!({ "path": format!("{DIR}/us8879901.zmx") })).unwrap();
    assert_eq!(reply["warnings"].as_array().map_or(0, Vec::len), 0, "{reply}");
    // optiland aims rays paraxially by default.
    execute(&mut s, "system.set", &json!({ "rayAiming": false })).unwrap();
    s
}

fn reference() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{DIR}/reference.json")).unwrap()).unwrap()
}

#[test]
fn paraxial_data_matches_optiland() {
    let mut s = session();
    let o = execute(&mut s, "overview", &json!({})).unwrap();
    let r = reference();
    let p = &o["paraxial"];
    let total = p["totalTrack"].as_f64().unwrap();
    let pairs = [
        ("efl", p["efl"].as_f64().unwrap()),
        ("bfl", p["bfl"].as_f64().unwrap()),
        ("fNumber", p["fNumber"].as_f64().unwrap()),
        ("entrancePupilDiameter", o["entrancePupilDiameter"].as_f64().unwrap()),
        ("entrancePupilZ", p["entrancePupilZ"].as_f64().unwrap()),
        ("exitPupilFromImage", p["exitPupilZ"].as_f64().unwrap() - total),
    ];
    for (key, ours) in pairs {
        let theirs = r["paraxial"][key].as_f64().unwrap();
        println!("{key:24} instaOptics {ours:14.8} optiland {theirs:14.8} Δ {:.2e}", ours - theirs);
        assert!((ours - theirs).abs() <= PARAXIAL * theirs.abs(), "{key}: {ours} vs {theirs}");
    }
}

#[test]
fn real_rays_match_optiland() {
    let mut s = session();
    let mut worst: f64 = 0.0;
    for ray in reference()["rays"].as_array().unwrap() {
        let request = json!({ "kind": "rayTrace", "field": ray["field"], "wavelength": ray["wavelength"], "px": ray["px"], "py": ray["py"] });
        let traced = execute(&mut s, "analyze", &json!({ "request": request })).unwrap();
        assert!(traced["failure"].is_null(), "{ray} {traced}");
        let end = traced["points"].as_array().unwrap().last().unwrap();
        let (dx, dy) = (end[0].as_f64().unwrap() - ray["x"].as_f64().unwrap(), end[1].as_f64().unwrap() - ray["y"].as_f64().unwrap());
        worst = worst.max(dx.hypot(dy));
        assert!(dx.hypot(dy) <= RAY, "{ray}: {end}");
    }
    println!("worst image intercept difference {:.3e} mm", worst);
}
