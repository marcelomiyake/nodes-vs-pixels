//! Single-line text editing as Chrome's text inputs do it. Offsets count the
//! value's characters. The caret stops between grapheme clusters, and moves
//! by word between the words the browser's segmenter finds, as Chrome's
//! word movement on Linux and macOS chooses among them.

use std::ops::Range;

use form_core::Field;
use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup};

use crate::copy;
use crate::layout::{BORDER, Rect};
use crate::text::Measure;
use crate::tokens::{spacing, text};

/// How Chrome masks each grapheme cluster of a password.
const MASK: char = '\u{2022}';
/// How far apart, in pixels, the edges of adjacent characters may be.
pub const TOUCHING: f64 = 0.5;

/// Where a text input lays out and clips its value: inside its border and
/// horizontal padding.
#[must_use]
pub fn content_box(input: Rect) -> Rect {
    let inset = BORDER + spacing::CONTROL_X;
    Rect::new(
        input.x + inset,
        input.y + BORDER,
        input.width - 2.0 * inset,
        input.height - 2.0 * BORDER,
    )
}

/// Pasted text as a single-line input takes it: each line break becomes a
/// space.
#[must_use]
pub fn single_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

/// Whether `field` is a password, which its input masks.
fn is_password(field: Field) -> bool {
    copy::field(field.key()).is_some_and(|copy| copy.input_type == "password")
}

/// What an input shows for `value`: the value, or for a password one mask
/// per grapheme cluster.
#[must_use]
pub fn shown(field: Field, value: &str, measure: &impl Measure) -> String {
    if is_password(field) {
        MASK.to_string().repeat(measure.graphemes(value).len())
    } else {
        value.to_owned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Word,
    Punctuation,
    Space,
}

/// A text input's value as the browser lays it out: masked for a password,
/// with where each character sits and where the caret can stop.
#[derive(Clone, Debug, PartialEq)]
pub struct InputText {
    value: String,
    /// What the input shows: the value, or one mask per grapheme cluster.
    pub shown: String,
    /// The left and right edges of each shown character, from where the
    /// text starts, in logical order.
    pub bounds: Vec<(f64, f64)>,
    masked: bool,
    /// The value's characters.
    chars: Vec<char>,
    /// The offset after each grapheme cluster of the value.
    clusters: Vec<usize>,
    /// Where the segmenter's words and the pieces between them start, and
    /// the value's end.
    boundaries: Vec<usize>,
    /// Words, runs of punctuation and runs of spaces, in order.
    segments: Vec<(Range<usize>, Kind)>,
}

impl InputText {
    pub fn new(field: Field, value: &str, measure: &impl Measure) -> Self {
        let masked = is_password(field);
        let mut end = 0;
        let clusters: Vec<usize> = measure
            .graphemes(value)
            .iter()
            .map(|cluster| {
                end += cluster.chars().count();
                end
            })
            .collect();
        let shown = if masked {
            MASK.to_string().repeat(clusters.len())
        } else {
            value.to_owned()
        };
        // Chrome moves over a password as one word, hiding where words are.
        let words = if masked {
            vec![value.to_owned()]
        } else {
            measure.words(value)
        };
        let mut start = 0;
        let boundaries = std::iter::once(0)
            .chain(words.iter().map(|word| {
                start += word.chars().count();
                start
            }))
            .collect();
        Self {
            value: value.to_owned(),
            bounds: measure.char_bounds(&shown, text::BODY),
            shown,
            masked,
            chars: value.chars().collect(),
            clusters,
            boundaries,
            segments: segments(&words),
        }
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The value's length in characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.clusters.last().copied().unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clusters.is_empty()
    }

    /// The shown offset of the value's offset `offset`: a mask stands for a
    /// whole cluster.
    #[must_use]
    pub fn shown_offset(&self, offset: usize) -> usize {
        if self.masked {
            self.clusters
                .iter()
                .take_while(|&&end| end <= offset)
                .count()
        } else {
            offset
        }
    }

    /// Where the caret before the value's offset `offset` sits, from where
    /// the text starts.
    #[must_use]
    pub fn caret_x(&self, offset: usize) -> f64 {
        caret_x(&self.shown, &self.bounds, self.shown_offset(offset))
    }

    /// How wide the text is laid out.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.bounds
            .iter()
            .map(|&(_, right)| right)
            .fold(0.0, f64::max)
    }

    /// The caret stop nearest `x`, measured from where the text starts.
    #[must_use]
    pub fn offset_at(&self, x: f64) -> usize {
        std::iter::once(0)
            .chain(self.clusters.iter().copied())
            .min_by(|&a, &b| {
                let distance = |offset| (self.caret_x(offset) - x).abs();
                distance(a).total_cmp(&distance(b))
            })
            .unwrap_or(0)
    }

    /// What a double click at `x` selects, as Chrome does from the caret
    /// stop nearest `x`: the word, punctuation or spaces after it, or before
    /// it at the end, or a whole password.
    #[must_use]
    pub fn word_at(&self, x: f64) -> Range<usize> {
        let offset = self.offset_at(x);
        self.segments
            .iter()
            .find(|(range, _)| range.contains(&offset))
            .or_else(|| self.segments.last())
            .map_or(0..0, |(range, _)| range.clone())
    }

    fn next_cluster(&self, offset: usize) -> usize {
        self.clusters
            .iter()
            .copied()
            .find(|&end| end > offset)
            .unwrap_or(self.len())
    }

    fn previous_cluster(&self, offset: usize) -> usize {
        self.clusters
            .iter()
            .copied()
            .rev()
            .find(|&end| end < offset)
            .unwrap_or(0)
    }

    /// Where moving a word right from `offset` stops, as Chrome's next word
    /// position finds it: at the first boundary after a word, unless
    /// punctuation follows, which then goes with the word if a letter or
    /// digit ends it, and otherwise joins the run of punctuation.
    fn next_word_end(&self, offset: usize) -> usize {
        for &runner in self.boundaries.iter().filter(|&&runner| runner > offset) {
            let Some(&after) = self.chars.get(runner) else {
                break;
            };
            let before = self.chars[runner - 1];
            if is_word_boundary(after) {
                if is_alphanumeric(before) {
                    return runner;
                }
            } else if is_word_break(before) {
                return runner;
            }
        }
        self.len()
    }

    /// Where moving a word left from `offset` stops, as Chrome's previous
    /// word position finds it: at the start of a word or of a run of
    /// punctuation, unless a letter or digit ends right before it.
    fn previous_word_start(&self, offset: usize) -> usize {
        let mut punctuation = None;
        for &runner in self
            .boundaries
            .iter()
            .rev()
            .filter(|&&runner| runner < offset)
        {
            let after = self.chars[runner];
            if is_word_boundary(after) {
                if runner > 0 && is_alphanumeric(self.chars[runner - 1]) {
                    return runner;
                }
                punctuation = Some(runner);
            } else if let Some(start) = punctuation {
                return start;
            } else if is_word_break(after) {
                return runner;
            }
        }
        0
    }

    fn byte(&self, offset: usize) -> usize {
        self.value
            .char_indices()
            .nth(offset)
            .map_or(self.value.len(), |(index, _)| index)
    }
}

/// Chrome's word movement looks at the UTF-16 code units beside a boundary,
/// where a character outside the Basic Multilingual Plane is a surrogate.
fn is_surrogate(c: char) -> bool {
    c.len_utf16() == 2
}

fn category(c: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(c)
}

/// A letter or decimal digit, as ICU's `u_isalnum` has it.
fn is_alphanumeric(c: char) -> bool {
    let category = category(c);
    !is_surrogate(c)
        && (GeneralCategoryGroup::Letter.contains(category)
            || category == GeneralCategory::DecimalNumber)
}

/// Punctuation or a symbol, which Chrome runs together between words.
fn is_word_boundary(c: char) -> bool {
    let category = category(c);
    is_surrogate(c)
        || GeneralCategoryGroup::Punctuation.contains(category)
        || GeneralCategoryGroup::Symbol.contains(category)
}

/// A printable character other than a space, which a word stop follows.
fn is_word_break(c: char) -> bool {
    is_surrogate(c)
        || c == '_'
        || !(GeneralCategoryGroup::Other.contains(category(c))
            || matches!(c, ' ' | '\u{A0}' | '\t'))
}

/// The value's words split into words, runs of punctuation and runs of
/// spaces.
fn segments(words: &[String]) -> Vec<(Range<usize>, Kind)> {
    let mut all: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut start = 0;
    for word in words {
        let end = start + word.chars().count();
        let kind = if word.chars().all(char::is_whitespace) {
            Kind::Space
        } else if word.chars().any(char::is_alphanumeric) {
            Kind::Word
        } else {
            Kind::Punctuation
        };
        match all.last_mut() {
            Some((range, last)) if *last == kind && kind != Kind::Word => range.end = end,
            _ => all.push((start..end, kind)),
        }
        start = end;
    }
    all
}

/// Where Chrome draws the caret before character `offset`: between the
/// characters on either side where they touch, and where the direction
/// changes, beside the one that runs left to right like the line. It sits
/// at the line's left edge before the first character and at its right edge
/// after the last.
fn caret_x(text: &str, bounds: &[(f64, f64)], offset: usize) -> f64 {
    let Some(&(next_left, next_right)) = bounds.get(offset) else {
        return bounds.iter().map(|&(_, right)| right).fold(0.0, f64::max);
    };
    let Some(&(before_left, before_right)) = offset.checked_sub(1).and_then(|i| bounds.get(i))
    else {
        return 0.0;
    };
    if (before_right - next_left).abs() <= TOUCHING {
        before_right
    } else if (before_left - next_right).abs() <= TOUCHING {
        before_left
    } else if text.chars().nth(offset - 1).is_some_and(is_rtl) {
        // Leaving a right-to-left run.
        next_left
    } else {
        // Entering one.
        before_right
    }
}

/// Characters of the scripts written right to left (bidirectional classes
/// R and AL, with the marks of their blocks).
fn is_rtl(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{08FF}'     // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic
        | '\u{FB1D}'..='\u{FDFF}'   // Hebrew and Arabic presentation forms
        | '\u{FE70}'..='\u{FEFF}'   // Arabic presentation forms B
        | '\u{10800}'..='\u{10FFF}' // Historic right-to-left scripts
        | '\u{1E800}'..='\u{1EFFF}' // Mende Kikakui, Adlam
    )
}

/// Caret movements, as the arrow, Home and End keys make them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

/// The selection and scroll of one text input. The selection runs from
/// `anchor`, where it started, to `head`, where the caret is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Editor {
    pub anchor: usize,
    pub head: usize,
    /// How far the text is scrolled to the left (`scrollLeft`).
    pub scroll: f64,
}

impl Editor {
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// Selects `range`, with the caret at its end.
    pub fn select(&mut self, range: Range<usize>) {
        self.anchor = range.start;
        self.head = range.end;
    }

    /// Puts the caret at `offset`, or extends the selection to it.
    pub fn place(&mut self, offset: usize, extend: bool) {
        self.head = offset;
        if !extend {
            self.anchor = offset;
        }
    }

    pub fn go(&mut self, text: &InputText, motion: Motion, extend: bool) {
        let range = self.range();
        let offset = match motion {
            // Without Shift, Left and Right collapse a selection to its edge.
            Motion::Left if !extend && !range.is_empty() => range.start,
            Motion::Right if !extend && !range.is_empty() => range.end,
            Motion::Left => text.previous_cluster(self.head),
            Motion::Right => text.next_cluster(self.head),
            Motion::WordLeft => text.previous_word_start(self.head),
            Motion::WordRight => text.next_word_end(self.head),
            Motion::Start => 0,
            Motion::End => text.len(),
        };
        self.place(offset, extend);
    }

    /// The selected part of the value.
    #[must_use]
    pub fn selected<'a>(&self, text: &'a InputText) -> &'a str {
        let range = self.range();
        &text.value[text.byte(range.start)..text.byte(range.end)]
    }

    /// Replaces the selection with `insert` and puts the caret after it;
    /// returns the new value.
    pub fn replace(&mut self, text: &InputText, insert: &str) -> String {
        let range = self.range();
        let value = &text.value;
        let (start, end) = (text.byte(range.start), text.byte(range.end));
        self.place(range.start + insert.chars().count(), false);
        format!("{}{insert}{}", &value[..start], &value[end..])
    }

    /// Deletes the selection, or else from the caret back or `forward` by a
    /// character or a `word`; returns the new value, if anything was deleted.
    pub fn delete(&mut self, text: &InputText, forward: bool, word: bool) -> Option<String> {
        let mut range = self.range();
        if range.is_empty() {
            let to = match (forward, word) {
                // Backspace deletes a character, such as an accent typed after
                // its letter.
                (false, false) if self.head > 0 => self.head - 1,
                (false, false) => 0,
                (false, true) => text.previous_word_start(self.head),
                (true, false) => text.next_cluster(self.head),
                (true, true) => text.next_word_end(self.head),
            };
            range = to.min(self.head)..to.max(self.head);
        }
        if range.is_empty() {
            return None;
        }
        self.select(range);
        Some(self.replace(text, ""))
    }

    /// Scrolls the least that shows the caret in a content box `width`
    /// wide, by whole pixels as Chrome does, and never past the text's end.
    pub fn reveal(&mut self, text: &InputText, width: f64) {
        let x = text.caret_x(self.head);
        if x - self.scroll > width {
            self.scroll = (x - width).ceil();
        } else if x < self.scroll {
            self.scroll = x.floor();
        }
        let end = (text.width() - width).ceil().max(0.0);
        self.scroll = self.scroll.clamp(0.0, end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::testing::FixedMeasure;

    fn name(value: &str) -> InputText {
        InputText::new(Field::FullName, value, &FixedMeasure)
    }

    fn editor(anchor: usize, head: usize) -> Editor {
        let mut editor = Editor::default();
        editor.select(anchor..head);
        editor
    }

    #[test]
    fn content_sits_inside_the_border_and_padding() {
        let content = content_box(Rect::new(10.0, 20.0, 300.0, 44.0));
        assert_eq!(content, Rect::new(23.0, 21.0, 274.0, 42.0));
        assert_eq!(single_line("a\r\nb\nc\rd"), "a b c d");
    }

    #[test]
    fn passwords_show_one_mask_per_cluster() {
        let text = InputText::new(Field::Password, "e\u{301}x", &FixedMeasure);
        assert_eq!(text.shown, "\u{2022}\u{2022}");
        assert_eq!(
            shown(Field::Password, "e\u{301}x", &FixedMeasure),
            text.shown
        );
        assert_eq!(shown(Field::Email, "e\u{301}x", &FixedMeasure), "e\u{301}x");
        assert_eq!(
            (text.len(), text.shown_offset(2), text.shown_offset(3)),
            (3, 1, 2)
        );
        // 16px BODY text: each shown character is 8px wide.
        assert_eq!((text.caret_x(2), text.width()), (8.0, 16.0));
        assert_eq!(text.offset_at(7.0), 2);
        assert_eq!(text.word_at(1.0), 0..3);
        assert_eq!(name("").word_at(5.0), 0..0);
    }

    #[test]
    fn hits_the_nearest_caret_stop_between_clusters() {
        let text = name("ae\u{301}b");
        assert!(!text.is_empty());
        assert_eq!(text.offset_at(-5.0), 0);
        assert_eq!(text.offset_at(3.0), 0);
        assert_eq!(text.offset_at(5.0), 1);
        // The accent shares its letter's stop.
        assert_eq!(text.offset_at(16.0), 1);
        assert_eq!(text.offset_at(21.0), 3);
        assert_eq!(text.offset_at(99.0), 4);
    }

    #[test]
    fn double_clicks_select_words_punctuation_or_spaces() {
        let text = name("ada@x.com  ok");
        assert_eq!(text.word_at(1.0), 0..3);
        assert_eq!(text.word_at(19.0), 0..3);
        // Past the middle of the last "a", the nearest stop is after it, so
        // Chrome selects the "@" after the stop.
        assert_eq!(text.word_at(21.0), 3..4);
        assert_eq!(text.word_at(25.0), 3..4);
        assert_eq!(text.word_at(76.0), 9..11);
        assert_eq!(text.word_at(-1.0), 0..3);
        assert_eq!(text.word_at(500.0), 11..13);
    }

    #[test]
    fn moves_by_cluster_word_and_line() {
        let text = name("Ada e\u{301}--x  ok");
        let mut caret = editor(0, 0);
        caret.go(&text, Motion::Right, false);
        assert_eq!(caret.range(), 1..1);
        caret.place(4, false);
        caret.go(&text, Motion::Right, false);
        assert_eq!(caret.head, 6);
        caret.go(&text, Motion::Left, false);
        assert_eq!(caret.head, 4);
        // Word stops: after "é--", as no letter ends "é" before them, and
        // after "x" and "ok", never at spaces.
        let mut stops = Vec::new();
        for _ in 0..6 {
            caret.go(&text, Motion::WordRight, false);
            stops.push(caret.head);
        }
        assert_eq!(stops, [8, 9, 13, 13, 13, 13]);
        stops.clear();
        for _ in 0..6 {
            caret.go(&text, Motion::WordLeft, false);
            stops.push(caret.head);
        }
        assert_eq!(stops, [11, 8, 6, 4, 0, 0]);
        caret.go(&text, Motion::End, true);
        assert_eq!(caret.range(), 0..13);
        // Left and Right collapse a selection to its edge.
        caret.go(&text, Motion::Right, false);
        assert_eq!(caret.range(), 13..13);
        caret.select(2..5);
        caret.go(&text, Motion::Left, false);
        assert_eq!(caret.range(), 2..2);
        caret.go(&text, Motion::Start, true);
        assert_eq!((caret.anchor, caret.head), (2, 0));
        caret.go(&text, Motion::Left, false);
        assert_eq!(caret.range(), 0..0);
    }

    #[test]
    fn moves_by_word_where_chrome_does() {
        // Chrome's stops moving right from the start and left from the end.
        for (value, right, left) in [
            ("e\u{301}--x", &[4, 5][..], &[4, 2, 0][..]),
            ("a, b", &[1, 2, 4], &[3, 1, 0]),
            ("a ,b", &[1, 3, 4], &[3, 2, 0]),
            ("a...b c", &[1, 4, 5, 7], &[6, 4, 1, 0]),
            ("x - y", &[1, 3, 5], &[4, 2, 0]),
            ("x--", &[1, 3], &[1, 0]),
            ("--x", &[2, 3], &[2, 0]),
            ("«a»", &[1, 2, 3], &[2, 1, 0]),
            ("$5 off", &[1, 2, 6], &[3, 1, 0]),
            ("a😀b", &[1, 2, 3], &[2, 1, 0]),
            ("😀 a", &[1, 3], &[2, 0]),
            ("a  b", &[1, 4], &[3, 0]),
            ("  a", &[3], &[2, 0]),
            ("a  ", &[1, 3], &[0]),
        ] {
            let text = name(value);
            let mut caret = editor(0, 0);
            let mut stops = Vec::new();
            while caret.head < text.len() {
                caret.go(&text, Motion::WordRight, false);
                stops.push(caret.head);
            }
            assert_eq!(stops, right, "{value:?} right");
            stops.clear();
            while caret.head > 0 {
                caret.go(&text, Motion::WordLeft, false);
                stops.push(caret.head);
            }
            assert_eq!(stops, left, "{value:?} left");
        }
    }

    #[test]
    fn edits_replace_the_selection() {
        let text = name("Ada Byron");
        let mut caret = editor(4, 9);
        assert_eq!(caret.selected(&text), "Byron");
        assert_eq!(caret.replace(&text, "Lovelace"), "Ada Lovelace");
        assert_eq!(caret.range(), 12..12);
        let mut caret = editor(3, 3);
        assert_eq!(caret.replace(&text, "é"), "Adaé Byron");
        assert_eq!(caret.head, 4);
    }

    #[test]
    fn deletes_characters_clusters_and_words() {
        let text = name("ae\u{301}x 👍\u{1F3FD}");
        let mut caret = editor(3, 3);
        // Backspace takes the accent alone; Delete takes a whole cluster.
        assert_eq!(
            caret.delete(&text, false, false).as_deref(),
            Some("aex 👍\u{1F3FD}")
        );
        assert_eq!(caret.head, 2);
        let mut caret = editor(1, 1);
        assert_eq!(
            caret.delete(&text, true, false).as_deref(),
            Some("ax 👍\u{1F3FD}")
        );
        let mut caret = editor(7, 7);
        assert_eq!(
            caret.delete(&text, false, false).as_deref(),
            Some("ae\u{301}x 👍")
        );
        let mut caret = editor(7, 7);
        assert_eq!(
            caret.delete(&text, false, true).as_deref(),
            Some("ae\u{301}x ")
        );
        let mut caret = editor(0, 0);
        assert_eq!(
            caret.delete(&text, true, true).as_deref(),
            Some(" 👍\u{1F3FD}")
        );
        assert_eq!(caret.delete(&text, false, false), None);
        let mut caret = editor(7, 7);
        assert_eq!(caret.delete(&text, true, false), None);
        let mut caret = editor(1, 4);
        assert_eq!(
            caret.delete(&text, true, true).as_deref(),
            Some("a 👍\u{1F3FD}")
        );
        // A joiner in Devanagari goes alone.
        let text = name("\u{915}\u{94D}\u{200D}");
        let mut caret = editor(3, 3);
        assert_eq!(
            caret.delete(&text, false, false).as_deref(),
            Some("\u{915}\u{94D}")
        );
    }

    #[test]
    fn scrolls_the_least_that_shows_the_caret() {
        // 20 characters, 160px of text, in a 100px content box.
        let text = name("abcdefghijklmnopqrst");
        let mut caret = editor(20, 20);
        caret.reveal(&text, 100.0);
        assert_eq!(caret.scroll, 60.0);
        caret.place(10, false);
        caret.reveal(&text, 100.0);
        assert_eq!(caret.scroll, 60.0);
        caret.place(5, false);
        caret.reveal(&text, 100.0);
        assert_eq!(caret.scroll, 40.0);
        caret.place(19, false);
        caret.reveal(&text, 100.5);
        assert_eq!(caret.scroll, 52.0);
        // Never further than the text's end, which may have moved back.
        caret.scroll = 90.0;
        caret.reveal(&text, 100.0);
        assert_eq!(caret.scroll, 60.0);
        let short = name("ab");
        caret.reveal(&short, 100.0);
        assert_eq!(caret.scroll, 0.0);
    }

    #[test]
    fn the_caret_stays_beside_left_to_right_text_where_the_direction_changes() {
        // "ab" then "אב" displayed reversed: a, b, ב, א.
        let bounds = [(0.0, 8.0), (8.0, 16.0), (24.0, 32.0), (16.0, 24.0)];
        let text = "ab\u{5D0}\u{5D1}";
        assert_eq!(caret_x(text, &bounds, 2), 16.0);
        assert_eq!(caret_x(text, &bounds, 3), 24.0);
        // "אב" then "ab": ב, א, a, b.
        let bounds = [(8.0, 16.0), (0.0, 8.0), (16.0, 24.0), (24.0, 32.0)];
        let text = "\u{5D0}\u{5D1}ab";
        assert_eq!(caret_x(text, &bounds, 2), 16.0);
        assert_eq!(caret_x(text, &bounds, 4), 32.0);
        assert_eq!(caret_x("", &[], 0), 0.0);
    }
}
