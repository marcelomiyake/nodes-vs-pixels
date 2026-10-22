//! Paints a [`Layout`] the way the React app's Tailwind classes style the same
//! elements, through a [`Painter`] that the browser glue backs with a canvas.

use form_core::Field;

use crate::editing::{self, InputText, TOUCHING, content_box};
use crate::form::FormState;
use crate::layout::{Align, BORDER, Content, FormLayout, Layout, Rect, SuccessLayout, TextBlock};
use crate::style::{Shadow, TextStyle};
use crate::text::{Measure, baseline};
use crate::tokens::{color, radius, shadow, spacing, text};

/// The checkbox tick, in the 20×20 box (`TermsCheckbox.tsx`).
pub const CHECK_PATH: &str = "M5.5 10.5l3 3 6-7";
const CHECK_STROKE: f64 = 2.0;
/// The success tick, in a 24×24 box centered in the icon (`SuccessPanel.tsx`).
pub const SUCCESS_PATH: &str = "M5 12.5l4.5 4.5L19 7.5";
const SUCCESS_SIZE: f64 = 24.0;
const SUCCESS_STROKE: f64 = 2.5;
/// Chrome's caret width.
const CARET_WIDTH: f64 = 1.0;

/// Drawing operations, in CSS pixels.
pub trait Painter {
    /// Fills a rectangle with rounded corners, optionally casting `shadow`.
    fn fill_rect(&mut self, rect: Rect, radius: f64, color: &str, shadow: Option<Shadow>);
    /// Strokes an SVG path translated to `origin`, with round caps and joins.
    fn stroke_path(&mut self, path: &str, origin: (f64, f64), width: f64, color: &str);
    /// Draws one line of text with its alphabetic baseline at `baseline`.
    fn fill_text(&mut self, text: &str, x: f64, baseline: f64, style: TextStyle, color: &str);
    /// Clips later drawing to `rect` until the matching [`Painter::restore`].
    fn clip(&mut self, rect: Rect);
    fn restore(&mut self);
}

/// A focusable control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Field(Field),
    Button,
}

/// Interaction state that only affects painting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    /// The control with focus. The terms checkbox is `Control::Field`.
    pub focus: Option<Control>,
    /// Whether the browser would match `:focus-visible` on the focused control.
    pub focus_visible: bool,
    /// Whether the pointer is over the submit button.
    pub hovered: bool,
    /// Whether the blinking caret is in its visible phase.
    pub caret_visible: bool,
    /// The focused input's horizontal text scroll (`scrollLeft`).
    pub scroll_left: f64,
    /// The focused input's selection, in characters.
    pub selection: (usize, usize),
}

/// Converts a DOM selection offset (UTF-16 code units) to characters.
#[must_use]
pub fn char_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= utf16
        })
        .count()
}

/// Paints the whole document.
pub fn paint(
    layout: &Layout,
    state: &FormState,
    ui: &Interaction,
    measure: &impl Measure,
    painter: &mut impl Painter,
) {
    let page = Rect::new(0.0, 0.0, layout.width, layout.height);
    painter.fill_rect(page, 0.0, color::PAGE, None);
    bordered(
        painter,
        layout.card,
        radius::CARD,
        color::LINE,
        color::SURFACE,
        Some(shadow::CARD),
    );
    match &layout.content {
        Content::Form(form) => paint_form(form, state, ui, measure, painter),
        Content::Success(success) => paint_success(success, measure, painter),
    }
}

fn paint_form(
    form: &FormLayout,
    state: &FormState,
    ui: &Interaction,
    measure: &impl Measure,
    painter: &mut impl Painter,
) {
    painter.fill_rect(form.logo, radius::LOGO, color::BRAND, None);
    paint_text(&form.logo_text, color::ON_BRAND, measure, painter);
    paint_text(&form.title, color::INK, measure, painter);
    paint_text(&form.subtitle, color::MUTED, measure, painter);

    for field in &form.fields {
        paint_text(&field.label, color::INK_SOFT, measure, painter);
        let focused = ui.focus == Some(Control::Field(field.field));
        let invalid = state.error(field.field).is_some();
        if focused {
            ring(painter, field.input, radius::CONTROL, invalid);
        }
        let border = if invalid {
            color::DANGER
        } else if focused {
            color::BRAND
        } else {
            color::FIELD
        };
        bordered(
            painter,
            field.input,
            radius::CONTROL,
            border,
            color::SURFACE,
            None,
        );
        let editor = focused.then_some(ui);
        paint_input(field.field, field.input, state, editor, measure, painter);
        if let Some(message) = &field.message {
            let tone = if invalid {
                color::DANGER
            } else {
                color::SUBTLE
            };
            paint_text(message, tone, measure, painter);
        }
    }

    let terms = &form.terms;
    let invalid = state.error(Field::AcceptTerms).is_some();
    if ui.focus == Some(Control::Field(Field::AcceptTerms)) && ui.focus_visible {
        ring(painter, terms.check, radius::CHECKBOX, invalid);
    }
    if state.values().accept_terms {
        painter.fill_rect(terms.check, radius::CHECKBOX, color::BRAND, None);
        let origin = (terms.check.x, terms.check.y);
        painter.stroke_path(CHECK_PATH, origin, CHECK_STROKE, color::ON_BRAND);
    } else {
        let border = if invalid { color::DANGER } else { color::FIELD };
        bordered(
            painter,
            terms.check,
            radius::CHECKBOX,
            border,
            color::SURFACE,
            None,
        );
    }
    paint_text(&terms.label, color::INK_SOFT, measure, painter);
    if let Some(message) = &terms.message {
        paint_text(message, color::DANGER, measure, painter);
    }

    if ui.focus == Some(Control::Button) && ui.focus_visible {
        ring(painter, form.button, radius::CONTROL, false);
    }
    let fill = if ui.hovered {
        color::BRAND_STRONG
    } else {
        color::BRAND
    };
    painter.fill_rect(form.button, radius::CONTROL, fill, None);
    paint_text(&form.button_text, color::ON_BRAND, measure, painter);
    if let Some(error) = &form.error {
        paint_text(error, color::DANGER, measure, painter);
    }
}

/// The value or placeholder of a text input, with the selection and caret
/// when it has focus. Text is clipped to the content box and scrolls with it.
fn paint_input(
    field: Field,
    input: Rect,
    state: &FormState,
    editor: Option<&Interaction>,
    measure: &impl Measure,
    painter: &mut impl Painter,
) {
    let style = text::BODY;
    let content = content_box(input);
    let line_top = input.y + (input.height - style.line_height) / 2.0;
    let metrics = measure.metrics(style);
    let text_baseline = baseline(line_top, style, metrics);
    // Chrome sizes the caret and the selection to the font, not the line.
    let glyphs = Rect::new(
        0.0,
        text_baseline - metrics.ascent,
        0.0,
        metrics.ascent + metrics.descent,
    );
    let value = state.text(field);
    // Only the focused input needs to know where its characters are.
    let text = editor.map(|_| InputText::new(field, value, measure));
    let shown = text.as_ref().map_or_else(
        || editing::shown(field, value, measure),
        |text| text.shown.clone(),
    );
    let scroll = editor.map_or(0.0, |ui| ui.scroll_left);
    let origin = content.x - scroll;

    painter.clip(content);
    if let Some((ui, text)) = editor.zip(text.as_ref()) {
        let selected = text.shown_offset(ui.selection.0)..text.shown_offset(ui.selection.1);
        for (left, right) in selection_spans(text.bounds.get(selected).unwrap_or_default()) {
            let highlight = Rect {
                x: origin + left,
                width: right - left,
                ..glyphs
            };
            painter.fill_rect(highlight, 0.0, color::BRAND_RING, None);
        }
    }
    if shown.is_empty() {
        let placeholder = crate::copy::field(field.key()).map_or("", |copy| copy.placeholder);
        painter.fill_text(placeholder, content.x, text_baseline, style, color::SUBTLE);
    } else {
        painter.fill_text(&shown, origin, text_baseline, style, color::INK);
    }
    let caret = editor.filter(|ui| ui.caret_visible && ui.selection.0 == ui.selection.1);
    if let Some((ui, text)) = caret.zip(text.as_ref()) {
        // Chrome scrolls by whole pixels, which can leave a caret at the
        // text's fractional end just past the content box; it keeps that
        // one inside, and hides one scrolled further out of view.
        let content_right = content.x + content.width;
        let last = content_right - CARET_WIDTH;
        let x = origin + text.caret_x(ui.selection.1);
        let caret = Rect {
            x: if x > last && x < content_right + 1.0 {
                last
            } else {
                x
            },
            width: CARET_WIDTH,
            ..glyphs
        };
        painter.fill_rect(caret, 0.0, color::INK, None);
    }
    painter.restore();
}

/// The highlight over the selected characters: their bounds, merged where
/// they touch.
fn selection_spans(selected: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = selected.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for (left, right) in sorted {
        match spans.last_mut() {
            Some(last) if left <= last.1 + TOUCHING => last.1 = last.1.max(right),
            _ => spans.push((left, right)),
        }
    }
    spans
}

fn paint_success(success: &SuccessLayout, measure: &impl Measure, painter: &mut impl Painter) {
    let icon = success.icon;
    painter.fill_rect(icon, icon.width / 2.0, color::SUCCESS_SOFT, None);
    let inset = (icon.width - SUCCESS_SIZE) / 2.0;
    let origin = (icon.x + inset, icon.y + inset);
    painter.stroke_path(SUCCESS_PATH, origin, SUCCESS_STROKE, color::SUCCESS);
    paint_text(&success.title, color::INK, measure, painter);
    paint_text(&success.body, color::MUTED, measure, painter);
}

fn paint_text(block: &TextBlock, color: &str, measure: &impl Measure, painter: &mut impl Painter) {
    let metrics = measure.metrics(block.style);
    for (index, line) in block.lines.iter().enumerate() {
        let x = match block.align {
            Align::Start => block.rect.x,
            Align::Center => {
                block.rect.x + (block.rect.width - measure.width(line, block.style)) / 2.0
            }
        };
        let line_baseline = baseline(block.line_top(index), block.style, metrics);
        painter.fill_text(line, x, line_baseline, block.style, color);
    }
}

/// A CSS box with a 1px border: the border color fills the border box, and
/// the background fills the padding box, whose corners are 1px tighter.
fn bordered(
    painter: &mut impl Painter,
    rect: Rect,
    radius: f64,
    border: &str,
    background: &str,
    shadow: Option<Shadow>,
) {
    painter.fill_rect(rect, radius, border, shadow);
    painter.fill_rect(rect.outset(-BORDER), radius - BORDER, background, None);
}

/// Tailwind's `ring`: a spread shadow around the border box.
fn ring(painter: &mut impl Painter, rect: Rect, radius: f64, invalid: bool) {
    let tone = if invalid {
        color::DANGER_RING
    } else {
        color::BRAND_RING
    };
    let outset = spacing::RING;
    painter.fill_rect(rect.outset(outset), radius + outset, tone, None);
}

#[cfg(test)]
pub mod testing {
    use super::Painter;
    use crate::layout::Rect;
    use crate::style::{Shadow, TextStyle};

    /// One recorded drawing operation.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Op {
        Rect {
            rect: Rect,
            radius: f64,
            color: String,
            shadow: Option<Shadow>,
        },
        Path {
            path: String,
            origin: (f64, f64),
            width: f64,
            color: String,
        },
        Text {
            text: String,
            x: f64,
            baseline: f64,
            color: String,
        },
        Clip(Rect),
        Restore,
    }

    /// Records operations instead of drawing them.
    #[derive(Debug, Default)]
    pub struct RecordingPainter {
        pub ops: Vec<Op>,
    }

    impl RecordingPainter {
        /// The recorded text operation for `text`, if any.
        #[must_use]
        pub fn text(&self, text: &str) -> Option<&Op> {
            self.ops
                .iter()
                .find(|op| matches!(op, Op::Text { text: t, .. } if t == text))
        }

        /// Every rectangle filled with `color`.
        #[must_use]
        pub fn rects(&self, color: &str) -> Vec<Rect> {
            self.ops
                .iter()
                .filter_map(|op| match op {
                    Op::Rect { rect, color: c, .. } if c == color => Some(*rect),
                    _ => None,
                })
                .collect()
        }
    }

    impl Painter for RecordingPainter {
        fn fill_rect(&mut self, rect: Rect, radius: f64, color: &str, shadow: Option<Shadow>) {
            self.ops.push(Op::Rect {
                rect,
                radius,
                color: color.to_owned(),
                shadow,
            });
        }

        fn stroke_path(&mut self, path: &str, origin: (f64, f64), width: f64, color: &str) {
            self.ops.push(Op::Path {
                path: path.to_owned(),
                origin,
                width,
                color: color.to_owned(),
            });
        }

        fn fill_text(&mut self, text: &str, x: f64, baseline: f64, _: TextStyle, color: &str) {
            self.ops.push(Op::Text {
                text: text.to_owned(),
                x,
                baseline,
                color: color.to_owned(),
            });
        }

        fn clip(&mut self, rect: Rect) {
            self.ops.push(Op::Clip(rect));
        }

        fn restore(&mut self) {
            self.ops.push(Op::Restore);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Op, RecordingPainter};
    use super::*;
    use crate::grpc_web::CallError;
    use crate::layout::{Viewport, layout};
    use crate::proto::SignUpResponse;
    use crate::text::testing::FixedMeasure;

    const DESKTOP: Viewport = Viewport {
        width: 1280.0,
        height: 900.0,
        media_width: 1280.0,
    };

    fn paint_with(state: &FormState, ui: &Interaction) -> (Layout, RecordingPainter) {
        let page = layout(DESKTOP, state, &FixedMeasure);
        let mut painter = RecordingPainter::default();
        paint(&page, state, ui, &FixedMeasure, &mut painter);
        (page, painter)
    }

    fn form_of(page: &Layout) -> &FormLayout {
        match &page.content {
            Content::Form(form) => form,
            Content::Success(_) => panic!("expected the form"),
        }
    }

    fn color_of(painter: &RecordingPainter, text: &str) -> String {
        match painter.text(text) {
            Some(Op::Text { color, .. }) => color.clone(),
            other => panic!("no text {text:?}: {other:?}"),
        }
    }

    fn filled(state: &mut FormState) {
        state.set_text(Field::FullName, "Ada Lovelace");
        state.set_text(Field::Email, "ada@example.com");
        state.set_text(Field::Password, "Analytical#Engine1");
        state.set_text(Field::ConfirmPassword, "Analytical#Engine1");
        state.set_accept_terms(true);
    }

    #[test]
    fn paints_the_page_and_a_bordered_card_with_its_shadow() {
        let (page, painter) = paint_with(&FormState::default(), &Interaction::default());
        let Op::Rect { rect, color, .. } = &painter.ops[0] else {
            panic!("expected the page background first");
        };
        assert_eq!(*rect, Rect::new(0.0, 0.0, page.width, page.height));
        assert_eq!(color, color::PAGE);
        assert_eq!(
            painter.ops[1],
            Op::Rect {
                rect: page.card,
                radius: radius::CARD,
                color: color::LINE.to_owned(),
                shadow: Some(shadow::CARD),
            }
        );
        assert_eq!(
            painter.ops[2],
            Op::Rect {
                rect: page.card.outset(-BORDER),
                radius: radius::CARD - BORDER,
                color: color::SURFACE.to_owned(),
                shadow: None,
            }
        );
    }

    #[test]
    fn paints_an_empty_form_with_placeholders_and_the_hint() {
        let (page, painter) = paint_with(&FormState::default(), &Interaction::default());
        let form = form_of(&page);
        assert_eq!(color_of(&painter, "np"), color::ON_BRAND);
        assert_eq!(color_of(&painter, "Create your account"), color::INK);
        assert_eq!(color_of(&painter, "Full name"), color::INK_SOFT);
        assert_eq!(color_of(&painter, "Ada Lovelace"), color::SUBTLE);
        assert_eq!(color_of(&painter, "Create account"), color::ON_BRAND);
        let hint = &form.fields[2].message.as_ref().expect("hint").lines[0];
        assert_eq!(color_of(&painter, hint), color::SUBTLE);
        assert_eq!(painter.rects(color::BRAND_RING), []);
        assert_eq!(painter.rects(color::BRAND), [form.logo, form.button]);
        assert_eq!(painter.rects(color::FIELD).len(), 5);

        // The placeholder sits in the content box. FixedMeasure fonts have a
        // 2px half-leading and a 16px ascent in the 24px line.
        let input = form.fields[0].input;
        let Some(Op::Text { x, baseline, .. }) = painter.text("Ada Lovelace") else {
            panic!("expected the placeholder");
        };
        assert_eq!(*x, input.x + BORDER + spacing::CONTROL_X);
        assert_eq!(*baseline, input.y + 10.0 + 2.0 + 16.0);
        assert!(painter.ops.contains(&Op::Clip(Rect::new(
            input.x + 13.0,
            input.y + 1.0,
            input.width - 26.0,
            42.0
        ))));
    }

    #[test]
    fn a_focused_field_gets_a_ring_a_brand_border_and_a_caret() {
        let mut state = FormState::default();
        state.set_text(Field::FullName, "Ada");
        let ui = Interaction {
            focus: Some(Control::Field(Field::FullName)),
            caret_visible: true,
            scroll_left: 7.0,
            selection: (2, 2),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &ui);
        let input = form_of(&page).fields[0].input;
        assert_eq!(painter.rects(color::BRAND_RING), [input.outset(3.0)]);
        assert!(painter.rects(color::BRAND).contains(&input));
        let Some(Op::Text { x, .. }) = painter.text("Ada") else {
            panic!("expected the value");
        };
        let origin = input.x + 13.0 - 7.0;
        assert_eq!(*x, origin);
        // The caret follows two 8px characters and spans ascent plus descent.
        let caret = Rect::new(origin + 16.0, input.y + 12.0, 1.0, 20.0);
        assert!(painter.rects(color::INK).contains(&caret));

        let blink = Interaction {
            caret_visible: false,
            ..ui
        };
        let (_, painter) = paint_with(&state, &blink);
        assert_eq!(painter.rects(color::INK), []);
    }

    #[test]
    fn the_caret_stays_inside_a_scrolled_input() {
        let mut state = FormState::default();
        state.set_text(Field::FullName, &"a".repeat(60));
        let (page, _) = paint_with(&state, &Interaction::default());
        let input = form_of(&page).fields[0].input;
        let content_right = input.x + input.width - 13.0;
        // Scrolled so the text's 480px end lands exactly on the clip edge.
        let ui = Interaction {
            focus: Some(Control::Field(Field::FullName)),
            caret_visible: true,
            scroll_left: 480.0 - (input.width - 26.0),
            selection: (60, 60),
            ..Interaction::default()
        };
        let (_, painter) = paint_with(&state, &ui);
        let caret = Rect::new(content_right - 1.0, input.y + 12.0, 1.0, 20.0);
        assert_eq!(painter.rects(color::INK), [caret]);

        // Scrolled back to the start, the caret is out of view and clipped.
        let start = Interaction {
            scroll_left: 0.0,
            ..ui
        };
        let (_, painter) = paint_with(&state, &start);
        let caret = Rect::new(input.x + 13.0 + 480.0, input.y + 12.0, 1.0, 20.0);
        assert_eq!(painter.rects(color::INK), [caret]);
    }

    #[test]
    fn a_selection_is_highlighted_instead_of_the_caret() {
        let mut state = FormState::default();
        state.set_text(Field::Password, "secret");
        let ui = Interaction {
            focus: Some(Control::Field(Field::Password)),
            caret_visible: true,
            selection: (1, 4),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &ui);
        let input = form_of(&page).fields[2].input;
        // Passwords are masked, one bullet per character.
        assert_eq!(color_of(&painter, "••••••"), color::INK);
        let highlight = Rect::new(input.x + 13.0 + 8.0, input.y + 12.0, 24.0, 20.0);
        assert_eq!(
            painter.rects(color::BRAND_RING),
            [input.outset(3.0), highlight]
        );
        assert_eq!(painter.rects(color::INK), []);
    }

    #[test]
    fn a_password_shows_one_bullet_per_grapheme() {
        let mut state = FormState::default();
        // FixedMeasure joins the acute accent to its "e".
        state.set_text(Field::Password, "e\u{301}e\u{301}x");
        let ui = Interaction {
            focus: Some(Control::Field(Field::Password)),
            caret_visible: true,
            selection: (2, 2),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &ui);
        let input = form_of(&page).fields[2].input;
        assert_eq!(color_of(&painter, "•••"), color::INK);
        // After the first "e" and its accent: one 8px bullet.
        let caret = Rect::new(input.x + 13.0 + 8.0, input.y + 12.0, 1.0, 20.0);
        assert_eq!(painter.rects(color::INK), [caret]);
    }

    #[test]
    fn right_to_left_text_places_the_caret_and_selection_visually() {
        let mut state = FormState::default();
        state.set_text(Field::FullName, "\u{5E9}\u{5DC}\u{5D5}\u{5DD}");
        let caret_at = |offset: usize| {
            let ui = Interaction {
                focus: Some(Control::Field(Field::FullName)),
                caret_visible: true,
                selection: (offset, offset),
                ..Interaction::default()
            };
            let (page, painter) = paint_with(&state, &ui);
            let input = form_of(&page).fields[0].input;
            let [caret] = painter.rects(color::INK)[..] else {
                panic!("expected one caret");
            };
            caret.x - (input.x + 13.0)
        };
        // Like Chrome: the line's edges at the ends, and between characters,
        // the first of which is the rightmost of the four 8px characters.
        assert_eq!(caret_at(0), 0.0);
        assert_eq!(caret_at(1), 24.0);
        assert_eq!(caret_at(4), 32.0);

        let first_two = Interaction {
            focus: Some(Control::Field(Field::FullName)),
            selection: (0, 2),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &first_two);
        let input = form_of(&page).fields[0].input;
        let highlight = Rect::new(input.x + 13.0 + 16.0, input.y + 12.0, 16.0, 20.0);
        assert!(painter.rects(color::BRAND_RING).contains(&highlight));
    }

    #[test]
    fn selections_merge_where_characters_touch() {
        assert_eq!(
            selection_spans(&[(8.0, 16.0), (0.0, 8.0), (20.0, 24.0)]),
            [(0.0, 16.0), (20.0, 24.0)]
        );
    }

    #[test]
    fn invalid_fields_turn_red_even_with_focus() {
        let mut state = FormState::default();
        state.begin_submit();
        let ui = Interaction {
            focus: Some(Control::Field(Field::Email)),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &ui);
        let form = form_of(&page);
        let email = form.fields[1].input;
        assert_eq!(painter.rects(color::DANGER_RING), [email.outset(3.0)]);
        assert_eq!(painter.rects(color::DANGER).len(), 5);
        assert_eq!(
            color_of(&painter, "Enter your email address."),
            color::DANGER
        );
        assert_eq!(
            color_of(&painter, "Accept the terms to continue."),
            color::DANGER
        );
    }

    #[test]
    fn the_checkbox_and_button_show_rings_only_when_focus_is_visible() {
        let mut state = FormState::default();
        state.set_accept_terms(true);
        let terms = Interaction {
            focus: Some(Control::Field(Field::AcceptTerms)),
            ..Interaction::default()
        };
        let (page, painter) = paint_with(&state, &terms);
        let form = form_of(&page);
        assert_eq!(painter.rects(color::BRAND_RING), []);
        assert!(painter.rects(color::BRAND).contains(&form.terms.check));
        assert!(painter.ops.contains(&Op::Path {
            path: CHECK_PATH.to_owned(),
            origin: (form.terms.check.x, form.terms.check.y),
            width: 2.0,
            color: color::ON_BRAND.to_owned(),
        }));

        let visible = Interaction {
            focus_visible: true,
            ..terms
        };
        let (_, painter) = paint_with(&state, &visible);
        assert_eq!(
            painter.rects(color::BRAND_RING),
            [form.terms.check.outset(3.0)]
        );

        let button = Interaction {
            focus: Some(Control::Button),
            focus_visible: true,
            hovered: true,
            ..Interaction::default()
        };
        let (_, painter) = paint_with(&state, &button);
        assert_eq!(painter.rects(color::BRAND_RING), [form.button.outset(3.0)]);
        assert_eq!(painter.rects(color::BRAND_STRONG), [form.button]);
    }

    #[test]
    fn paints_the_network_error_and_the_success_panel() {
        let mut state = FormState::default();
        filled(&mut state);
        state.begin_submit();
        state.finish_submit(Err(CallError::Http(503)));
        let (_, painter) = paint_with(&state, &Interaction::default());
        let error = "We couldn't reach the server. Please try again.";
        assert_eq!(color_of(&painter, error), color::DANGER);

        state.begin_submit();
        state.finish_submit(Ok(SignUpResponse::default()));
        let (page, painter) = paint_with(&state, &Interaction::default());
        let Content::Success(success) = &page.content else {
            panic!("expected the success panel");
        };
        assert_eq!(
            painter.rects(color::SUCCESS_SOFT),
            [success.icon],
            "a circle: the radius is half the size"
        );
        assert!(painter.ops.contains(&Op::Path {
            path: SUCCESS_PATH.to_owned(),
            origin: (success.icon.x + 12.0, success.icon.y + 12.0),
            width: 2.5,
            color: color::SUCCESS.to_owned(),
        }));
        let Some(Op::Text { x, color, .. }) = painter.text("Account created") else {
            panic!("expected the title");
        };
        assert_eq!(color, color::INK);
        let title_width = 15.0 * text::TITLE.size / 2.0;
        assert_eq!(
            *x,
            success.title.rect.x + (success.title.rect.width - title_width) / 2.0
        );
        assert_eq!(color_of(&painter, &success.body.lines[0]), color::MUTED);
    }

    #[test]
    fn converts_utf16_offsets_to_characters() {
        assert_eq!(char_offset("a😀b", 0), 0);
        assert_eq!(char_offset("a😀b", 1), 1);
        assert_eq!(char_offset("a😀b", 3), 2);
        assert_eq!(char_offset("a😀b", 4), 3);
        assert_eq!(char_offset("a😀b", 9), 3);
    }
}
