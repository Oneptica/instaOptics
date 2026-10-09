//! Sequential lens model: rotationally symmetric surfaces along z, lengths in mm, wavelengths in µm.
//! The first surface vertex sits at z = 0; the last thickness reaches the image plane.

use serde::{Deserialize, Serialize};

use crate::glass;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    /// 0 means a flat surface.
    pub radius: f64,
    /// Distance to the next surface, or to the image plane after the last surface.
    pub thickness: f64,
    /// Medium after the surface: "" / "AIR", a catalog glass, or a model glass "nd/vd".
    #[serde(default)]
    pub material: String,
    /// Fixed clear semi-aperture; None means automatic (no clipping).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semi_diameter: Option<f64>,
    /// Conic constant k (0 sphere, −1 paraboloid).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub conic: f64,
    /// Even asphere coefficients A4, A6, A8, A10, …
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aspheric: Vec<f64>,
    /// x, y offset of this surface only (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decenter: Option<[f64; 2]>,
    /// Rotation about x then y of this surface only (degrees).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt: Option<[f64; 2]>,
    /// Parameters the optimizer may change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<Variables>,
    /// Glass perturbation of the medium after this surface (tolerancing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glass_offset: Option<GlassOffset>,
    /// Thin-film coating, e.g. "MgF2@550", "1.38:99.6,2.1:60" (index:thickness in nm, incident side first) or a metal
    /// ("AL", "M:0.96+6.69i"); see `coating::parse`. None is uncoated (Fresnel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coating: Option<String>,
    /// Computes the thickness (marginal ray height solve) or copies a parameter from another surface (pickup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve: Option<crate::configs::Solve>,
    /// Makes this a coordinate break: it does not interact with rays but moves and rotates the coordinate system of
    /// every following surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinate_break: Option<CoordinateBreak>,
}

/// Decenter, then tilt about x, y and z (degrees), as in OpticStudio's coordinate break with order 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoordinateBreak {
    pub decenter: [f64; 2],
    pub tilt: [f64; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlassOffset {
    /// Δnd.
    pub index: f64,
    /// Fractional change of vd.
    pub abbe: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variables {
    #[serde(default, skip_serializing_if = "is_false")]
    pub radius: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub thickness: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub conic: bool,
    /// One flag per even asphere coefficient (A4, A6, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aspheric: Vec<bool>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Objective {
    /// RMS spot radius about each field's polychromatic centroid (mm).
    #[default]
    Spot,
    /// RMS wavefront error about each field and wavelength's mean (waves).
    Wavefront,
    /// No image-quality term: only the operands and constraints.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperandKind {
    /// Effective focal length (mm).
    Efl,
    /// First vertex to image (mm).
    TotalTrack,
    /// Paraxial back focal length (mm).
    BackFocus,
    /// Paraxial image-space F/#.
    FNumber,
    /// Paraxial image height of the largest field (mm).
    ImageHeight,
    /// Chief ray angle at the image for a field (degrees).
    ChiefRayAngle,
    /// Real chief ray distortion for a field (%).
    Distortion,
    /// Thickness of a surface (mm).
    Thickness,
    /// Radius of a surface (mm).
    Radius,
    /// RMS spot radius for a field (mm).
    SpotRadius,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Relation {
    #[default]
    Equal,
    AtMost,
    AtLeast,
}

fn one() -> f64 {
    1.0
}

/// One line of the merit function: a quantity, how it should relate to the target, and its weight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operand {
    pub kind: OperandKind,
    #[serde(default)]
    pub relation: Relation,
    pub target: f64,
    #[serde(default = "one")]
    pub weight: f64,
    /// Surface index for thickness and radius operands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<usize>,
    /// Field index for chief ray, distortion and spot operands; None means every field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<usize>,
    /// Restricts the operand to one configuration; None means every configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<usize>,
}

/// Merit function settings and constraints for the optimizer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationSettings {
    #[serde(default)]
    pub objective: Objective,
    /// Hexapolar pupil rings per field and wavelength.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rings: Option<usize>,
    /// mm, first vertex to image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_track: Option<f64>,
    /// mm, last vertex to image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_back_focus: Option<f64>,
    /// Degrees at the image, over all fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chief_ray_angle: Option<f64>,
    /// Minimum centre and edge thickness of glass (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_glass_center: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_glass_edge: Option<f64>,
    /// Minimum centre and edge air gap (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_air: Option<f64>,
    /// User-defined merit function lines, added to the objective and constraints above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operands: Vec<Operand>,
}

impl Surface {
    pub fn new(radius: f64, thickness: f64, material: &str) -> Self {
        Surface { radius, thickness, material: material.to_string(), ..Default::default() }
    }

    pub fn curvature(&self) -> f64 {
        if self.radius == 0.0 || self.coordinate_break.is_some() { 0.0 } else { 1.0 / self.radius }
    }

    pub fn is_mirror(&self) -> bool {
        glass::is_mirror(&self.material) && self.coordinate_break.is_none()
    }

    /// Whether the medium after this surface is a glass (not air, a mirror or a coordinate break).
    pub fn is_glass(&self) -> bool {
        self.coordinate_break.is_none() && !glass::is_air(&self.material) && !glass::is_mirror(&self.material)
    }

    pub fn is_plain_sphere(&self) -> bool {
        self.conic == 0.0 && self.aspheric.iter().all(|&a| a == 0.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApertureType {
    #[default]
    EntrancePupilDiameter,
    /// Image space F/# = EFL / EPD.
    ImageFNumber,
    /// Paraxial working F/# = 1 / (2 n' |u'|) for the actual object.
    WorkingFNumber,
    /// Object space numerical aperture (finite object).
    ObjectNa,
    /// The stop semi-diameter, in mm.
    FloatByStop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldType {
    /// Half-field angle in degrees.
    #[default]
    Angle,
    /// Object height in mm (finite object).
    ObjectHeight,
    /// Paraxial image height in mm.
    ImageHeight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LensSystem {
    pub name: String,
    /// Distance from the object to surface 1; None means infinity.
    pub object_distance: Option<f64>,
    pub surfaces: Vec<Surface>,
    pub stop_index: usize,
    /// The working entrance pupil diameter. With another aperture type it is a starting value that
    /// `resolved()` replaces.
    pub entrance_pupil_diameter: f64,
    #[serde(default)]
    pub aperture_type: ApertureType,
    /// F/#, NA or stop semi-diameter for the other aperture types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aperture_value: Option<f64>,
    /// Field values in units of `field_type`.
    pub fields: Vec<f64>,
    #[serde(default)]
    pub field_type: FieldType,
    pub wavelengths: Vec<f64>,
    /// Index into `wavelengths`.
    pub primary_wavelength: usize,
    /// Aim real rays at the stop instead of paraxially at the entrance pupil.
    #[serde(default)]
    pub ray_aiming: bool,
    /// Effective focal length the optimizer holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_efl: Option<f64>,
    #[serde(default)]
    pub optimization: OptimizationSettings,
    /// Glasses defined in the system itself (from loaded catalogs); they take precedence over the built-in catalog.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glasses: Vec<glass::GlassDef>,
    /// Multi-configuration data: parameters that take a different value in each configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configs: Option<crate::configs::Configurations>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LensError(pub String);

impl std::fmt::Display for LensError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LensError {}

impl LensSystem {
    pub fn primary_wavelength(&self) -> f64 {
        self.wavelengths.get(self.primary_wavelength).or(self.wavelengths.first()).copied().unwrap_or(glass::LINE_D)
    }

    pub fn last(&self) -> usize {
        self.surfaces.len() - 1
    }

    pub fn stop(&self) -> usize {
        self.stop_index.min(self.last())
    }

    /// Global z of each surface vertex.
    pub fn surface_z(&self) -> Vec<f64> {
        let mut z = 0.0;
        self.surfaces.iter().map(|surface| { let current = z; z += surface.thickness; current }).collect()
    }

    pub fn image_z(&self) -> f64 {
        self.surfaces.iter().map(|surface| surface.thickness).sum()
    }

    pub fn max_field(&self) -> f64 {
        self.fields.iter().fold(0.0_f64, |max, field| max.max(field.abs()))
    }

    /// Refractive index of a material at a wavelength: glasses defined in the system first, then the built-in catalog.
    pub fn index_of(&self, material: &str, wavelength: f64) -> Option<f64> {
        let name = material.trim();
        if let Some(glass) = self.glasses.iter().find(|g| g.name.trim().eq_ignore_ascii_case(name)) {
            return glass.index(wavelength);
        }
        glass::refractive_index(material, wavelength)
    }

    pub fn is_known_material(&self, material: &str) -> bool {
        glass::is_known_material(material) || self.glasses.iter().any(|g| g.name.trim().eq_ignore_ascii_case(material.trim()))
    }

    /// Indices of the media: [object space, after surface 1, …, after surface k]. After an odd number of mirrors the
    /// indices are negative (light travels towards −z), which keeps the paraxial equations unchanged.
    pub fn medium_indices(&self, wavelength: f64) -> Result<Vec<f64>, LensError> {
        let mut n = Vec::with_capacity(self.surfaces.len() + 1);
        n.push(1.0);
        let mut sign = 1.0;
        for (i, surface) in self.surfaces.iter().enumerate() {
            let previous = *n.last().unwrap();
            if surface.coordinate_break.is_some() {
                n.push(previous);
                continue;
            }
            if surface.is_mirror() {
                sign = -sign;
                n.push(-previous);
                continue;
            }
            let index = sign * self.index_of(&surface.material, wavelength)
                .ok_or_else(|| LensError(format!("Unknown material \"{}\" on surface {}", surface.material, i + 1)))?;
            n.push(match surface.glass_offset {
                // Shift nd and scale the dispersion so vd changes by the requested fraction.
                Some(offset) => {
                    let nd = self.index_of(&surface.material, glass::LINE_D).unwrap_or(index);
                    sign * (nd + offset.index + (index.abs() - nd) / (1.0 + offset.abbe))
                }
                None => index,
            });
        }
        Ok(n)
    }

    /// Checks that the system can be traced; returns the first problem found.
    pub fn validate(&self) -> Result<(), LensError> {
        let fail = |message: &str| Err(LensError(message.to_string()));
        if self.surfaces.is_empty() {
            return fail("The system has no surfaces");
        }
        if !(self.entrance_pupil_diameter > 0.0) {
            return fail("Entrance pupil diameter must be positive");
        }
        if self.wavelengths.is_empty() {
            return fail("Add at least one wavelength");
        }
        if self.wavelengths.iter().any(|&w| !(w > 0.0)) {
            return fail("Wavelengths must be positive");
        }
        if self.fields.is_empty() {
            return fail("Add at least one field");
        }
        if let Some(distance) = self.object_distance {
            if !(distance > 0.0) {
                return fail("Object distance must be positive");
            }
        }
        for &wavelength in &self.wavelengths {
            self.medium_indices(wavelength)?;
        }
        for (i, surface) in self.surfaces.iter().enumerate() {
            if let Some(text) = surface.coating.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                crate::coating::parse(text, self.primary_wavelength()).map_err(|e| LensError(format!("Coating on surface {}: {e}", i + 1)))?;
            }
        }
        crate::configs::validate(self)?;
        Ok(())
    }
}

// ---------- sample systems ----------

const VISIBLE: [f64; 3] = [0.4861327, 0.5875618, 0.6562725];

fn visible(name: &str, stop_index: usize, entrance_pupil_diameter: f64, fields: &[f64], surfaces: Vec<Surface>) -> LensSystem {
    LensSystem {
        name: name.to_string(),
        object_distance: None,
        surfaces,
        stop_index,
        entrance_pupil_diameter,
        fields: fields.to_vec(),
        wavelengths: VISIBLE.to_vec(),
        primary_wavelength: 1,
        ray_aiming: true,
        aperture_type: ApertureType::EntrancePupilDiameter,
        aperture_value: None,
        field_type: FieldType::Angle,
        target_efl: None,
        optimization: OptimizationSettings::default(),
        glasses: Vec::new(),
        configs: None,
    }
}

pub fn cooke_triplet() -> LensSystem {
    visible("Cooke triplet", 3, 10.0, &[0.0, 14.0, 20.0], vec![
        Surface::new(22.01359, 3.25896, "N-SK16"),
        Surface::new(-435.76044, 6.00755, "AIR"),
        Surface::new(-22.21328, 0.99997, "F2"),
        Surface::new(20.29192, 4.75041, "AIR"),
        Surface::new(79.6836, 2.95208, "N-SK16"),
        Surface::new(-18.39533, 42.20778, "AIR"),
    ])
}

pub fn singlet() -> LensSystem {
    visible("Plano-convex singlet", 0, 10.0, &[0.0, 3.0, 5.0], vec![
        Surface::new(51.68, 4.0, "N-BK7"),
        Surface::new(0.0, 97.3629, "AIR"),
    ])
}

pub fn achromat() -> LensSystem {
    visible("Achromatic doublet", 0, 20.0, &[0.0, 1.5, 3.0], vec![
        Surface::new(46.66, 6.0, "N-BK7"),
        Surface::new(-43.3, 2.5, "F2"),
        Surface::new(-485.78, 94.2692, "AIR"),
    ])
}

pub fn aspheric_singlet() -> LensSystem {
    LensSystem {
        name: "Aspheric singlet".to_string(),
        object_distance: None,
        surfaces: vec![
            Surface { conic: -0.73798, ..Surface::new(28.5976, 7.0, "N-BK7") },
            Surface::new(-245.621, 45.8126, "AIR"),
        ],
        stop_index: 0,
        entrance_pupil_diameter: 20.0,
        fields: vec![0.0, 1.0, 2.0],
        wavelengths: vec![glass::LINE_D],
        primary_wavelength: 0,
        ray_aiming: true,
        aperture_type: ApertureType::EntrancePupilDiameter,
        aperture_value: None,
        field_type: FieldType::Angle,
        target_efl: None,
        optimization: OptimizationSettings::default(),
        glasses: Vec::new(),
        configs: None,
    }
}

pub fn landscape_lens() -> LensSystem {
    visible("Landscape lens", 0, 8.0, &[0.0, 10.0, 15.0], vec![
        Surface::new(0.0, 16.8227, "AIR"),
        Surface::new(-73.0962, 4.0, "N-BK7"),
        Surface::new(-30.8395, 99.4881, "AIR"),
    ])
}

/// (id, system) for each built-in sample.
pub fn samples() -> Vec<(&'static str, LensSystem)> {
    vec![
        ("singlet", singlet()),
        ("asphere", aspheric_singlet()),
        ("landscape", landscape_lens()),
        ("achromat", achromat()),
        ("cooke", cooke_triplet()),
    ]
}
