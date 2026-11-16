//! Interface language. English text is the key; `i18n/<code>.tsv` holds `English<TAB>translation` lines. A string
//! missing from a catalog shows in English. Figures stay in English so a window and its exported PNG match.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Code, name in its own language, catalog. English (index 0) has no catalog.
pub const LANGUAGES: [(&str, &str, &str); 9] = [
    ("en", "English", ""),
    ("zh-CN", "简体中文", include_str!("../i18n/zh-CN.tsv")),
    ("zh-TW", "繁體中文", include_str!("../i18n/zh-TW.tsv")),
    ("ja", "日本語", include_str!("../i18n/ja.tsv")),
    ("ko", "한국어", include_str!("../i18n/ko.tsv")),
    ("de", "Deutsch", include_str!("../i18n/de.tsv")),
    ("fr", "Français", include_str!("../i18n/fr.tsv")),
    ("es", "Español", include_str!("../i18n/es.tsv")),
    ("ru", "Русский", include_str!("../i18n/ru.tsv")),
];

/// The language strings are looked up in, as an index into LANGUAGES.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn catalogs() -> &'static Vec<HashMap<&'static str, &'static str>> {
    static CATALOGS: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        LANGUAGES
            .iter()
            .map(|(_, _, text)| text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t')).filter(|(_, v)| !v.is_empty()).collect())
            .collect()
    })
}

/// The system language from the usual environment variables (LC_ALL, LC_MESSAGES, LANG, LANGUAGE).
fn system_language() -> usize {
    let Some(locale) = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"].iter().filter_map(|v| std::env::var(v).ok()).find(|v| !v.is_empty() && v != "C" && v != "POSIX") else {
        return 0;
    };
    let locale = locale.replace('_', "-").to_lowercase();
    // Chinese: Taiwan and Hong Kong use traditional characters.
    if locale.starts_with("zh") {
        return if locale.contains("tw") || locale.contains("hk") || locale.contains("hant") { 2 } else { 1 };
    }
    LANGUAGES.iter().position(|(code, ..)| locale.starts_with(code)).unwrap_or(0)
}

/// `code` is a LANGUAGES code, or "system" (or empty) for the system language.
pub fn set(code: &str) {
    let index = LANGUAGES.iter().position(|(c, ..)| *c == code).unwrap_or_else(system_language);
    ACTIVE.store(index, Ordering::Relaxed);
}

/// The interface text for an English string.
pub fn t(english: &'static str) -> &'static str {
    let active = ACTIVE.load(Ordering::Relaxed);
    if active == 0 {
        return english;
    }
    catalogs().get(active).and_then(|c| c.get(english).copied()).unwrap_or(english)
}

/// Inter for Latin text, the icon font, then a system CJK font after them so Chinese, Japanese and Korean text and file
/// names render. Inter and the icons are built in; egui's own fonts stay as the fallback for Cyrillic and symbols.
pub fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\malgun.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("inter".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "inter".into());
    if let Some(bytes) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("cjk".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
    egui_material_icons::initialize(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every string the interface translates, and every window title, is in every catalog.
    #[test]
    fn catalogs_cover_every_interface_string() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut keys: Vec<String> = crate::panels::Panel::ALL.iter().map(|p| p.title_en().to_string()).collect();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.ends_with("i18n.rs") {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap();
            for (at, _) in text.match_indices("t(\"") {
                // A call of t itself, not get(" or id_salt(".
                if text[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &text[at + 3..];
                if let Some(end) = rest.find("\")") {
                    keys.push(rest[..end].to_string());
                }
            }
        }
        let catalogs = catalogs();
        for (i, (code, ..)) in LANGUAGES.iter().enumerate().skip(1) {
            let missing: Vec<&String> = keys.iter().filter(|k| !catalogs[i].contains_key(k.as_str())).collect();
            assert!(missing.is_empty(), "{code} lacks {missing:?}");
        }
    }
}
