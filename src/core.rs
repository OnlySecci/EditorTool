use regex::{Regex, RegexBuilder};
use similar::{ChangeTag, TextDiff};
use std::time::Duration;

#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub chars: usize,
    pub breaks: usize,
    pub bytes: usize,
}

impl Counts {
    pub fn of(text: &str) -> Self {
        Self {
            chars: text.chars().filter(|&c| c != '\u{fffc}').count(),
            breaks: text.bytes().filter(|&c| c == b'\n').count(),
            bytes: text.len() - text.matches('\u{fffc}').count() * 3,
        }
    }
    pub fn update(&mut self, inserted: &str, deleted: &str) {
        let a = Self::of(inserted);
        let b = Self::of(deleted);
        self.chars = self.chars.saturating_sub(b.chars) + a.chars;
        self.breaks = self.breaks.saturating_sub(b.breaks) + a.breaks;
        self.bytes = self.bytes.saturating_sub(b.bytes) + a.bytes;
    }
}

pub fn plain_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\0', "")
}

pub fn pattern(needle: &str, match_case: bool) -> Option<Regex> {
    if needle.is_empty() {
        return None;
    }
    RegexBuilder::new(&regex::escape(needle))
        .case_insensitive(!match_case)
        .build()
        .ok()
}

#[derive(Clone)]
pub struct Comparison {
    pub left_style: String,
    pub right_style: String,
    pub removed: usize,
    pub added: usize,
}

pub fn compare(left: &str, right: &str) -> Comparison {
    // Bound the expensive diff computation; huge inputs receive a coarse comparison.
    if left.len() + right.len() > 16 * 1024 * 1024 {
        let equal = left == right;
        return Comparison {
            left_style: (if equal { "A" } else { "B" }).repeat(left.len()),
            right_style: (if equal { "A" } else { "C" }).repeat(right.len()),
            removed: if equal { 0 } else { left.lines().count() },
            added: if equal { 0 } else { right.lines().count() },
        };
    }
    let diff = TextDiff::configure()
        .timeout(Duration::from_secs(2))
        .diff_lines(left, right);
    let mut result = Comparison {
        left_style: String::with_capacity(left.len()),
        right_style: String::with_capacity(right.len()),
        removed: 0,
        added: 0,
    };
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                result.left_style.extend(std::iter::repeat('A').take(len));
                result.right_style.extend(std::iter::repeat('A').take(len));
            }
            ChangeTag::Delete => {
                result.left_style.extend(std::iter::repeat('B').take(len));
                result.removed += 1;
            }
            ChangeTag::Insert => {
                result.right_style.extend(std::iter::repeat('C').take(len));
                result.added += 1;
            }
        }
    }
    result
}
