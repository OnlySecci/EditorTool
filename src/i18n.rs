//! Embedded UI catalogs. Language changes take effect on the next application start.
//! Document contents, file paths and protocol identifiers are never translated.
use std::{collections::HashMap, sync::OnceLock};

pub const LANGUAGES: &[&str] = &[
    "English",
    "Deutsch",
    "Français",
    "Italiano",
    "Español",
    "Português",
    "Nederlands",
    "Polski",
    "Türkçe",
    "Українська",
];
static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

pub fn init(language: u8) {
    let data = match language {
        1 => include_str!("../locales/de.json"),
        2 => include_str!("../locales/fr.json"),
        3 => include_str!("../locales/it.json"),
        4 => include_str!("../locales/es.json"),
        5 => include_str!("../locales/pt.json"),
        6 => include_str!("../locales/nl.json"),
        7 => include_str!("../locales/pl.json"),
        8 => include_str!("../locales/tr.json"),
        9 => include_str!("../locales/uk.json"),
        _ => "{}",
    };
    // Catalogs are bundled, never downloaded or read from user-supplied files.
    let _ = CATALOG.set(serde_json::from_str(data).unwrap_or_default());
}

pub fn tr(key: &str) -> &str {
    CATALOG
        .get()
        .and_then(|c| c.get(key))
        .map(String::as_str)
        .unwrap_or(key)
}

pub fn menu_path(path: &str) -> String {
    path.split('/')
        .map(|part| {
            let plain = part.replace('&', "");
            let (base, suffix) = plain
                .strip_suffix('…')
                .map(|s| (s, "…"))
                .unwrap_or((&plain, ""));
            let marker = if part.starts_with('&') { "&" } else { "" };
            // FLTK treats slashes as hierarchy separators even in translated labels.
            format!("{marker}{}{suffix}", tr(base).replace('/', " · "))
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Substitute already formatted values into a translated template, leaving values opaque.
pub fn formatted(template: &str, rendered: String) -> String {
    let translated = tr(template);
    if translated == template {
        return rendered;
    }
    fn parts(text: &str) -> Vec<(String, Option<String>)> {
        let mut result = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars().peekable();
        let mut positional = 0;
        while let Some(c) = chars.next() {
            if (c == '{' || c == '}') && chars.peek() == Some(&c) {
                literal.push(c);
                chars.next();
            } else if c == '{' {
                let mut field = String::new();
                for ch in chars.by_ref() {
                    if ch == '}' {
                        break;
                    }
                    field.push(ch);
                }
                let name = field.split(':').next().unwrap_or("");
                let name = if name.is_empty() {
                    let n = positional.to_string();
                    positional += 1;
                    n
                } else {
                    name.to_owned()
                };
                result.push((std::mem::take(&mut literal), Some(name)));
            } else {
                literal.push(c);
            }
        }
        result.push((literal, None));
        result
    }
    static PATTERNS: OnceLock<std::sync::Mutex<HashMap<String, regex::Regex>>> = OnceLock::new();
    let source = parts(template);
    let mut patterns = PATTERNS.get_or_init(Default::default).lock().unwrap();
    let regex = patterns.entry(template.to_owned()).or_insert_with(|| {
        let mut pattern = String::from("(?s)\\A");
        for (literal, field) in &source {
            pattern.push_str(&regex::escape(literal));
            if field.is_some() {
                pattern.push_str("(.*?)");
            }
        }
        pattern.push_str("\\z");
        regex::Regex::new(&pattern).expect("escaped bundled format template")
    });
    let Some(captures) = regex.captures(&rendered) else {
        return rendered;
    };
    let mut fields = HashMap::new();
    for (index, name) in source.iter().filter_map(|(_, f)| f.as_ref()).enumerate() {
        fields.insert(
            name.as_str(),
            captures.get(index + 1).map(|m| m.as_str()).unwrap_or(""),
        );
    }
    let mut output = String::new();
    for (literal, field) in parts(translated) {
        output.push_str(&literal);
        if let Some(field) = field {
            let Some(value) = fields.get(field.as_str()) else {
                return rendered;
            };
            output.push_str(value);
        }
    }
    output
}

#[macro_export]
macro_rules! tr_format {
    ($template:literal $($arguments:tt)*) => {
        $crate::i18n::formatted($template, format!($template $($arguments)*))
    };
}
