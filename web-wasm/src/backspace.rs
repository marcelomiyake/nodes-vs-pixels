//! How much Backspace deletes, as Chrome's backspace state machine decides
//! from the characters before the caret: one character, such as an accent
//! typed after its letter or a joiner in Indic text, but a whole emoji
//! sequence, flag or keycap.

use icu_properties::props::{
    CanonicalCombiningClass, Emoji, EmojiModifierBase, ExtendedPictographic, RegionalIndicator,
    VariationSelector,
};
use icu_properties::{CodePointMapData, CodePointSetData};

const JOINER: char = '\u{200D}';
const KEYCAP: char = '\u{20E3}';
const CANCEL_TAG: char = '\u{E007F}';

/// What Backspace has seen so far, reading back from the caret.
#[derive(Clone, Copy)]
enum Back {
    Selector,
    Emoji,
    Joiner,
    SelectorAndJoiner,
    Modifier,
    SelectorAndModifier,
    Keycap,
    SelectorAndKeycap,
    Flag { odd: bool },
    TagTerm,
    Tags,
}

/// How many of the characters `before` the caret, nearest first, Backspace
/// deletes.
pub fn length(mut before: impl Iterator<Item = char>) -> usize {
    let Some(last) = before.next() else {
        return 0;
    };
    let mut state = match last {
        c if is_selector(c) => Back::Selector,
        c if is_regional_indicator(c) => Back::Flag { odd: true },
        c if is_modifier(c) => Back::Modifier,
        c if is_pictographic(c) => Back::Emoji,
        KEYCAP => Back::Keycap,
        CANCEL_TAG => Back::TagTerm,
        _ => return 1,
    };
    let mut length = 1;
    // Whether the sequence is the base of a tag sequence, which ends it.
    let mut tagged = false;
    for c in before {
        state = match state {
            Back::Selector if is_pictographic(c) && tagged => return length + 1,
            Back::Selector if is_pictographic(c) => {
                length += 1;
                Back::Emoji
            }
            // A selector goes with what it follows, unless that is a mark
            // or another selector.
            Back::Selector => return length + usize::from(!is_selector(c) && !is_mark(c)),
            Back::Emoji if c == JOINER => Back::Joiner,
            Back::Joiner if is_pictographic(c) => {
                length += 2;
                if is_modifier(c) {
                    Back::Modifier
                } else {
                    Back::Emoji
                }
            }
            Back::Joiner if is_selector(c) => Back::SelectorAndJoiner,
            Back::SelectorAndJoiner if is_pictographic(c) => {
                length += 3;
                Back::Emoji
            }
            Back::Modifier if is_selector(c) => Back::SelectorAndModifier,
            Back::Modifier if is_modifier_base(c) && tagged => return length + 1,
            Back::Modifier if is_modifier_base(c) => {
                length += 1;
                Back::Emoji
            }
            Back::SelectorAndModifier if is_modifier_base(c) => return length + 2,
            Back::Keycap if is_selector(c) => Back::SelectorAndKeycap,
            Back::Keycap if is_keycap_base(c) => return length + 1,
            Back::SelectorAndKeycap if is_keycap_base(c) => return length + 2,
            Back::Flag { odd } if is_regional_indicator(c) => {
                // Flags pair up from the start of the text.
                if odd {
                    length += 1;
                } else {
                    length -= 1;
                }
                Back::Flag { odd: !odd }
            }
            Back::TagTerm | Back::Tags if is_tag(c) => {
                length += 1;
                Back::Tags
            }
            Back::Tags if is_modifier(c) => {
                length += 1;
                tagged = true;
                Back::Modifier
            }
            Back::Tags if is_selector(c) => {
                length += 1;
                tagged = true;
                Back::Selector
            }
            Back::Tags if is_pictographic(c) => return length + 1,
            _ => return length,
        };
    }
    length
}

fn is_selector(c: char) -> bool {
    CodePointSetData::new::<VariationSelector>().contains(c)
}

fn is_regional_indicator(c: char) -> bool {
    CodePointSetData::new::<RegionalIndicator>().contains(c)
}

/// Skin tones.
fn is_modifier(c: char) -> bool {
    matches!(c, '\u{1F3FB}'..='\u{1F3FF}')
}

/// What a skin tone can follow.
fn is_modifier_base(c: char) -> bool {
    CodePointSetData::new::<EmojiModifierBase>().contains(c)
}

/// What Chrome takes for an emoji, including those yet to be assigned.
fn is_pictographic(c: char) -> bool {
    CodePointSetData::new::<ExtendedPictographic>().contains(c)
        || CodePointSetData::new::<Emoji>().contains(c)
}

fn is_keycap_base(c: char) -> bool {
    matches!(c, '0'..='9' | '#' | '*')
}

/// The tag digits and small letters that name a subdivision's flag.
fn is_tag(c: char) -> bool {
    matches!(c, '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}')
}

/// A combining mark, such as an accent, which reorders with others.
fn is_mark(c: char) -> bool {
    CodePointMapData::<CanonicalCombiningClass>::new().get(c)
        != CanonicalCombiningClass::NotReordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_as_much_as_chrome_does() {
        // How many characters Chrome's Backspace deletes at the end of each.
        for (text, deleted) in [
            ("", 0),
            ("a1", 1),
            ("e\u{301}", 1),
            ("\u{262B}\u{301}", 1),
            ("\u{A9}\u{FE0F}", 2),
            ("e\u{FE0F}", 2),
            ("a\u{301}\u{FE0F}", 1),
            ("a\u{FE0E}\u{FE0F}", 1),
            ("\u{915}\u{94D}\u{FE0F}", 1),
            ("x\u{2639}\u{FE0F}", 2),
            ("1\u{FE0F}\u{20E3}", 3),
            ("1\u{20E3}", 2),
            ("x\u{20E3}", 1),
            ("a🇺🇸", 2),
            ("a🇺🇸\u{1F1EB}", 1),
            ("🇺🇸🇫🇷", 2),
            ("👍\u{1F3FD}", 2),
            ("a\u{1F3FD}", 1),
            ("\u{261D}\u{FE0F}\u{1F3FD}", 3),
            ("x👩\u{1F3FD}\u{200D}💻", 4),
            ("\u{1F3FD}\u{200D}💻", 3),
            ("x👨\u{200D}👩\u{200D}👧", 5),
            ("x\u{2764}\u{FE0F}\u{200D}🔥", 4),
            ("x\u{A9}\u{200D}👍", 3),
            ("x\u{2603}\u{200D}\u{2603}", 3),
            ("ab\u{200D}👍", 1),
            ("x👍\u{200D}", 1),
            ("\u{915}\u{94D}\u{200D}", 1),
            ("x\u{1F000}", 1),
            (
                "x🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
                7,
            ),
            ("x👍\u{1F3FD}\u{E0067}\u{E007F}", 4),
            ("x\u{2764}\u{FE0F}\u{E0067}\u{E007F}", 4),
            ("xa\u{E0067}\u{E007F}", 2),
            ("x\u{E007F}", 1),
        ] {
            assert_eq!(length(text.chars().rev()), deleted, "{text:?}");
        }
    }
}
