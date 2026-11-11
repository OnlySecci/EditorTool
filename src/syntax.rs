//! Lightweight lexical highlighting. Byte-aligned FLTK style buffers; no parser runtime.
use fltk::{
    enums::{Color, Font},
    text::StyleTableEntry,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    Plain,
    Python,
    PowerShell,
    Rust,
    JavaScript,
    Json,
    Shell,
    Cpp,
    Html,
    Css,
    Config,
    Sql,
}
impl Language {
    pub fn detect(path: Option<&Path>) -> Self {
        match path
            .and_then(Path::extension)
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "py" | "pyw" => Self::Python,
            "ps1" | "psm1" | "psd1" => Self::PowerShell,
            "rs" => Self::Rust,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => Self::JavaScript,
            "json" | "jsonc" => Self::Json,
            "sh" | "bash" | "zsh" => Self::Shell,
            "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" => Self::Cpp,
            "html" | "htm" | "xml" | "svg" => Self::Html,
            "css" | "scss" => Self::Css,
            "yaml" | "yml" | "toml" | "ini" | "conf" => Self::Config,
            "sql" => Self::Sql,
            _ => Self::Plain,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Plain => "Text",
            Self::Python => "Python",
            Self::PowerShell => "PowerShell",
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript / TypeScript",
            Self::Json => "JSON",
            Self::Shell => "Shell",
            Self::Cpp => "C / C++ / C# / Java",
            Self::Html => "HTML / XML",
            Self::Css => "CSS",
            Self::Config => "YAML / TOML / INI",
            Self::Sql => "SQL",
        }
    }
}

pub fn table() -> Vec<StyleTableEntry> {
    // A plain, B keyword, C string, D comment, E number, F function, G variable, H punctuation.
    [
        0xf0f0f3, 0xcf9fff, 0xa5d6a7, 0x818794, 0xf4c78d, 0x86caff, 0xf19dc2, 0xaeb7c7,
    ]
    .iter()
    .map(|&c| StyleTableEntry {
        color: Color::from_hex(c),
        font: Font::Courier,
        size: 16,
    })
    .collect()
}

fn keyword(word: &str, language: Language) -> bool {
    let words = match language {
        Language::Python=>"False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case",
        Language::PowerShell=>"begin break catch class continue data do dynamicparam else elseif end enum exit filter finally for foreach from function if in param process return switch throw trap try until using var while workflow parallel sequence true false null",
        Language::Rust=>"as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while",
        Language::JavaScript=>"async await break case catch class const continue debugger default delete do else export extends false finally for from function get if import in instanceof let new null of return set static super switch this throw true try typeof undefined var void while yield interface type implements private public readonly enum",
        Language::Cpp=>"abstract auto bool break case catch char class const constexpr continue default delete do double else enum explicit extern false final float for friend if import include inline int interface long namespace new null nullptr override package private protected public return short signed sizeof static string struct super switch template this throw true try typedef typename unsigned using virtual void volatile while",
        Language::Shell=>"if then elif else fi for in do done while until case esac function select time export local readonly declare return exit break continue source",
        Language::Sql=>"select from where join left right inner outer on as and or not null true false insert into values update set delete create table alter drop index order by group having limit offset distinct union all exists case when then else end asc desc primary key foreign references begin commit rollback",
        Language::Json|Language::Config=>"true false null yes no on off",
        _=>"",
    };
    words.split_ascii_whitespace().any(|w| {
        if matches!(
            language,
            Language::PowerShell | Language::Sql | Language::Config
        ) {
            w.eq_ignore_ascii_case(word)
        } else {
            w == word
        }
    })
}

pub fn highlight(text: &str, language: Language) -> Option<String> {
    if language == Language::Plain || text.len() > 4 * 1024 * 1024 {
        return None;
    }
    let b = text.as_bytes();
    let mut style = vec![b'A'; b.len()];
    let mut i = 0;
    let hash = matches!(
        language,
        Language::Python | Language::PowerShell | Language::Shell | Language::Config
    );
    let c_style = matches!(
        language,
        Language::Rust | Language::JavaScript | Language::Json | Language::Cpp | Language::Css
    );
    while i < b.len() {
        let start = i;
        let tail = &b[i..];
        let block = if language == Language::PowerShell && tail.starts_with(b"<#") {
            Some((2, b"#>".as_slice()))
        } else if c_style && tail.starts_with(b"/*") {
            Some((2, b"*/".as_slice()))
        } else if language == Language::Html && tail.starts_with(b"<!--") {
            Some((4, b"-->".as_slice()))
        } else {
            None
        };
        if let Some((open, end)) = block {
            i += open;
            while i < b.len() && !b[i..].starts_with(end) {
                i += 1;
            }
            i = (i + end.len()).min(b.len());
            style[start..i].fill(b'D');
            continue;
        }
        if (hash && b[i] == b'#')
            || (c_style && tail.starts_with(b"//"))
            || (language == Language::Sql && tail.starts_with(b"--"))
        {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            style[start..i].fill(b'D');
            continue;
        }
        if b[i] == b'\''
            || b[i] == b'"'
            || (matches!(language, Language::JavaScript | Language::Shell) && b[i] == b'`')
        {
            let quote = b[i];
            let triple = language == Language::Python
                && i + 2 < b.len()
                && b[i + 1] == quote
                && b[i + 2] == quote;
            let width = if triple { 3 } else { 1 };
            i += width;
            while i < b.len() {
                if b[i] == b'\\' || (language == Language::PowerShell && b[i] == b'`') {
                    i = (i + 2).min(b.len());
                    continue;
                }
                if b[i] == quote
                    && (!triple || (i + 2 < b.len() && b[i + 1] == quote && b[i + 2] == quote))
                {
                    i += width;
                    break;
                }
                i += 1;
            }
            style[start..i].fill(b'C');
            continue;
        }
        if b[i] == b'$' && matches!(language, Language::PowerShell | Language::Shell) {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b"_:{}?".contains(&b[i])) {
                i += 1;
            }
            style[start..i].fill(b'G');
            continue;
        }
        if b[i].is_ascii_digit() {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            style[start..i].fill(b'E');
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'_'
                    || (language == Language::PowerShell && b[i] == b'-'))
            {
                i += 1;
            }
            let word = &text[start..i];
            let mut next = i;
            while next < b.len() && b[next].is_ascii_whitespace() {
                next += 1;
            }
            let kind = if keyword(word, language) {
                b'B'
            } else if (next < b.len() && b[next] == b'(')
                || (language == Language::PowerShell && word.contains('-'))
            {
                b'F'
            } else {
                b'A'
            };
            style[start..i].fill(kind);
            continue;
        }
        if b"{}[]():,;=+-*/<>!&|".contains(&b[i]) {
            style[i] = b'H';
        }
        i += 1;
    }
    // A..H are all ASCII; one style byte is kept for every UTF-8 source byte.
    Some(String::from_utf8(style).expect("ASCII styles"))
}
