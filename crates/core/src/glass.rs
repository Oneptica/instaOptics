//! Refractive index models. Wavelengths are in micrometres.

use serde::{Deserialize, Serialize};

pub const LINE_D: f64 = 0.5875618;
pub const LINE_F: f64 = 0.4861327;
pub const LINE_C: f64 = 0.6562725;

/// A catalog glass with Sellmeier coefficients: n² = 1 + Σ B·λ² / (λ² − C).
#[derive(Clone, Copy, Debug)]
pub struct CatalogGlass {
    pub name: &'static str,
    pub nd: f64,
    pub vd: f64,
    pub b: [f64; 3],
    pub c: [f64; 3],
}

const fn glass(name: &'static str, nd: f64, vd: f64, b: [f64; 3], c: [f64; 3]) -> CatalogGlass {
    CatalogGlass { name, nd, vd, b, c }
}

/// Schott Sellmeier data, plus fused silica and CaF2 from Malitson.
pub const CATALOG: &[CatalogGlass] = &[
    glass("N-FK51A", 1.48656, 84.47, [0.971247817, 0.216901417, 0.904651666], [0.00472301995, 0.0153575612, 168.68133]),
    glass("N-PK51", 1.52855, 76.98, [1.15610775, 0.153229344, 0.785618966], [0.00585597402, 0.0194072416, 140.537046]),
    glass("N-BK7", 1.5168, 64.17, [1.03961212, 0.231792344, 1.01046945], [0.00600069867, 0.0200179144, 103.560653]),
    glass("N-K5", 1.52249, 59.48, [1.08511833, 0.199562005, 0.930511663], [0.00661099503, 0.024110866, 111.982777]),
    glass("N-BAK1", 1.5725, 57.55, [1.12365662, 0.309276848, 0.881511957], [0.00644742752, 0.0222284402, 107.297751]),
    glass("N-BAK4", 1.56883, 55.98, [1.28834642, 0.132817724, 0.945395373], [0.00779980626, 0.0315631177, 105.965875]),
    glass("N-SK2", 1.60738, 56.65, [1.28189012, 0.257738258, 0.96818604], [0.0072719164, 0.0242823527, 110.377773]),
    glass("N-SK14", 1.60311, 60.6, [0.936155374, 0.594052018, 1.04374583], [0.00461716525, 0.016885927, 103.736265]),
    glass("N-SK16", 1.62041, 60.32, [1.34317774, 0.241144399, 0.994317969], [0.00704687339, 0.0229005, 92.7508526]),
    glass("N-SSK8", 1.61773, 49.83, [1.44857867, 0.117965926, 1.06937528], [0.00869310149, 0.0421566593, 111.300666]),
    glass("N-LAK9", 1.691, 54.71, [1.46231905, 0.344399589, 1.15508372], [0.00724270156, 0.0243353131, 85.4686868]),
    glass("N-LAK22", 1.65113, 55.89, [1.14229781, 0.535138441, 1.04088385], [0.00585778594, 0.0198546147, 100.834017]),
    glass("N-BAF10", 1.67003, 47.11, [1.5851495, 0.143559385, 1.08521269], [0.00926681282, 0.0424489805, 105.613573]),
    glass("N-KZFS4", 1.61336, 44.49, [1.35055424, 0.197575506, 1.09962992], [0.0087628207, 0.0371767201, 90.3866994]),
    glass("N-LAF2", 1.744, 44.85, [1.80984227, 0.15729555, 1.0930037], [0.0101711622, 0.0442431765, 100.687748]),
    glass("N-LASF44", 1.8042, 46.5, [1.78897105, 0.38675867, 1.30506243], [0.00872506277, 0.0308085023, 92.7743824]),
    glass("N-LASF9", 1.85025, 32.17, [2.00029547, 0.298926886, 1.80691843], [0.0121426017, 0.0538736236, 156.530829]),
    glass("F2", 1.62004, 36.37, [1.34533359, 0.209073176, 0.937357162], [0.00997743871, 0.0470450767, 111.886764]),
    glass("N-F2", 1.62005, 36.43, [1.39757037, 0.159201403, 1.2686543], [0.00995906143, 0.0546931752, 119.248346]),
    glass("N-SF2", 1.64769, 33.82, [1.47343127, 0.163681849, 1.36920899], [0.0109019098, 0.0585683687, 127.404933]),
    glass("N-SF5", 1.6727, 32.25, [1.52481889, 0.187085527, 1.42729015], [0.011254756, 0.0588995392, 129.141675]),
    glass("N-SF1", 1.71736, 29.62, [1.60865158, 0.237725916, 1.51530653], [0.0119654879, 0.0590589722, 135.521676]),
    glass("N-SF10", 1.72828, 28.53, [1.62153902, 0.256287842, 1.64447552], [0.0122241457, 0.0595736775, 147.468793]),
    glass("N-SF4", 1.75513, 27.38, [1.67780282, 0.282849893, 1.63539276], [0.012679345, 0.0602038419, 145.760496]),
    glass("N-SF11", 1.78472, 25.68, [1.73759695, 0.313747346, 1.89878101], [0.013188707, 0.0623068142, 155.23629]),
    glass("N-SF6", 1.80518, 25.36, [1.77931763, 0.338149866, 2.08734474], [0.0133714182, 0.0617533621, 174.01759]),
    glass("N-SF57", 1.84666, 23.78, [1.87543831, 0.37375749, 2.30001797], [0.0141749518, 0.0640509927, 177.389795]),
    glass("F_SILICA", 1.458464, 67.82, [0.6961663, 0.4079426, 0.8974794], [0.00467914826, 0.0135120631, 97.9340025]),
    glass("CAF2", 1.433849, 94.99, [0.5675888, 0.4710914, 3.8484723], [0.00252643, 0.0100783, 1200.556]),
];

pub fn find(name: &str) -> Option<&'static CatalogGlass> {
    let name = name.trim();
    CATALOG.iter().find(|glass| glass.name.eq_ignore_ascii_case(name))
}

pub fn sellmeier(glass: &CatalogGlass, wavelength: f64) -> f64 {
    let l2 = wavelength * wavelength;
    let sum: f64 = 1.0 + (0..3).map(|i| glass.b[i] * l2 / (l2 - glass.c[i])).sum::<f64>();
    sum.sqrt()
}

/// A model glass "nd/vd" uses a two-term Cauchy fit through nd and the F–C dispersion.
fn model_index(nd: f64, vd: f64, wavelength: f64) -> f64 {
    let dispersion = (nd - 1.0) / vd;
    let b = dispersion / (1.0 / (LINE_F * LINE_F) - 1.0 / (LINE_C * LINE_C));
    let a = nd - b / (LINE_D * LINE_D);
    a + b / (wavelength * wavelength)
}

pub fn is_air(material: &str) -> bool {
    let name = material.trim();
    name.is_empty() || name.eq_ignore_ascii_case("AIR")
}

pub fn is_mirror(material: &str) -> bool {
    material.trim().eq_ignore_ascii_case("MIRROR")
}

/// Parses a model glass written as "nd/vd", e.g. "1.5168/64.17".
pub fn parse_model_glass(material: &str) -> Option<(f64, f64)> {
    let (nd, vd) = material.trim().split_once('/')?;
    let nd: f64 = nd.trim().parse().ok()?;
    let vd: f64 = vd.trim().parse().ok()?;
    (nd >= 1.0 && vd > 0.0 && nd.is_finite() && vd.is_finite()).then_some((nd, vd))
}

pub fn is_known_material(material: &str) -> bool {
    is_air(material) || is_mirror(material) || find(material).is_some() || parse_model_glass(material).is_some()
}

/// Refractive index of a material at a wavelength in µm, or None when the material is unknown.
pub fn refractive_index(material: &str, wavelength: f64) -> Option<f64> {
    if is_air(material) {
        return Some(1.0);
    }
    if let Some(glass) = find(material) {
        return Some(sellmeier(glass, wavelength));
    }
    parse_model_glass(material).map(|(nd, vd)| model_index(nd, vd, wavelength))
}

/// A glass defined by one of the OpticStudio catalog (AGF) dispersion formulas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassDef {
    pub name: String,
    /// AGF formula number, 1–13.
    pub formula: u8,
    pub coefficients: Vec<f64>,
    #[serde(default)]
    pub nd: f64,
    #[serde(default)]
    pub vd: f64,
    /// Wavelength range of validity, µm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    /// Catalog the glass comes from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub catalog: String,
}

pub const FORMULA_NAMES: [&str; 13] = [
    "Schott", "Sellmeier 1", "Herzberger", "Sellmeier 2", "Conrady", "Sellmeier 3", "Handbook of Optics 1",
    "Handbook of Optics 2", "Sellmeier 4", "Extended", "Sellmeier 5", "Extended 2", "Extended 3",
];

impl GlassDef {
    /// Refractive index at a wavelength in µm, or None when the formula number or coefficients are unusable.
    pub fn index(&self, wavelength: f64) -> Option<f64> {
        let c = |i: usize| self.coefficients.get(i).copied().unwrap_or(0.0);
        let l = wavelength;
        let l2 = l * l;
        // Sums of K·λ²/(λ²−L) over (K, L) coefficient pairs.
        let pairs = |count: usize| (0..count).map(|k| c(2 * k) * l2 / (l2 - c(2 * k + 1))).sum::<f64>();
        let power = |p: i32| l.powi(p);
        let n2 = match self.formula {
            1 => c(0) + c(1) * l2 + c(2) * power(-2) + c(3) * power(-4) + c(4) * power(-6) + c(5) * power(-8),
            2 => 1.0 + pairs(3),
            3 => {
                let big_l = 1.0 / (l2 - 0.028);
                return Some(c(0) + c(1) * big_l + c(2) * big_l * big_l + c(3) * l2 + c(4) * power(4) + c(5) * power(6));
            }
            4 => 1.0 + c(0) + c(1) * l2 / (l2 - c(2) * c(2)) + c(3) * l2 / (l2 - c(4) * c(4)),
            5 => return Some(c(0) + c(1) / l + c(2) / l.powf(3.5)),
            6 => 1.0 + pairs(4),
            7 => c(0) + c(1) / (l2 - c(2)) - c(3) * l2,
            8 => c(0) + c(1) * l2 / (l2 - c(2)) - c(3) * l2,
            9 => c(0) + c(1) * l2 / (l2 - c(2)) + c(3) * l2 / (l2 - c(4)),
            10 => c(0) + c(1) * l2 + c(2) * power(-2) + c(3) * power(-4) + c(4) * power(-6) + c(5) * power(-8) + c(6) * power(6) + c(7) * power(8),
            11 => 1.0 + pairs(5),
            12 => c(0) + c(1) * l2 + c(2) * power(-2) + c(3) * power(-4) + c(4) * power(-6) + c(5) * power(-8) + c(6) * power(4) + c(7) * power(6),
            13 => c(0) + c(1) * l2 + c(2) * power(4) + c(3) * power(-2) + c(4) * power(-4) + c(5) * power(-6) + c(6) * power(-8) + c(7) * power(-10) + c(8) * power(-12),
            _ => return None,
        };
        (n2 > 0.0 && n2.is_finite()).then(|| n2.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_nd_and_vd() {
        for glass in CATALOG {
            let nd = sellmeier(glass, LINE_D);
            let vd = (nd - 1.0) / (sellmeier(glass, LINE_F) - sellmeier(glass, LINE_C));
            // Catalog nd and vd are rounded, so compare at their printed precision.
            assert!((nd - glass.nd).abs() <= 1e-4, "{} nd {nd}", glass.name);
            assert!((vd - glass.vd).abs() <= 0.05, "{} vd {vd}", glass.name);
        }
    }

    fn bk7_as(formula: u8, coefficients: Vec<f64>) -> GlassDef {
        GlassDef { name: "TEST".into(), formula, coefficients, ..Default::default() }
    }

    #[test]
    fn agf_sellmeier_formulas_agree_with_the_built_in_catalog() {
        // N-BK7 in AGF order: K1 L1 K2 L2 K3 L3.
        let bk7 = vec![1.03961212, 0.00600069867, 0.231792344, 0.0200179144, 1.01046945, 103.560653];
        let builtin = find("N-BK7").unwrap();
        for (formula, mut coefficients) in [(2, bk7.clone()), (6, bk7.clone()), (11, bk7.clone())] {
            coefficients.resize(10, 0.0); // unused extra terms are zero
            let glass = bk7_as(formula, coefficients);
            for wavelength in [0.4, 0.5, 0.5875618, 0.7, 1.0] {
                let (a, b) = (glass.index(wavelength).unwrap(), sellmeier(builtin, wavelength));
                assert!((a - b).abs() < 1e-12, "formula {formula} at {wavelength}: {a} vs {b}");
            }
        }
        // Handbook of Optics 2 is a one-term Sellmeier with a linear-in-λ² remainder.
        let one_term = bk7_as(8, vec![1.0, 1.03961212, 0.00600069867, 0.0]);
        let n2: f64 = 1.0 + 1.03961212 * 0.25 / (0.25 - 0.00600069867);
        assert!((one_term.index(0.5).unwrap() - n2.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn agf_power_series_formulas_use_the_documented_terms() {
        // n² = 2.25 + λ-dependent terms, evaluated by hand at λ = 0.5.
        let l: f64 = 0.5;
        let check = |formula: u8, coefficients: Vec<f64>, n2: f64| {
            let got = bk7_as(formula, coefficients).index(l).unwrap();
            assert!((got - n2.sqrt()).abs() < 1e-12, "formula {formula}: {got} vs {}", n2.sqrt());
        };
        check(1, vec![2.25, 0.01, 0.02, 0.03, 0.04, 0.05], 2.25 + 0.01 * l.powi(2) + 0.02 * l.powi(-2) + 0.03 * l.powi(-4) + 0.04 * l.powi(-6) + 0.05 * l.powi(-8));
        check(10, vec![2.25, 0.01, 0.0, 0.0, 0.0, 0.0, 0.02, 0.03], 2.25 + 0.01 * l.powi(2) + 0.02 * l.powi(6) + 0.03 * l.powi(8));
        check(12, vec![2.25, 0.01, 0.0, 0.0, 0.0, 0.0, 0.02, 0.03], 2.25 + 0.01 * l.powi(2) + 0.02 * l.powi(4) + 0.03 * l.powi(6));
        check(13, vec![2.25, 0.01, 0.02, 0.03, 0.0, 0.0, 0.0, 0.0, 0.04], 2.25 + 0.01 * l.powi(2) + 0.02 * l.powi(4) + 0.03 * l.powi(-2) + 0.04 * l.powi(-12));
        check(7, vec![2.0, 0.1, 0.01, 0.05], 2.0 + 0.1 / (l * l - 0.01) - 0.05 * l * l);
        check(9, vec![1.5, 0.3, 0.02, 0.2, 0.05], 1.5 + 0.3 * l * l / (l * l - 0.02) + 0.2 * l * l / (l * l - 0.05));
        // Conrady and Herzberger give n directly.
        assert!((bk7_as(5, vec![1.5, 0.004, 0.0002]).index(l).unwrap() - (1.5 + 0.004 / l + 0.0002 / l.powf(3.5))).abs() < 1e-12);
        let big_l = 1.0 / (l * l - 0.028);
        assert!((bk7_as(3, vec![1.5, 0.003, 0.0001, -0.002, 0.0001, 0.0]).index(l).unwrap() - (1.5 + 0.003 * big_l + 0.0001 * big_l * big_l - 0.002 * l * l + 0.0001 * l.powi(4))).abs() < 1e-12);
        assert!(bk7_as(99, vec![1.0]).index(l).is_none());
    }

    #[test]
    fn model_glass_reproduces_nd_and_vd() {
        let nd = refractive_index("1.6/40", LINE_D).unwrap();
        let vd = (nd - 1.0) / (refractive_index("1.6/40", LINE_F).unwrap() - refractive_index("1.6/40", LINE_C).unwrap());
        assert!((nd - 1.6).abs() < 1e-12 && (vd - 40.0).abs() < 1e-9);
        assert!(refractive_index("unobtainium", LINE_D).is_none());
        assert_eq!(refractive_index(" air ", 0.5), Some(1.0));
    }
}
