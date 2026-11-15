//! instaOptics files: lens documents (.iol), Zemax OpticStudio .zmx import and export, .agf glass catalogs and CSV.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod agf;
pub mod csv;
pub mod document;
pub mod zmx;

pub use document::{parse_document, serialize_document};

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("{0}")]
    Format(String),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Decodes text that OpticStudio may write as UTF-16 (with or without a byte order mark) or UTF-8.
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |data: &[u8], big: bool| {
        let units: Vec<u16> = data.chunks_exact(2).map(|c| if big { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xff, 0xfe, rest @ ..] => utf16(rest, false),
        [0xfe, 0xff, rest @ ..] => utf16(rest, true),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        // UTF-16 without a BOM shows up as every second byte being zero.
        [_, 0, _, 0, ..] => utf16(bytes, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Shortest round-trip text for a number the user typed; non-finite values become 0.
pub(crate) fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let rounded: f64 = format!("{value:.12e}").parse().unwrap_or(value);
    format!("{rounded}")
}
