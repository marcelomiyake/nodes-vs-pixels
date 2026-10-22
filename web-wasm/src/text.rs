//! Text measurement and line breaking that reproduce how Chrome lays out the
//! same text in the DOM: integer font metrics, half-leading rounded down, and
//! greedy breaking at the opportunities Chrome finds.
//!
//! The only text wrapped is the copy from `spec/copy.json` and a full name
//! that passed validation, which allows letters of any script, spaces,
//! apostrophes (`'`, `’`) and hyphens. The break rules cover that domain;
//! characters a name cannot hold, such as emoji or soft hyphens, are outside
//! it.

use std::collections::HashSet;

use unicode_linebreak::{BreakClass, break_property, linebreaks};

use crate::style::TextStyle;

/// Font ascent and descent in CSS pixels, as the browser rounds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
}

/// Measures text the way the browser will paint it.
pub trait Measure {
    fn width(&self, text: &str, style: TextStyle) -> f64;
    fn metrics(&self, style: TextStyle) -> FontMetrics;

    /// Splits a run of a script written without spaces (Thai, Lao, Khmer,
    /// Myanmar) into words; the browser breaks lines between them using its
    /// dictionaries. The pieces must concatenate back to `run`.
    fn dictionary_words(&self, run: &str) -> Vec<String> {
        vec![run.to_owned()]
    }
}

/// The alphabetic baseline of a line box whose top edge is `line_top`.
#[must_use]
pub fn baseline(line_top: f64, style: TextStyle, metrics: FontMetrics) -> f64 {
    let half_leading = ((style.line_height - metrics.ascent - metrics.descent) / 2.0).floor();
    line_top + half_leading + metrics.ascent
}

/// Splits `text` into lines no wider than `max_width` at break opportunities.
/// Runs of whitespace collapse as in CSS, and a word wider than the line
/// overflows rather than breaking.
#[must_use]
pub fn wrap(text: &str, style: TextStyle, max_width: f64, measure: &impl Measure) -> Vec<String> {
    let collapsed = collapse_whitespace(text);
    let mut lines = Vec::new();
    let mut line = String::new();
    for segment in segments(&collapsed, measure) {
        let candidate = format!("{line}{segment}");
        if !line.is_empty() && measure.width(trim_spaces_end(&candidate), style) > max_width {
            lines.push(trim_spaces_end(&line).to_owned());
            segment.trim_start_matches(' ').clone_into(&mut line);
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(trim_spaces_end(&line).to_owned());
    }
    lines
}

/// The width of the widest unbreakable segment: CSS `min-content`.
#[must_use]
pub fn min_content_width(text: &str, style: TextStyle, measure: &impl Measure) -> f64 {
    segments(&collapse_whitespace(text), measure)
        .into_iter()
        .map(|segment| measure.width(trim_spaces_end(segment), style))
        .fold(0.0, f64::max)
}

/// Drops the collapsible spaces that hang at the end of a line. Other
/// whitespace, such as a no-break space, is laid out like any character.
fn trim_spaces_end(line: &str) -> &str {
    line.trim_end_matches(' ')
}

/// Collapses spaces, tabs and newlines like CSS `white-space: normal`.
#[must_use]
pub fn collapse_whitespace(text: &str) -> String {
    text.split([' ', '\t', '\n', '\r'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits at each break opportunity, keeping separators attached to the
/// preceding segment.
fn segments<'a>(text: &'a str, measure: &impl Measure) -> Vec<&'a str> {
    let mut ends = break_opportunities(text);
    ends.extend(dictionary_breaks(text, measure));
    ends.sort_unstable();
    ends.dedup();
    let mut start = 0;
    ends.into_iter()
        .filter(|&end| end > 0)
        .map(|end| {
            let segment = &text[start..end];
            start = end;
            segment
        })
        .collect()
}

/// Break opportunities as Chrome finds them in an English page, as recorded
/// in `spec/line-breaks.json`. Like Blink's `LazyLineBreakIterator`, a pair
/// of printable ASCII characters follows Blink's own table, and any other pair
/// follows ICU's UAX #14 rules.
fn break_opportunities(text: &str) -> Vec<usize> {
    let uax14 = uax14_breaks(text);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut breaks: Vec<usize> = Vec::new();
    for (i, &(index, after)) in chars.iter().enumerate().skip(1) {
        let before = chars[i - 1].1;
        let before_before = i.checked_sub(2).map(|j| chars[j].1);
        // ICU's rules see the character that combining marks attach to.
        let (base_index, base) = base_at(&chars, i - 1);
        let allowed = if before == '-' && after.is_ascii_digit() {
            // Blink keeps a minus sign with its number, but breaks
            // "ABCD-1234" and "1234-5678".
            before_before.is_some_and(|c| c.is_ascii_alphanumeric())
        } else if is_printable_ascii(before) && is_printable_ascii(after) {
            ascii_breaks(before, after)
        } else if matches!(base, '-' | '\u{2010}') && is_letter(after) {
            let hyphen_index = chars[base_index].0;
            let word_initial = hyphen_index == 0 || breaks.last() == Some(&hyphen_index);
            let before_hyphen = base_index.checked_sub(1).map(|j| base_at(&chars, j).1);
            breaks_before_letter(word_initial, before_hyphen)
                || class(after) == BreakClass::HebrewLetter && !word_initial
        } else {
            (uax14.contains(&index) && !joins_aksaras(&chars, i))
                || east_asian_quote_break(&chars, i)
        };
        if allowed {
            breaks.push(index);
        }
    }
    breaks.push(text.len());
    breaks
}

/// UAX #14 as `unicode-linebreak` implements it for Unicode 15.0, with
/// conditional Japanese starters (small kana, the prolonged sound mark)
/// resolved as ideographs, as Chrome does for `line-break: auto`, and with
/// the classes of Unicode 17.
fn uax14_breaks(text: &str) -> HashSet<usize> {
    // Characters are swapped for stand-ins of their class with the same
    // UTF-8 length, so the offsets stay valid for `text`.
    let resolved: String = text
        .chars()
        .map(|c| {
            let class = match class(c) {
                BreakClass::ConditionalJapaneseStarter => BreakClass::Ideographic,
                class => class,
            };
            if class == break_property(u32::from(c)) {
                c
            } else {
                stand_in(class, c.len_utf8())
            }
        })
        .collect();
    linebreaks(&resolved).map(|(index, _)| index).collect()
}

/// A character that `unicode-linebreak` puts in `class`, `len` bytes long in
/// UTF-8.
fn stand_in(class: BreakClass, len: usize) -> char {
    match (class, len) {
        (BreakClass::Ideographic, 3) => '\u{4E00}',
        (BreakClass::CombiningMark, 3) => '\u{20D0}',
        (BreakClass::After, 3) => '\u{1361}',
        (BreakClass::CombiningMark, _) => '\u{101FD}',
        (BreakClass::After, _) => '\u{10100}',
        (BreakClass::OpenPunctuation, _) => '\u{13258}',
        (BreakClass::NonStarter, _) => '\u{16FE0}',
        _ => '\u{20000}',
    }
}

/// UAX #14 rule 28a: no breaks inside the orthographic syllables of Brahmic
/// scripts, which join letters through viramas and prebase signs. `i` is the
/// index of the character after the break.
fn joins_aksaras(chars: &[(usize, char)], i: usize) -> bool {
    use Aksara::{FinalVirama, Letter, Prebase, Start, Virama};
    let base = |j: usize| {
        let (k, c) = base_at(chars, j);
        Some((k, aksara(c)?))
    };
    let Some((k, before)) = base(i - 1) else {
        return false;
    };
    match (before, aksara(chars[i].1)) {
        (Prebase, Some(Letter | Start)) => true,
        (Virama, Some(Letter)) => k > 0 && matches!(base(k - 1), Some((_, Letter | Start))),
        (Letter | Start, Some(Letter | Start)) => chars[i + 1..]
            .iter()
            .find(|&&(_, c)| !is_mark(c))
            .is_some_and(|&(_, c)| aksara(c) == Some(FinalVirama)),
        _ => false,
    }
}

/// The character at `index`, or the base that the combining marks ending
/// there attach to (UAX #14 rule 9), with its index. Marks at the start or
/// after a space have no base and stand for themselves (rule 10).
fn base_at(chars: &[(usize, char)], index: usize) -> (usize, char) {
    let mut k = index;
    while k > 0 && is_mark(chars[k].1) && !is_break_space(chars[k - 1].1) {
        k -= 1;
    }
    (k, chars[k].1)
}

/// Combining marks (and joiners), which attach to the character before
/// them. Since Unicode 15.1, viramas are not combining marks.
fn is_mark(c: char) -> bool {
    aksara(c).is_none()
        && matches!(
            class(c),
            BreakClass::CombiningMark | BreakClass::ZeroWidthJoiner
        )
}

/// Spaces and line breaks, which marks cannot attach to.
fn is_break_space(c: char) -> bool {
    matches!(
        class(c),
        BreakClass::Space
            | BreakClass::ZeroWidthSpace
            | BreakClass::Mandatory
            | BreakClass::CarriageReturn
            | BreakClass::LineFeed
            | BreakClass::NextLine
    )
}

/// Initial and final quotation marks (UAX #14 class QU, categories Pi, Pf).
const INITIAL_QUOTES: &str = "\u{AB}\u{2018}\u{201B}\u{201C}\u{201F}\u{2039}\u{2E02}\u{2E04}\u{2E09}\u{2E0C}\u{2E1C}\u{2E20}";
const FINAL_QUOTES: &str =
    "\u{BB}\u{2019}\u{201D}\u{203A}\u{2E03}\u{2E05}\u{2E0A}\u{2E0D}\u{2E1D}\u{2E21}";

/// UAX #14 rule 19a (Unicode 16): between East Asian characters, a final
/// quotation mark allows a break after it, and an initial one before it.
/// `i` is the index of the character after the break.
fn east_asian_quote_break(chars: &[(usize, char)], i: usize) -> bool {
    let (quote_index, before) = base_at(chars, i - 1);
    let after = chars[i].1;
    if FINAL_QUOTES.contains(before) {
        let before_quote = quote_index.checked_sub(1).map(|j| base_at(chars, j).1);
        before_quote.is_some_and(is_east_asian) && starts_east_asian_line(after)
    } else if INITIAL_QUOTES.contains(after) {
        // The quote's own combining marks come after it.
        let after_quote = chars[i + 1..]
            .iter()
            .map(|&(_, c)| c)
            .find(|&c| !is_mark(c));
        is_east_asian(before)
            && !matches!(
                class(before),
                BreakClass::OpenPunctuation | BreakClass::Quotation
            )
            && after_quote.is_some_and(is_east_asian)
    } else {
        false
    }
}

/// Whether a hyphen (`-` or U+2010) breaks before a letter: not when it starts a word
/// (UAX #14 rule 20a), and not after a Hebrew letter unless another follows
/// (rule 21a, Unicode 15.1).
fn breaks_before_letter(word_initial: bool, before_hyphen: Option<char>) -> bool {
    !word_initial && !before_hyphen.is_some_and(|c| class(c) == BreakClass::HebrewLetter)
}

/// Letters as UAX #14 resolves them (AL and HL, with SA and AI as AL).
fn is_letter(c: char) -> bool {
    matches!(
        class(c),
        BreakClass::Alphabetic
            | BreakClass::HebrewLetter
            | BreakClass::ComplexContext
            | BreakClass::Ambiguous
    )
}

/// East Asian characters before which a line may start.
fn starts_east_asian_line(c: char) -> bool {
    is_east_asian(c)
        && matches!(
            class(c),
            BreakClass::Ideographic
                | BreakClass::ConditionalJapaneseStarter
                | BreakClass::HangulLvSyllable
                | BreakClass::HangulLvtSyllable
                | BreakClass::HangulLJamo
        )
}

const fn is_printable_ascii(c: char) -> bool {
    matches!(c, '!'..='~')
}

/// Blink's table of breaks between printable ASCII characters
/// (`kAsciiLineBreakTable` in `text_break_iterator.cc`).
fn ascii_breaks(before: char, after: char) -> bool {
    match before {
        '-' => !"!$),./:;?]}".contains(after),
        '?' => !"!\"'),./:;?]}".contains(after),
        '!' | '"' | '#' | '%' | '&' | ')' | '*' | '+' | ',' | '.' | ':' | ';' | '=' | '>'
        | '\\' | ']' | '|' | '}' | '~' => "(<[{".contains(after),
        _ => false,
    }
}

/// Characters of East Asian width F, W or H, the context in which a quotation
/// mark allows breaks.
fn is_east_asian(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{115F}'     // Hangul Jamo initials
        | '\u{2E80}'..='\u{303E}'   // CJK radicals, symbols and punctuation
        | '\u{3041}'..='\u{33FF}'   // Kana, Bopomofo, Hangul compatibility jamo
        | '\u{3400}'..='\u{4DBF}'   // CJK Extension A
        | '\u{4E00}'..='\u{A4CF}'   // CJK Unified Ideographs, Yi
        | '\u{A960}'..='\u{A97F}'   // Hangul Jamo Extended-A
        | '\u{AC00}'..='\u{D7A3}'   // Hangul Syllables
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility Ideographs
        | '\u{FE30}'..='\u{FE4F}'   // CJK Compatibility Forms
        | '\u{FF00}'..='\u{FFEF}'   // Halfwidth and Fullwidth Forms
        | '\u{16FE0}'..='\u{18DFF}' // Ideographic symbols, Tangut, Khitan
        | '\u{1AFF0}'..='\u{1B2FF}' // Kana extensions, Nushu
        | '\u{20000}'..='\u{3FFFD}' // CJK Extensions B and later
    )
}

/// The breaks between dictionary words inside runs of scripts written
/// without spaces (Thai, Lao, Khmer, Myanmar and others).
fn dictionary_breaks(text: &str, measure: &impl Measure) -> Vec<usize> {
    let mut breaks = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((run_start, c)) = chars.next() {
        if !is_complex(c) {
            continue;
        }
        let mut run_end = run_start + c.len_utf8();
        while let Some((index, next)) = chars.next_if(|&(_, next)| is_complex(next)) {
            run_end = index + next.len_utf8();
        }
        let words = measure.dictionary_words(&text[run_start..run_end]);
        let mut offset = run_start;
        for word in words.iter().take(words.len().saturating_sub(1)) {
            offset += word.len();
            breaks.push(offset);
        }
    }
    breaks
}

/// The class of `c` in Unicode 17, for the rules of `unicode-linebreak`.
/// Aksaras break like ideographs, and viramas stay with the aksara before
/// them like combining marks; [`joins_aksaras`] applies the rest of rule 28a.
fn class(c: char) -> BreakClass {
    match change(c) {
        Some(Change::Class(class)) => class,
        Some(Change::Aksara(Aksara::Virama | Aksara::FinalVirama)) => BreakClass::CombiningMark,
        Some(Change::Aksara(_)) => BreakClass::Ideographic,
        None if is_complex_mark(c) => BreakClass::CombiningMark,
        None => break_property(u32::from(c)),
    }
}

/// Characters of the scripts written without spaces (class SA), marks
/// included.
fn is_complex(c: char) -> bool {
    break_property(u32::from(c)) == BreakClass::ComplexContext
}

/// The combining marks (Mn, Mc) of class SA, which rule 1 resolves to CM.
fn is_complex_mark(c: char) -> bool {
    let c = u32::from(c);
    let next = COMPLEX_MARKS.partition_point(|&(first, _)| first <= c);
    next.checked_sub(1).is_some_and(|i| c <= COMPLEX_MARKS[i].1)
}

/// From `LineBreak.txt` and `UnicodeData.txt` of Unicode 17, sorted.
const COMPLEX_MARKS: [(u32, u32); 27] = [
    (0x0E31, 0x0E31),
    (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E),
    (0x0EB1, 0x0EB1),
    (0x0EB4, 0x0EBC),
    (0x0EC8, 0x0ECE),
    (0x102B, 0x103E),
    (0x1056, 0x1059),
    (0x105E, 0x1060),
    (0x1062, 0x1064),
    (0x1067, 0x106D),
    (0x1071, 0x1074),
    (0x1082, 0x108D),
    (0x108F, 0x108F),
    (0x109A, 0x109D),
    (0x17B4, 0x17D3),
    (0x17DD, 0x17DD),
    (0x1A55, 0x1A5E),
    (0x1A60, 0x1A7C),
    (0xA9E5, 0xA9E5),
    (0xAA7B, 0xAA7D),
    (0xAAB0, 0xAAB0),
    (0xAAB2, 0xAAB4),
    (0xAAB7, 0xAAB8),
    (0xAABE, 0xAABF),
    (0xAAC1, 0xAAC1),
    (0x1171D, 0x1172B),
];

fn aksara(c: char) -> Option<Aksara> {
    match change(c) {
        Some(Change::Aksara(aksara)) => Some(aksara),
        _ => None,
    }
}

fn change(c: char) -> Option<Change> {
    let c = u32::from(c);
    let next = UNICODE_17.partition_point(|&(first, _, _)| first <= c);
    let (_, last, change) = *UNICODE_17.get(next.checked_sub(1)?)?;
    (c <= last).then_some(change)
}

/// The classes Unicode 15.1 added for Brahmic scripts (rule 28a).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Aksara {
    /// AK: a consonant or independent vowel.
    Letter,
    /// AS: a letter that may also start a syllable on its own.
    Start,
    /// AP: a sign written before the letter it belongs to.
    Prebase,
    /// VI: a virama that joins the next letter to the syllable.
    Virama,
    /// VF: a virama that ends the syllable.
    FinalVirama,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    Class(BreakClass),
    Aksara(Aksara),
}

const AK: Change = Change::Aksara(Aksara::Letter);
const AS: Change = Change::Aksara(Aksara::Start);
const AP: Change = Change::Aksara(Aksara::Prebase);
const VI: Change = Change::Aksara(Aksara::Virama);
const VF: Change = Change::Aksara(Aksara::FinalVirama);
const BA: Change = Change::Class(BreakClass::After);
const CM: Change = Change::Class(BreakClass::CombiningMark);
const ID: Change = Change::Class(BreakClass::Ideographic);
const NS: Change = Change::Class(BreakClass::NonStarter);
const OP: Change = Change::Class(BreakClass::OpenPunctuation);

/// The line break classes of Unicode 17, which Chrome's ICU uses, where they
/// differ from Unicode 15.0 (`unicode-linebreak`) for a character a name can
/// hold, and the viramas of rule 28a. From `LineBreak.txt`, sorted.
const UNICODE_17: [(u32, u32, Change); 72] = [
    (0x0897, 0x0897, CM),
    (0x1B05, 0x1B33, AK),
    (0x1B44, 0x1B44, VI),
    (0x1B45, 0x1B4C, AK),
    (0x1BC0, 0x1BE5, AS),
    (0x1BF2, 0x1BF3, VF),
    (0xA984, 0xA9B2, AK),
    (0xA9C0, 0xA9C0, VI),
    (0xA9CF, 0xA9CF, BA),
    (0xAA00, 0xAA28, AS),
    (0xAA40, 0xAA42, BA),
    (0xAA44, 0xAA4B, BA),
    (0x10D69, 0x10D69, CM),
    (0x10EFA, 0x10EFC, CM),
    (0x11003, 0x11004, AP),
    (0x11005, 0x11037, AK),
    (0x11046, 0x11046, VI),
    (0x11071, 0x11072, AK),
    (0x11075, 0x11075, AK),
    (0x11305, 0x1130C, AK),
    (0x1130F, 0x11310, AK),
    (0x11313, 0x11328, AK),
    (0x1132A, 0x11330, AK),
    (0x11332, 0x11333, AK),
    (0x11335, 0x11339, AK),
    (0x1133D, 0x1133D, BA),
    (0x1134D, 0x1134D, VI),
    (0x11350, 0x11350, AS),
    (0x1135D, 0x1135D, BA),
    (0x1135E, 0x1135F, AS),
    (0x11360, 0x11361, AK),
    (0x11380, 0x11389, AS),
    (0x1138B, 0x1138B, AS),
    (0x1138E, 0x1138E, AS),
    (0x11390, 0x11391, AS),
    (0x11392, 0x113B5, AK),
    (0x113B7, 0x113B7, ID),
    (0x113B8, 0x113C0, CM),
    (0x113C2, 0x113C2, CM),
    (0x113C5, 0x113C5, CM),
    (0x113C7, 0x113CA, CM),
    (0x113CC, 0x113CD, CM),
    (0x113D0, 0x113D0, VI),
    (0x113D1, 0x113D1, AP),
    (0x113D3, 0x113D3, ID),
    (0x11900, 0x11906, AK),
    (0x11909, 0x11909, AK),
    (0x1190C, 0x11913, AK),
    (0x11915, 0x11916, AK),
    (0x11918, 0x1192F, AK),
    (0x1193E, 0x1193E, VI),
    (0x1193F, 0x1193F, AP),
    (0x11941, 0x11941, AP),
    (0x11B60, 0x11B67, CM),
    (0x11EE0, 0x11EF1, AS),
    (0x11EF2, 0x11EF2, BA),
    (0x11F02, 0x11F02, AP),
    (0x11F04, 0x11F10, AK),
    (0x11F12, 0x11F33, AK),
    (0x11F42, 0x11F42, VI),
    (0x1342F, 0x1342F, OP),
    (0x16100, 0x1611D, AS),
    (0x1611E, 0x1612E, CM),
    (0x16FF2, 0x16FF3, NS),
    (0x16FF4, 0x16FF6, ID),
    (0x187F8, 0x187FF, ID),
    (0x18D09, 0x18D1E, ID),
    (0x18D80, 0x18DF2, ID),
    (0x1E6E3, 0x1E6E3, CM),
    (0x1E6E6, 0x1E6E6, CM),
    (0x1E6EE, 0x1E6EF, CM),
    (0x1E6F5, 0x1E6F5, CM),
];

#[cfg(test)]
pub mod testing {
    use super::{FontMetrics, Measure};
    use crate::style::TextStyle;

    /// Every character is half the font size wide, and spaceless scripts
    /// split into three-character "words".
    pub struct FixedMeasure;

    impl Measure for FixedMeasure {
        fn width(&self, text: &str, style: TextStyle) -> f64 {
            #[allow(clippy::cast_precision_loss)]
            let count = text.chars().count() as f64;
            count * style.size / 2.0
        }

        fn metrics(&self, style: TextStyle) -> FontMetrics {
            FontMetrics {
                ascent: style.size,
                descent: (style.size / 4.0).round(),
            }
        }

        fn dictionary_words(&self, run: &str) -> Vec<String> {
            let chars: Vec<char> = run.chars().collect();
            chars.chunks(3).map(|word| word.iter().collect()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FixedMeasure;
    use super::*;
    use crate::tokens::text;

    #[test]
    fn baseline_rounds_half_leading_down() {
        // Chrome: 24px Inter has ascent 23 and descent 6 in a 32px line box.
        let metrics = FontMetrics {
            ascent: 23.0,
            descent: 6.0,
        };
        assert_eq!(baseline(100.0, text::TITLE, metrics), 124.0);
        let metrics = FontMetrics {
            ascent: 16.0,
            descent: 4.0,
        };
        assert_eq!(baseline(10.0, text::BODY, metrics), 28.0);
    }

    #[test]
    fn wraps_greedily_at_spaces() {
        // SMALL is 14px, so each character is 7px wide.
        let lines = wrap("aaa bbb ccc", text::SMALL, 7.0 * 7.0, &FixedMeasure);
        assert_eq!(lines, ["aaa bbb", "ccc"]);
    }

    #[test]
    fn breaks_after_hyphens_and_keeps_long_words_whole() {
        let lines = wrap("Lovelace-Byron x", text::SMALL, 7.0 * 9.0, &FixedMeasure);
        assert_eq!(lines, ["Lovelace-", "Byron x"]);
        let lines = wrap("abcdefghij k", text::SMALL, 7.0 * 4.0, &FixedMeasure);
        assert_eq!(lines, ["abcdefghij", "k"]);
    }

    #[test]
    fn breaks_where_chrome_does() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../spec/line-breaks.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let collapsed = collapse_whitespace(text);
            let lines: Vec<&str> = segments(&collapsed, &FixedMeasure)
                .into_iter()
                .map(trim_spaces_end)
                .collect();
            assert_eq!(serde_json::json!(lines), case["lines"], "{text}");
        }
    }

    #[test]
    fn resolved_classes_have_stand_ins() {
        let ranges: Vec<(u32, u32)> = UNICODE_17
            .iter()
            .map(|&(first, last, _)| (first, last))
            .collect();
        for table in [&ranges[..], &COMPLEX_MARKS[..]] {
            for pair in table.windows(2) {
                assert!(pair[0].1 < pair[1].0, "{pair:?}");
            }
        }
        for &(first, last) in ranges.iter().chain(&COMPLEX_MARKS) {
            for c in (first..=last).filter_map(char::from_u32) {
                let class = class(c);
                let stand_in = stand_in(class, c.len_utf8());
                assert_eq!(break_property(u32::from(stand_in)), class, "{c:?}");
                assert_eq!(stand_in.len_utf8(), c.len_utf8(), "{c:?}");
            }
        }
    }

    #[test]
    fn packs_ideographs_up_to_the_line_width() {
        // 14px SMALL text: each character is 7px wide, so 4 characters fit.
        let lines = wrap("Hi, 李小龍李小龍.", text::SMALL, 7.0 * 4.0, &FixedMeasure);
        assert_eq!(lines, ["Hi,", "李小龍李", "小龍."]);
    }

    #[test]
    fn breaks_spaceless_scripts_between_dictionary_words() {
        // FixedMeasure splits Thai into three-character words.
        let name = "สมชายใจดี";
        assert_eq!(
            wrap(name, text::SMALL, 7.0 * 4.0, &FixedMeasure),
            ["สมช", "ายใ", "จดี"]
        );
        assert_eq!(
            wrap(&format!("a{name}b"), text::SMALL, 7.0 * 7.0, &FixedMeasure),
            ["aสมชายใ", "จดีb"]
        );
        assert_eq!(min_content_width(name, text::SMALL, &FixedMeasure), 21.0);
    }

    #[test]
    fn min_content_is_the_widest_unbreakable_segment() {
        assert_eq!(
            min_content_width("ab  abcd-ef", text::SMALL, &FixedMeasure),
            35.0
        );
        assert_eq!(min_content_width("", text::SMALL, &FixedMeasure), 0.0);
    }

    #[test]
    fn handles_empty_text_and_repeated_spaces() {
        assert_eq!(wrap("", text::SMALL, 10.0, &FixedMeasure), [""]);
        assert_eq!(
            wrap(" -a \t b\n", text::SMALL, 1000.0, &FixedMeasure),
            ["-a b"]
        );
        // A no-break space is not collapsible, so it keeps its width.
        assert_eq!(
            wrap("a\u{a0}", text::SMALL, 7.0, &FixedMeasure),
            ["a\u{a0}"]
        );
        assert_eq!(
            min_content_width("a\u{a0}", text::SMALL, &FixedMeasure),
            14.0
        );
    }
}
