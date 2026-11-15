//! Surface shape: sag, slope and ray intersection for spherical, conic and even-asphere surfaces.

use crate::system::Surface;
use crate::vec3::{Vec3, normalize};

/// Surface sag z(h); outside a conic's domain it returns the rim sag.
pub fn sag(surface: &Surface, h: f64) -> f64 {
    let c = surface.curvature();
    let k = surface.conic;
    let h2 = h * h;
    let mut z = 0.0;
    if c != 0.0 {
        let root = 1.0 - (1.0 + k) * c * c * h2;
        z = if root < 0.0 {
            if 1.0 + k == 0.0 { c * h2 / 2.0 } else { 1.0 / ((1.0 + k) * c) }
        } else {
            c * h2 / (1.0 + root.sqrt())
        };
    }
    for (j, &a) in surface.aspheric.iter().enumerate() {
        if a != 0.0 {
            z += a * h2.powi(j as i32 + 2);
        }
    }
    z
}

/// dz/dh, or NaN outside the conic domain.
pub fn sag_slope(surface: &Surface, h: f64) -> f64 {
    let c = surface.curvature();
    let k = surface.conic;
    let mut slope = 0.0;
    if c != 0.0 {
        let root = 1.0 - (1.0 + k) * c * c * h * h;
        if root <= 0.0 {
            return f64::NAN;
        }
        slope = c * h / root.sqrt();
    }
    for (j, &a) in surface.aspheric.iter().enumerate() {
        if a != 0.0 {
            slope += 2.0 * (j as f64 + 2.0) * a * h.powi(2 * j as i32 + 3);
        }
    }
    slope
}

/// Largest height at which a surface is defined.
pub fn surface_limit(surface: &Surface) -> f64 {
    let c = surface.curvature();
    let k = surface.conic;
    if c == 0.0 || 1.0 + k <= 0.0 { f64::INFINITY } else { 1.0 / ((1.0 + k) * c * c).sqrt() }
}

pub struct Hit {
    /// Distance along the (unit) ray direction.
    pub t: f64,
    pub normal: Vec3,
}

/// Ray–surface intersection in vertex coordinates, or None on a miss.
pub fn intersect(surface: &Surface, p: Vec3, d: Vec3) -> Option<Hit> {
    let c = surface.curvature();
    let k = surface.conic;
    // Quadric part: c(x² + y² + (1+k)z²) − 2z = 0
    let a = c * (d[0] * d[0] + d[1] * d[1] + (1.0 + k) * d[2] * d[2]);
    let b = c * (p[0] * d[0] + p[1] * d[1] + (1.0 + k) * p[2] * d[2]) - d[2];
    let f = c * (p[0] * p[0] + p[1] * p[1] + (1.0 + k) * p[2] * p[2]) - 2.0 * p[2];
    let discriminant = b * b - a * f;
    if discriminant < 0.0 {
        return None;
    }
    let denominator = b - discriminant.sqrt();
    if denominator == 0.0 {
        return None;
    }
    let mut t = -f / denominator;
    if !surface.is_plain_sphere() {
        // Newton refinement of z(t) − sag(r(t)) = 0 from the quadric (or vertex plane) solution.
        for _ in 0..30 {
            let (x, y, z) = (p[0] + t * d[0], p[1] + t * d[1], p[2] + t * d[2]);
            let r = x.hypot(y);
            let slope = sag_slope(surface, r);
            if !slope.is_finite() {
                return None;
            }
            let g = z - sag(surface, r);
            let dg = d[2] - if r > 0.0 { slope * (x * d[0] + y * d[1]) / r } else { 0.0 };
            if dg == 0.0 {
                return None;
            }
            let step = g / dg;
            t -= step;
            if step.abs() < 1e-12 {
                break;
            }
        }
    }
    let (x, y) = (p[0] + t * d[0], p[1] + t * d[1]);
    let r = x.hypot(y);
    let slope = sag_slope(surface, r);
    if !slope.is_finite() {
        return None;
    }
    let normal = if r > 0.0 { normalize([-slope * x / r, -slope * y / r, 1.0]) } else { [0.0, 0.0, 1.0] };
    Some(Hit { t, normal })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_sag_and_intersection_agree() {
        let surface = Surface::new(20.0, 1.0, "AIR");
        let expected = 20.0 - (400.0_f64 - 25.0).sqrt();
        assert!((sag(&surface, 5.0) - expected).abs() < 1e-12);
        let hit = intersect(&surface, [0.0, 5.0, -3.0], [0.0, 0.0, 1.0]).unwrap();
        assert!((hit.t - 3.0 - expected).abs() < 1e-12);
    }

    #[test]
    fn asphere_intersection_lands_on_the_surface() {
        let surface = Surface { conic: -0.7, aspheric: vec![1e-5, -2e-8], ..Surface::new(30.0, 1.0, "AIR") };
        let p = [0.3, 4.0, -5.0];
        let d = normalize([0.01, 0.05, 1.0]);
        let hit = intersect(&surface, p, d).unwrap();
        let (x, y, z) = (p[0] + hit.t * d[0], p[1] + hit.t * d[1], p[2] + hit.t * d[2]);
        assert!((z - sag(&surface, x.hypot(y))).abs() < 1e-11);
    }
}
