//! Places every element exactly where the React app's Tailwind classes put it:
//! a centered card on a padded page, with one breakpoint (`sm`).

use form_core::Field;

use crate::copy;
use crate::form::{FormState, Status};
use crate::style::TextStyle;
use crate::text::{Measure, min_content_width, wrap};
use crate::tokens::{breakpoint, container, spacing, text};

/// Tailwind's `border` width.
pub const BORDER: f64 = 1.0;

/// The text fields in form order (the terms checkbox is laid out separately).
pub const TEXT_FIELDS: [Field; 4] = [
    Field::FullName,
    Field::Email,
    Field::Password,
    Field::ConfirmPassword,
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    /// Grows (or, for negative `by`, shrinks) the rectangle on every side.
    #[must_use]
    pub fn outset(self, by: f64) -> Self {
        Self::new(
            self.x - by,
            self.y - by,
            self.width + 2.0 * by,
            self.height + 2.0 * by,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
}

/// Wrapped text inside a block whose lines are each `style.line_height` tall.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<String>,
    pub rect: Rect,
    pub style: TextStyle,
    pub align: Align,
}

impl TextBlock {
    fn new(
        content: &str,
        style: TextStyle,
        frame: Rect,
        align: Align,
        measure: &impl Measure,
    ) -> Self {
        let lines = wrap(content, style, frame.width, measure);
        #[allow(clippy::cast_precision_loss)]
        let height = lines.len() as f64 * style.line_height;
        Self {
            lines,
            rect: Rect { height, ..frame },
            style,
            align,
        }
    }

    /// The top edge of line `index`.
    #[must_use]
    pub fn line_top(&self, index: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let offset = index as f64 * self.style.line_height;
        self.rect.y + offset
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldLayout {
    pub field: Field,
    pub label: TextBlock,
    pub input: Rect,
    pub message: Option<TextBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TermsLayout {
    pub check: Rect,
    pub label: TextBlock,
    pub message: Option<TextBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormLayout {
    pub logo: Rect,
    pub logo_text: TextBlock,
    pub title: TextBlock,
    pub subtitle: TextBlock,
    pub fields: Vec<FieldLayout>,
    pub terms: TermsLayout,
    pub button: Rect,
    pub button_text: TextBlock,
    pub error: Option<TextBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuccessLayout {
    pub icon: Rect,
    pub title: TextBlock,
    pub body: TextBlock,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Form(Box<FormLayout>),
    Success(SuccessLayout),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Document width and height in CSS pixels.
    pub width: f64,
    pub height: f64,
    pub card: Rect,
    pub content: Content,
}

/// The browser window as CSS sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Layout width, excluding any classic scrollbar (`clientWidth`).
    pub width: f64,
    /// Viewport height (`innerHeight`), the `min-h-dvh` of the page.
    pub height: f64,
    /// Width used by media queries (`innerWidth`).
    pub media_width: f64,
}

/// Lays out the page for `state`.
#[must_use]
pub fn layout(viewport: Viewport, state: &FormState, measure: &impl Measure) -> Layout {
    let wide = viewport.media_width >= breakpoint::SM;
    let (page_y, card_pad) = if wide {
        (spacing::PAGE_Y_WIDE, spacing::CARD_PAD_WIDE)
    } else {
        (spacing::PAGE_Y, spacing::CARD_PAD)
    };
    let card_width = container::CARD
        .min(viewport.width - 2.0 * spacing::PAGE_X)
        .max(0.0);
    let card_x = (viewport.width - card_width) / 2.0;
    let inset = BORDER + card_pad;
    let column = |top: f64| {
        Rect::new(
            card_x + inset,
            top,
            (card_width - 2.0 * inset).max(0.0),
            0.0,
        )
    };

    // Measure the content once to size the card, then place it for real.
    let (_, content_height) = content(column(0.0), state, measure);
    let card_height = content_height + 2.0 * inset;
    let height = viewport.height.max(card_height + 2.0 * page_y);
    let card = Rect::new(
        card_x,
        (height - card_height) / 2.0,
        card_width,
        card_height,
    );
    let (content, _) = content(column(card.y + inset), state, measure);

    Layout {
        width: viewport.width,
        height,
        card,
        content,
    }
}

/// Lays out the card content from `column.y` downwards; returns it and its height.
fn content(column: Rect, state: &FormState, measure: &impl Measure) -> (Content, f64) {
    if state.status() == Status::Success {
        let (success, bottom) = success(column, state.display_name(), measure);
        (Content::Success(success), bottom - column.y)
    } else {
        let (form, bottom) = form(column, state, measure);
        (Content::Form(Box::new(form)), bottom - column.y)
    }
}

fn form(column: Rect, state: &FormState, measure: &impl Measure) -> (FormLayout, f64) {
    let block = |content: &str, style: TextStyle, top: f64| {
        TextBlock::new(
            content,
            style,
            Rect { y: top, ..column },
            Align::Start,
            measure,
        )
    };

    let logo = Rect::new(column.x, column.y, spacing::LOGO, spacing::LOGO);
    let logo_text = centered_line(copy::BRAND, text::LOGO, logo, measure);
    let title = block(
        copy::TITLE,
        text::TITLE,
        logo.bottom() + spacing::HEADER_GAP,
    );
    let subtitle = block(
        copy::SUBTITLE,
        text::SMALL,
        title.rect.bottom() + spacing::TITLE_GAP,
    );

    let mut y = subtitle.rect.bottom() + spacing::FORM_GAP;
    let mut fields = Vec::with_capacity(TEXT_FIELDS.len());
    for field in TEXT_FIELDS {
        let label_text = copy::field(field.key()).map_or("", |copy| copy.label);
        let label = block(label_text, text::LABEL, y);
        let input = Rect::new(
            column.x,
            label.rect.bottom() + spacing::LABEL_GAP,
            column.width,
            spacing::CONTROL,
        );
        let message = state
            .message(field)
            .map(|message| block(message, text::SMALL, input.bottom() + spacing::LABEL_GAP));
        y = message.as_ref().map_or(input.bottom(), |m| m.rect.bottom()) + spacing::FIELD_GAP;
        fields.push(FieldLayout {
            field,
            label,
            input,
            message,
        });
    }

    let check = Rect::new(column.x, y, spacing::CHECK, spacing::CHECK);
    let label_x = column.x + spacing::CHECK + spacing::CHECK_GAP;
    let label_frame = Rect::new(
        label_x,
        y,
        column.width - spacing::CHECK - spacing::CHECK_GAP,
        0.0,
    );
    let label = TextBlock::new(copy::TERMS, text::SMALL, label_frame, Align::Start, measure);
    let row_bottom = check.bottom().max(label.rect.bottom());
    let message = state
        .message(Field::AcceptTerms)
        .map(|message| block(message, text::SMALL, row_bottom + spacing::LABEL_GAP));
    y = message.as_ref().map_or(row_bottom, |m| m.rect.bottom()) + spacing::FIELD_GAP;
    let terms = TermsLayout {
        check,
        label,
        message,
    };

    let button = Rect::new(column.x, y, column.width, spacing::CONTROL);
    let caption = if state.locked() {
        copy::SUBMITTING
    } else {
        copy::SUBMIT
    };
    let button_text = centered_line(caption, text::BUTTON, button, measure);
    let error = (state.status() == Status::Failed).then(|| {
        block(
            copy::NETWORK_ERROR,
            text::SMALL,
            button.bottom() + spacing::FIELD_GAP,
        )
    });
    let bottom = error.as_ref().map_or(button.bottom(), |e| e.rect.bottom());

    let layout = FormLayout {
        logo,
        logo_text,
        title,
        subtitle,
        fields,
        terms,
        button,
        button_text,
        error,
    };
    (layout, bottom)
}

fn success(column: Rect, name: &str, measure: &impl Measure) -> (SuccessLayout, f64) {
    let icon = Rect::new(
        column.x + (column.width - spacing::ICON) / 2.0,
        column.y,
        spacing::ICON,
        spacing::ICON,
    );
    let title_frame = Rect {
        y: icon.bottom() + spacing::HEADER_GAP,
        ..column
    };
    let title = TextBlock::new(
        copy::SUCCESS_TITLE,
        text::TITLE,
        title_frame,
        Align::Center,
        measure,
    );
    let body_frame = Rect {
        y: title.rect.bottom() + spacing::TITLE_GAP,
        ..column
    };
    let message = copy::SUCCESS_BODY.replace("{name}", name);
    // A centered flex item grows to its min-content width when a word is
    // wider than the column, then stays centered on the column.
    let width = column
        .width
        .max(min_content_width(&message, text::SMALL, measure));
    let body_frame = Rect {
        x: column.x + (column.width - width) / 2.0,
        width,
        ..body_frame
    };
    let body = TextBlock::new(&message, text::SMALL, body_frame, Align::Center, measure);
    let bottom = body.rect.bottom();
    (SuccessLayout { icon, title, body }, bottom)
}

/// A single line centered in `frame`, as flex centering does in the DOM.
fn centered_line(
    content: &str,
    style: TextStyle,
    frame: Rect,
    measure: &impl Measure,
) -> TextBlock {
    let top = frame.y + (frame.height - style.line_height) / 2.0;
    let line = Rect::new(frame.x, top, frame.width, 0.0);
    TextBlock::new(content, style, line, Align::Center, measure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc_web::CallError;
    use crate::proto::SignUpResponse;
    use crate::text::testing::FixedMeasure;

    const PHONE: Viewport = Viewport {
        width: 390.0,
        height: 844.0,
        media_width: 390.0,
    };
    const DESKTOP: Viewport = Viewport {
        width: 1280.0,
        height: 2000.0,
        media_width: 1280.0,
    };

    fn form_of(layout: &Layout) -> &FormLayout {
        match &layout.content {
            Content::Form(form) => form,
            Content::Success(_) => panic!("expected the form"),
        }
    }

    #[test]
    fn phone_layout_fills_the_width_minus_page_padding() {
        let page = layout(PHONE, &FormState::default(), &FixedMeasure);
        assert_eq!(page.card.x, 16.0);
        assert_eq!(page.card.width, 358.0);
        let form = form_of(&page);
        assert_eq!(form.logo.x, 16.0 + BORDER + spacing::CARD_PAD);
        assert_eq!(
            form.fields[0].input.width,
            358.0 - 2.0 * (BORDER + spacing::CARD_PAD)
        );
    }

    #[test]
    fn wide_layout_centers_a_fixed_width_card() {
        let page = layout(DESKTOP, &FormState::default(), &FixedMeasure);
        assert_eq!(page.card.width, container::CARD);
        assert_eq!(page.card.x, (1280.0 - container::CARD) / 2.0);
        assert_eq!(page.height, 2000.0);
        assert_eq!(page.card.y, (2000.0 - page.card.height) / 2.0);
        assert_eq!(
            form_of(&page).logo.x,
            page.card.x + BORDER + spacing::CARD_PAD_WIDE
        );
    }

    #[test]
    fn short_viewports_grow_the_document_and_keep_the_page_padding() {
        let short = Viewport {
            height: 300.0,
            ..PHONE
        };
        let page = layout(short, &FormState::default(), &FixedMeasure);
        assert_eq!(page.card.y, spacing::PAGE_Y);
        assert_eq!(page.height, page.card.height + 2.0 * spacing::PAGE_Y);
    }

    #[test]
    fn stacks_the_form_like_the_dom() {
        let page = layout(DESKTOP, &FormState::default(), &FixedMeasure);
        let form = form_of(&page);
        assert_eq!(form.title.rect.y, form.logo.bottom() + spacing::HEADER_GAP);
        assert_eq!(
            form.logo_text.rect.y,
            form.logo.y + (spacing::LOGO - text::LOGO.line_height) / 2.0
        );
        assert_eq!(
            form.subtitle.rect.y,
            form.title.rect.bottom() + spacing::TITLE_GAP
        );
        let first = &form.fields[0];
        assert_eq!(
            first.label.rect.y,
            form.subtitle.rect.bottom() + spacing::FORM_GAP
        );
        assert_eq!(
            first.input.y,
            first.label.rect.bottom() + spacing::LABEL_GAP
        );
        assert_eq!(first.message, None);
        let password = &form.fields[2];
        let hint = password.message.as_ref().expect("password hint");
        assert_eq!(hint.rect.y, password.input.bottom() + spacing::LABEL_GAP);
        assert_eq!(
            form.fields[3].label.rect.y,
            hint.rect.bottom() + spacing::FIELD_GAP
        );
        assert_eq!(
            form.terms.check.y,
            form.fields[3].input.bottom() + spacing::FIELD_GAP
        );
        assert_eq!(
            form.terms.label.rect.x,
            form.terms.check.x + spacing::CHECK + spacing::CHECK_GAP
        );
        let row_bottom = form
            .terms
            .check
            .bottom()
            .max(form.terms.label.rect.bottom());
        assert_eq!(form.button.y, row_bottom + spacing::FIELD_GAP);
        assert_eq!(form.button_text.lines, [copy::SUBMIT]);
        assert_eq!(form.error, None);
        let inset = BORDER + spacing::CARD_PAD_WIDE;
        assert_eq!(page.card.bottom(), form.button.bottom() + inset);
    }

    #[test]
    fn errors_push_the_following_fields_down() {
        let mut state = FormState::default();
        let before = layout(PHONE, &state, &FixedMeasure);
        state.begin_submit();
        let after = layout(PHONE, &state, &FixedMeasure);
        let (before, after) = (form_of(&before), form_of(&after));
        assert!(after.fields[0].message.is_some());
        assert!(after.fields[1].input.y > before.fields[1].input.y);
        let terms = after.terms.message.as_ref().expect("terms error");
        assert_eq!(terms.lines.join(" "), "Accept the terms to continue.");
        assert_eq!(after.button.y, terms.rect.bottom() + spacing::FIELD_GAP);
    }

    #[test]
    fn submitting_and_failure_change_the_button_area() {
        let mut state = FormState::default();
        state.set_text(Field::FullName, "Ada Lovelace");
        state.set_text(Field::Email, "ada@example.com");
        state.set_text(Field::Password, "Analytical#Engine1");
        state.set_text(Field::ConfirmPassword, "Analytical#Engine1");
        state.set_accept_terms(true);
        state.begin_submit();
        let page = layout(DESKTOP, &state, &FixedMeasure);
        assert_eq!(form_of(&page).button_text.lines, [copy::SUBMITTING]);

        state.finish_submit(Err(CallError::Http(500)));
        let page = layout(DESKTOP, &state, &FixedMeasure);
        let form = form_of(&page);
        let error = form.error.as_ref().expect("network error");
        assert_eq!(error.rect.y, form.button.bottom() + spacing::FIELD_GAP);
        assert_eq!(
            page.card.bottom(),
            error.rect.bottom() + BORDER + spacing::CARD_PAD_WIDE
        );

        state.begin_submit();
        state.finish_submit(Ok(SignUpResponse::default()));
        let page = layout(DESKTOP, &state, &FixedMeasure);
        let Content::Success(success) = &page.content else {
            panic!("expected the success panel");
        };
        assert_eq!(
            success.icon.x,
            page.card.x + (page.card.width - spacing::ICON) / 2.0
        );
        assert_eq!(success.title.align, Align::Center);
        assert_eq!(
            success.body.lines.join(" "),
            "Welcome aboard, Ada Lovelace. Your account is ready."
        );
        assert_eq!(
            success.body.line_top(1),
            success.body.rect.y + text::SMALL.line_height
        );
    }

    #[test]
    fn success_text_grows_to_fit_an_unbreakable_name() {
        let wide_word = "W".repeat(60);
        let mut state = FormState::default();
        state.set_text(Field::FullName, &format!("{wide_word} Lovelace"));
        state.set_text(Field::Email, "ada@example.com");
        state.set_text(Field::Password, "Analytical#Engine1");
        state.set_text(Field::ConfirmPassword, "Analytical#Engine1");
        state.set_accept_terms(true);
        state.begin_submit();
        state.finish_submit(Ok(SignUpResponse::default()));

        let page = layout(PHONE, &state, &FixedMeasure);
        let Content::Success(success) = &page.content else {
            panic!("expected the success panel");
        };
        // Sixty 7px characters are wider than the 308px column.
        assert_eq!(success.body.rect.width, 420.0);
        assert_eq!(
            success.body.rect.x + 210.0,
            page.card.x + page.card.width / 2.0
        );
        assert_eq!(
            success.body.lines,
            [
                "Welcome aboard,",
                wide_word.as_str(),
                "Lovelace. Your account is ready."
            ]
        );
    }

    #[test]
    fn rect_helpers() {
        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(rect.bottom(), 60.0);
        assert_eq!(rect.outset(3.0), Rect::new(7.0, 17.0, 36.0, 46.0));
    }
}
