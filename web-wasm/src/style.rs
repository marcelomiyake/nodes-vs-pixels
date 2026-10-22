//! Value types for the generated design tokens and copy.

use crate::tokens::{FONT_FALLBACK, FONT_FAMILY};

/// A font size, line height and weight from `design/tokens.json`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f64,
    pub line_height: f64,
    pub weight: u16,
}

impl TextStyle {
    /// The CSS font shorthand for this style, as the canvas expects it.
    #[must_use]
    pub fn font(self) -> String {
        format!(
            "{} {}px {FONT_FAMILY}, {FONT_FALLBACK}",
            self.weight, self.size
        )
    }
}

/// A drop shadow from `design/tokens.json`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub color: &'static str,
}

/// The copy for one text field from `spec/copy.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldCopy {
    pub label: &'static str,
    pub placeholder: &'static str,
    pub input_type: &'static str,
    pub autocomplete: &'static str,
    pub hint: Option<&'static str>,
}
