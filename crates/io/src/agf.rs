//! OpticStudio glass catalogs (.agf): NM starts a glass, CD gives the dispersion coefficients, LD the wavelength range.

use instaoptics_core::glass::GlassDef;

pub fn parse_agf(text: &str, catalog: &str) -> Vec<GlassDef> {
    let mut glasses: Vec<GlassDef> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let (Some(tag), Some(rest)) = (line.get(..2), line.get(2..)) else { continue };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let num = |i: usize| fields.get(i).and_then(|f| f.parse::<f64>().ok());
        match tag {
            // NM name formula MIL# nd vd status [melt frequency]
            "NM" => {
                let Some(name) = fields.first() else { continue };
                glasses.push(GlassDef {
                    name: (*name).to_string(),
                    formula: num(1).map_or(0, |f| f as u8),
                    coefficients: Vec::new(),
                    nd: num(3).unwrap_or(0.0),
                    vd: num(4).unwrap_or(0.0),
                    range: None,
                    catalog: catalog.to_string(),
                });
            }
            "CD" => {
                if let Some(glass) = glasses.last_mut() {
                    glass.coefficients = fields.iter().filter_map(|f| f.parse::<f64>().ok()).filter(|v| v.is_finite()).collect();
                }
            }
            "LD" => {
                if let (Some(glass), Some(lo), Some(hi)) = (glasses.last_mut(), num(0), num(1)) {
                    glass.range = Some([lo, hi]);
                }
            }
            _ => {}
        }
    }
    // AGF also lists obsolete and incomplete entries; keep glasses with a known formula and coefficients.
    glasses.retain(|g| (1..=13).contains(&g.formula) && !g.coefficients.is_empty());
    glasses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_sellmeier_glass() {
        let text = "CC test\nNM N-BK7 2 517642.251 1.5168 64.17 0 0\nCD 1.03961212 0.00600069867 0.231792344 0.0200179144 1.01046945 103.560653 0 0 0 0\nLD 0.3 2.5\n";
        let glasses = parse_agf(text, "TEST");
        assert_eq!(glasses.len(), 1);
        assert_eq!(glasses[0].formula, 2);
        assert_eq!(glasses[0].range, Some([0.3, 2.5]));
        let n = glasses[0].index(0.5875618).unwrap();
        assert!((n - 1.5168).abs() < 1e-4);
    }
}
