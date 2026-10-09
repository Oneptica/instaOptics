//! Thin-film coatings: the transfer matrix method for the amplitude reflection and transmission of s and p light.
//!
//! Coating text (comma separated, incident side first):
//! * `MgF2` or `MgF2@550` — one quarter-wave layer of n = 1.38 at the given wavelength in nm (default: the primary one)
//! * `QW1.38@550` — a quarter-wave layer of any index
//! * `1.38:99.6` — a layer of index and thickness in nm
//! * `HR@1064` — a high reflector: ten quarter-wave pairs of n = 2.10 / 1.46 plus one more high-index layer
//! * `AL`, `AG`, `AU` — a metal substrate (constant approximate index at 550 nm); `M:0.96+6.69i` for any other
//!
//! A metal (or the last layer's substrate) replaces the glass behind the coating, so metals suit mirrors.

use num_complex::Complex64 as C;

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub index: f64,
    /// Thickness in nm.
    pub thickness: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coating {
    pub layers: Vec<Layer>,
    /// Complex index replacing the medium behind the coating (a metal).
    pub substrate: Option<C>,
}

fn metal(name: &str) -> Option<C> {
    match name.to_ascii_uppercase().as_str() {
        "AL" => Some(C::new(0.96, 6.69)),
        "AG" => Some(C::new(0.06, 3.40)),
        "AU" => Some(C::new(0.43, 2.45)),
        _ => None,
    }
}

fn parse_complex(text: &str) -> Option<C> {
    // n+ki or n-ki, with an optional trailing "i".
    let text = text.trim().trim_end_matches(['i', 'j']);
    let split = text.rfind(['+', '-']).filter(|&at| at > 0)?;
    let (re, im) = text.split_at(split);
    let im = im.trim_end_matches('k').trim_start_matches('+');
    let im: f64 = im.trim_start_matches('-').trim_end_matches('k').parse().ok()?;
    let sign = if text[split..].starts_with('-') { -1.0 } else { 1.0 };
    // k is the extinction coefficient: n − ik convention is written as n+ki here.
    Some(C::new(re.trim().parse().ok()?, sign * im))
}

/// Parses coating text; `default_wavelength` is in µm.
pub fn parse(spec: &str, default_wavelength: f64) -> Result<Coating, String> {
    let mut coating = Coating::default();
    for token in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let (head, wavelength_nm) = match token.split_once('@') {
            Some((head, nm)) => (head.trim(), nm.trim().parse::<f64>().map_err(|_| format!("bad wavelength in \"{token}\""))?),
            None => (token, default_wavelength * 1000.0),
        };
        if !(wavelength_nm > 0.0) {
            return Err(format!("bad wavelength in \"{token}\""));
        }
        let quarter = |index: f64| Layer { index, thickness: wavelength_nm / (4.0 * index) };
        if let Some(index) = metal(head) {
            coating.substrate = Some(index);
        } else if let Some(text) = head.strip_prefix("M:").or_else(|| head.strip_prefix("m:")) {
            coating.substrate = Some(parse_complex(text).ok_or_else(|| format!("cannot read the metal index in \"{token}\"; write M:n+ki"))?);
        } else if head.eq_ignore_ascii_case("MgF2") {
            coating.layers.push(quarter(1.38));
        } else if head.eq_ignore_ascii_case("HR") {
            for _ in 0..10 {
                coating.layers.push(quarter(2.10));
                coating.layers.push(quarter(1.46));
            }
            coating.layers.push(quarter(2.10));
        } else if let Some(index) = head.strip_prefix("QW").or_else(|| head.strip_prefix("qw")) {
            let index: f64 = index.parse().map_err(|_| format!("cannot read the index in \"{token}\""))?;
            if !(index > 0.0) {
                return Err(format!("the index must be positive in \"{token}\""));
            }
            coating.layers.push(quarter(index));
        } else if let Some((index, thickness)) = head.split_once(':') {
            let (index, thickness): (f64, f64) = (
                index.trim().parse().map_err(|_| format!("cannot read the layer \"{token}\"; write index:thickness_nm"))?,
                thickness.trim().parse().map_err(|_| format!("cannot read the layer \"{token}\"; write index:thickness_nm"))?,
            );
            if !(index > 0.0 && thickness >= 0.0) {
                return Err(format!("the layer \"{token}\" needs a positive index and thickness"));
            }
            coating.layers.push(Layer { index, thickness });
        } else {
            return Err(format!("unknown coating \"{token}\""));
        }
    }
    Ok(coating)
}

/// Amplitude coefficients for E-field components along ŝ and p̂ (p̂ = k̂ × ŝ for each ray), as in polarization ray tracing.
#[derive(Clone, Copy, Debug)]
pub struct Coefficients {
    pub r_s: C,
    pub r_p: C,
    pub t_s: C,
    pub t_p: C,
    /// cos θ in the medium behind the coating.
    pub cos_t: C,
}

fn cos_in(n0: f64, sin0: f64, n: C) -> C {
    let sin = C::new(n0 * sin0, 0.0) / n;
    let cos = (C::new(1.0, 0.0) - sin * sin).sqrt();
    // The branch with a non-negative real part (and a non-positive imaginary part for absorbing media).
    if cos.re < 0.0 || (cos.re == 0.0 && cos.im > 0.0) { -cos } else { cos }
}

/// Reflection and transmission of a coated interface between a medium of index `n0` (incidence angle with
/// `cos_i`) and a medium of complex index `n_sub`, at a wavelength in µm.
pub fn coefficients(coating: Option<&Coating>, n0: f64, n_sub: C, wavelength: f64, cos_i: f64) -> Coefficients {
    let cos_i = cos_i.clamp(1e-9, 1.0);
    let sin0 = (1.0 - cos_i * cos_i).sqrt();
    let n_sub = coating.and_then(|c| c.substrate).unwrap_or(n_sub);
    let cos0 = C::new(cos_i, 0.0);
    let cos_s = cos_in(n0, sin0, n_sub);
    let empty = Coating::default();
    let layers = &coating.unwrap_or(&empty).layers;
    // Admittances of the medium of incidence and the substrate; characteristic matrix of the layer stack.
    let result = |polarization_p: bool| {
        let admittance = |n: C, cos: C| if polarization_p { n / cos } else { n * cos };
        let eta0 = admittance(C::new(n0, 0.0), cos0);
        let eta_s = admittance(n_sub, cos_s);
        let (mut m00, mut m01, mut m10, mut m11) = (C::new(1.0, 0.0), C::new(0.0, 0.0), C::new(0.0, 0.0), C::new(1.0, 0.0));
        for layer in layers {
            let n = C::new(layer.index, 0.0);
            let cos = cos_in(n0, sin0, n);
            let delta = 2.0 * std::f64::consts::PI * n * cos * (layer.thickness * 1e-3 / wavelength);
            let (cd, sd) = (delta.cos(), delta.sin());
            let eta = admittance(n, cos);
            let i = C::new(0.0, 1.0);
            let (a00, a01, a10, a11) = (cd, i * sd / eta, i * eta * sd, cd);
            let (b00, b01, b10, b11) = (m00 * a00 + m01 * a10, m00 * a01 + m01 * a11, m10 * a00 + m11 * a10, m10 * a01 + m11 * a11);
            (m00, m01, m10, m11) = (b00, b01, b10, b11);
        }
        let b = m00 + m01 * eta_s;
        let c = m10 + m11 * eta_s;
        let r = (eta0 * b - c) / (eta0 * b + c);
        let t = C::new(2.0, 0.0) * eta0 / (eta0 * b + c);
        (r, t)
    };
    let (r_s, t_s) = result(false);
    let (r_tmm_p, t_tmm_p) = result(true);
    Coefficients {
        r_s,
        // Local p̂ = k̂ × ŝ flips with the propagation direction, so the reflection coefficient changes sign; the
        // transmitted tangential field converts to the E amplitude by cos θ_in / cos θ_out.
        r_p: -r_tmm_p,
        t_s,
        t_p: t_tmm_p * cos0 / cos_s,
        cos_t: cos_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: C, b: C) {
        assert!((a - b).norm() < 1e-12, "{a} vs {b}");
    }

    #[test]
    fn uncoated_matches_fresnel() {
        let (n1, n2) = (1.0_f64, 1.5_f64);
        for theta in [0.0_f64, 0.3, 0.9, 1.3] {
            let (c1, sin1) = (theta.cos(), theta.sin());
            let c2 = (1.0 - (n1 * sin1 / n2).powi(2)).sqrt();
            let k = coefficients(None, n1, C::new(n2, 0.0), 0.55, c1);
            close(k.r_s, C::new((n1 * c1 - n2 * c2) / (n1 * c1 + n2 * c2), 0.0));
            close(k.r_p, C::new((n2 * c1 - n1 * c2) / (n2 * c1 + n1 * c2), 0.0));
            close(k.t_s, C::new(2.0 * n1 * c1 / (n1 * c1 + n2 * c2), 0.0));
            close(k.t_p, C::new(2.0 * n1 * c1 / (n2 * c1 + n1 * c2), 0.0));
        }
    }

    #[test]
    fn quarter_wave_mgf2_removes_the_reflection_at_the_design_wavelength() {
        // The ideal single layer has n = sqrt(1.52); 1.38 leaves about 1.3 % instead of 4.3 %.
        let coating = parse("MgF2@550", 0.55).unwrap();
        let r = coefficients(Some(&coating), 1.0, C::new(1.52, 0.0), 0.55, 1.0).r_s.norm_sqr();
        let bare = coefficients(None, 1.0, C::new(1.52, 0.0), 0.55, 1.0).r_s.norm_sqr();
        let expected = ((1.0_f64 * 1.52 - 1.38 * 1.38) / (1.0 * 1.52 + 1.38 * 1.38)).powi(2);
        assert!((r - expected).abs() < 1e-12 && r < 0.4 * bare, "{r} {bare} {expected}");
    }

    #[test]
    fn energy_is_conserved_without_absorption() {
        let coating = parse("1.38:90,2.1:60,1.38:40", 0.55).unwrap();
        let n_sub = 1.52;
        for cos_i in [1.0, 0.8, 0.5] {
            let k = coefficients(Some(&coating), 1.0, C::new(n_sub, 0.0), 0.55, cos_i);
            let power = (n_sub * k.cos_t.re) / cos_i;
            for (r, t) in [(k.r_s, k.t_s), (k.r_p, k.t_p)] {
                let total = r.norm_sqr() + power * t.norm_sqr();
                assert!((total - 1.0).abs() < 1e-9, "R + T = {total}");
            }
        }
    }

    #[test]
    fn high_reflector_and_metal() {
        let hr = parse("HR@1064", 1.064).unwrap();
        assert_eq!(hr.layers.len(), 21);
        assert!(coefficients(Some(&hr), 1.0, C::new(1.52, 0.0), 1.064, 1.0).r_s.norm_sqr() > 0.99);
        let al = parse("AL", 0.55).unwrap();
        let r = coefficients(Some(&al), 1.0, C::new(1.0, 0.0), 0.55, 1.0).r_s.norm_sqr();
        assert!((0.88..0.93).contains(&r), "aluminium reflectance {r}");
        assert_eq!(parse("M:0.96+6.69i", 0.55).unwrap().substrate, Some(C::new(0.96, 6.69)));
        assert!(parse("nonsense", 0.55).is_err() && parse("1.4:x", 0.55).is_err() && parse("MgF2@-3", 0.55).is_err());
    }
}
