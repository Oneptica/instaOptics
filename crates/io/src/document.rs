//! The instaOptics lens document: JSON `{format, version, system}`. A bare system object also opens.

use instaoptics_core::LensSystem;
use serde_json::{Value, json};

use crate::IoError;

pub const FORMAT: &str = "instaoptics-lens";
pub const EXTENSION: &str = "iol";

pub fn serialize_document(system: &LensSystem) -> Result<String, IoError> {
    let mut text = serde_json::to_string_pretty(&json!({ "format": FORMAT, "version": 1, "system": system }))?;
    text.push('\n');
    Ok(text)
}

pub fn parse_document(text: &str) -> Result<LensSystem, IoError> {
    let value: Value = serde_json::from_str(text)?;
    let system = match value.get("format").and_then(Value::as_str) {
        Some(FORMAT) => value.get("system").cloned().ok_or_else(|| IoError::Format("the document has no system".into()))?,
        Some(other) => return Err(IoError::Format(format!("unknown document format {other}"))),
        None => value,
    };
    let system: LensSystem = serde_json::from_value(system)?;
    if system.surfaces.is_empty() {
        return Err(IoError::Format("the lens has no surfaces".into()));
    }
    Ok(system)
}
