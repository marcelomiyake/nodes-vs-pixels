//! The transparent DOM laid over the canvas: native controls for focus,
//! typing and autofill, and the text that the accessibility tree reads. It
//! has the React app's ids, roles and ARIA attributes, and every element sits
//! on what the canvas paints for it, so clicking a label focuses its control
//! just as it does in the React app.

use form_core::Field;

use crate::copy;
use crate::form::{FormState, Status};
use crate::layout::{Content, Layout, Rect, TEXT_FIELDS};
use crate::style::TextStyle;
use crate::tokens::text;

/// An element created once, when the app starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub id: String,
    pub tag: &'static str,
    /// The parent's id, or `None` for the app root.
    pub parent: Option<&'static str>,
    pub attributes: Vec<(&'static str, String)>,
    /// Text content that never changes.
    pub text: &'static str,
    /// The font, so text boxes and input scrolling match the painted text.
    pub style: Option<TextStyle>,
}

/// A change to apply to an element on every render.
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    /// Where the element sits, in document pixels; `None` hides it.
    Rect(Option<Rect>),
    Text(String),
    /// Sets an attribute, or removes it when `None`.
    Attribute(&'static str, Option<String>),
    /// The value of a text input, as a controlled React input keeps it.
    Value(String),
    Checked(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub id: String,
    pub props: Vec<Prop>,
}

/// The id of the success heading, which takes focus after signing up.
pub const SUCCESS_TITLE: &str = "success-title";
/// The id of the submit button.
pub const SUBMIT: &str = "submit";

fn message_id(field: Field) -> String {
    format!("{}-message", field.key())
}

fn label_id(field: Field) -> String {
    format!("{}-label", field.key())
}

fn template(
    id: impl Into<String>,
    tag: &'static str,
    parent: Option<&'static str>,
    attributes: &[(&'static str, &str)],
    (text, style): (&'static str, Option<TextStyle>),
) -> Template {
    Template {
        id: id.into(),
        tag,
        parent,
        attributes: attributes
            .iter()
            .map(|&(name, value)| (name, value.to_owned()))
            .collect(),
        text,
        style,
    }
}

/// An element whose text changes, or that has none.
const fn dynamic(style: TextStyle) -> (&'static str, Option<TextStyle>) {
    ("", Some(style))
}

const NONE: (&str, Option<TextStyle>) = ("", None);

/// The label, input and message of a text field.
fn text_field_templates(field: Field) -> Vec<Template> {
    let form = Some("form");
    let key = field.key();
    let Some(copy) = copy::field(key) else {
        return Vec::new();
    };
    let mut attributes = vec![
        ("name", key),
        ("type", copy.input_type),
        ("autocomplete", copy.autocomplete),
        ("placeholder", copy.placeholder),
    ];
    if copy.input_type == "email" {
        // Chromium offers no selection on email inputs, and the canvas paints
        // the caret from it, so email takes a text input with its keyboard.
        attributes[1] = ("type", "text");
        attributes.push(("inputmode", "email"));
    }
    vec![
        template(
            label_id(field),
            "label",
            form,
            &[("for", key)],
            (copy.label, Some(text::LABEL)),
        ),
        template(key, "input", form, &attributes, dynamic(text::BODY)),
        template(message_id(field), "p", form, &[], dynamic(text::SMALL)),
    ]
}

/// Every overlay element, parents before children.
#[must_use]
pub fn templates() -> Vec<Template> {
    let form = Some("form");
    let mut all = vec![
        template("title", "h1", None, &[], (copy::TITLE, Some(text::TITLE))),
        template(
            "subtitle",
            "p",
            None,
            &[],
            (copy::SUBTITLE, Some(text::SMALL)),
        ),
        template(
            "form",
            "form",
            None,
            &[
                ("method", "post"),
                ("novalidate", ""),
                ("aria-labelledby", "title"),
            ],
            NONE,
        ),
    ];
    all.extend(TEXT_FIELDS.into_iter().flat_map(text_field_templates));
    let terms = Field::AcceptTerms;
    all.extend([
        template(
            terms.key(),
            "input",
            form,
            &[("name", terms.key()), ("type", "checkbox")],
            NONE,
        ),
        template(
            label_id(terms),
            "label",
            form,
            &[("for", terms.key())],
            (copy::TERMS, Some(text::SMALL)),
        ),
        template(message_id(terms), "p", form, &[], dynamic(text::SMALL)),
        template(
            SUBMIT,
            "button",
            form,
            &[("type", "submit")],
            dynamic(text::BUTTON),
        ),
        template(
            "error",
            "p",
            form,
            &[("role", "alert")],
            dynamic(text::SMALL),
        ),
        template(
            "success",
            "section",
            None,
            &[("aria-labelledby", SUCCESS_TITLE)],
            NONE,
        ),
        template(
            SUCCESS_TITLE,
            "h1",
            Some("success"),
            &[("tabindex", "-1")],
            (copy::SUCCESS_TITLE, Some(text::TITLE)),
        ),
        template(
            "success-body",
            "p",
            Some("success"),
            &[],
            dynamic(text::SMALL),
        ),
    ]);
    all
}

fn update(id: impl Into<String>, props: Vec<Prop>) -> Update {
    Update {
        id: id.into(),
        props,
    }
}

/// An ARIA state such as `aria-invalid="true"`.
fn aria(name: &'static str, on: bool) -> Prop {
    Prop::Attribute(name, Some(on.to_string()))
}

/// Where a container is shown: an empty rect, since only its children are
/// positioned, or hidden.
fn shown(visible: bool) -> Prop {
    Prop::Rect(visible.then_some(Rect::default()))
}

/// The state of every overlay element for `layout`.
#[must_use]
pub fn updates(layout: &Layout, state: &FormState) -> Vec<Update> {
    match &layout.content {
        Content::Form(form) => {
            let submitting = state.status() == Status::Submitting;
            let mut all = vec![
                update("title", vec![Prop::Rect(Some(form.title.rect))]),
                update("subtitle", vec![Prop::Rect(Some(form.subtitle.rect))]),
                update("form", vec![shown(true), aria("aria-busy", submitting)]),
                update("success", vec![shown(false)]),
            ];
            for field in &form.fields {
                let message = field.message.as_ref().map(|message| message.rect);
                all.extend(field_updates(
                    field.field,
                    field.label.rect,
                    field.input,
                    message,
                    state,
                ));
            }
            let terms = &form.terms;
            let message = terms.message.as_ref().map(|message| message.rect);
            all.extend(field_updates(
                Field::AcceptTerms,
                terms.label.rect,
                terms.check,
                message,
                state,
            ));
            let caption = if state.locked() {
                copy::SUBMITTING
            } else {
                copy::SUBMIT
            };
            all.push(update(
                SUBMIT,
                vec![
                    Prop::Rect(Some(form.button)),
                    Prop::Text(caption.to_owned()),
                    Prop::Attribute("disabled", submitting.then(String::new)),
                ],
            ));
            let error = form.error.as_ref().map(|error| error.rect);
            let error_text = if error.is_some() {
                copy::NETWORK_ERROR
            } else {
                ""
            };
            all.push(update(
                "error",
                vec![Prop::Rect(error), Prop::Text(error_text.to_owned())],
            ));
            all
        }
        Content::Success(success) => vec![
            update("title", vec![Prop::Rect(None)]),
            update("subtitle", vec![Prop::Rect(None)]),
            update("form", vec![shown(false)]),
            update("success", vec![shown(true)]),
            update(SUCCESS_TITLE, vec![Prop::Rect(Some(success.title.rect))]),
            update(
                "success-body",
                vec![
                    Prop::Rect(Some(success.body.rect)),
                    Prop::Text(copy::SUCCESS_BODY.replace("{name}", state.display_name())),
                ],
            ),
        ],
    }
}

/// The label, control and message of one field.
fn field_updates(
    field: Field,
    label: Rect,
    control: Rect,
    message: Option<Rect>,
    state: &FormState,
) -> [Update; 3] {
    let key = field.key();
    let invalid = state.error(field).is_some();
    let message_text = state.message(field).filter(|_| message.is_some());
    let described_by = message_text.map(|_| message_id(field));
    let mut props = vec![
        Prop::Rect(Some(control)),
        aria("aria-invalid", invalid),
        Prop::Attribute("aria-describedby", described_by),
    ];
    if field == Field::AcceptTerms {
        props.push(aria("aria-disabled", state.locked()));
        props.push(Prop::Checked(state.values().accept_terms));
    } else {
        props.push(Prop::Attribute(
            "readonly",
            state.locked().then(String::new),
        ));
        props.push(Prop::Value(state.text(field).to_owned()));
    }
    [
        update(label_id(field), vec![Prop::Rect(Some(label))]),
        update(key, props),
        update(
            message_id(field),
            vec![
                Prop::Rect(message),
                Prop::Text(message_text.unwrap_or_default().to_owned()),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc_web::CallError;
    use crate::layout::{Viewport, layout};
    use crate::proto::SignUpResponse;
    use crate::text::testing::FixedMeasure;

    const PHONE: Viewport = Viewport {
        width: 390.0,
        height: 844.0,
        media_width: 390.0,
    };

    fn render(state: &FormState) -> Vec<Update> {
        updates(&layout(PHONE, state, &FixedMeasure), state)
    }

    fn props<'a>(all: &'a [Update], id: &str) -> &'a [Prop] {
        &all.iter()
            .find(|update| update.id == id)
            .unwrap_or_else(|| panic!("no update for {id}"))
            .props
    }

    fn attribute<'a>(all: &'a [Update], id: &str, name: &str) -> Option<&'a str> {
        props(all, id).iter().find_map(|prop| match prop {
            Prop::Attribute(n, value) if *n == name => Some(value.as_deref()),
            _ => None,
        })?
    }

    fn rect(all: &[Update], id: &str) -> Option<Rect> {
        props(all, id).iter().find_map(|prop| match prop {
            Prop::Rect(rect) => Some(*rect),
            _ => None,
        })?
    }

    #[test]
    fn templates_mirror_the_react_markup() {
        let all = templates();
        let ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "title",
                "subtitle",
                "form",
                "fullName-label",
                "fullName",
                "fullName-message",
                "email-label",
                "email",
                "email-message",
                "password-label",
                "password",
                "password-message",
                "confirmPassword-label",
                "confirmPassword",
                "confirmPassword-message",
                "acceptTerms",
                "acceptTerms-label",
                "acceptTerms-message",
                "submit",
                "error",
                "success",
                "success-title",
                "success-body",
            ]
        );
        let email = &all[7];
        assert!(email.attributes.contains(&("type", "text".to_owned())));
        assert!(
            email
                .attributes
                .contains(&("inputmode", "email".to_owned()))
        );
        let password = &all[10];
        assert_eq!(password.tag, "input");
        assert_eq!(password.parent, Some("form"));
        assert!(
            password
                .attributes
                .contains(&("type", "password".to_owned()))
        );
        assert!(
            password
                .attributes
                .contains(&("autocomplete", "new-password".to_owned()))
        );
        assert_eq!(password.style, Some(text::BODY));
        assert_eq!(all[0].text, copy::TITLE);
        assert_eq!(all[9].text, "Password");
        assert!(all[2].attributes.contains(&("method", "post".to_owned())));
        assert_eq!(all[21].parent, Some("success"));
    }

    #[test]
    fn places_controls_on_their_painted_rects() {
        let state = FormState::default();
        let page = layout(PHONE, &state, &FixedMeasure);
        let all = updates(&page, &state);
        let Content::Form(form) = &page.content else {
            panic!("expected the form");
        };
        assert_eq!(rect(&all, "email"), Some(form.fields[1].input));
        assert_eq!(rect(&all, "email-label"), Some(form.fields[1].label.rect));
        assert_eq!(rect(&all, "acceptTerms"), Some(form.terms.check));
        assert_eq!(rect(&all, "submit"), Some(form.button));
        assert_eq!(rect(&all, "success"), None);
        assert_eq!(rect(&all, "error"), None);
        // Only the password field has a hint before anything is touched.
        assert_eq!(rect(&all, "email-message"), None);
        assert_eq!(
            attribute(&all, "password", "aria-describedby"),
            Some("password-message")
        );
        assert_eq!(attribute(&all, "email", "aria-describedby"), None);
        assert_eq!(attribute(&all, "email", "aria-invalid"), Some("false"));
        assert_eq!(attribute(&all, "form", "aria-busy"), Some("false"));
        assert!(props(&all, "email").contains(&Prop::Value(String::new())));
    }

    #[test]
    fn errors_are_described_and_marked_invalid() {
        let mut state = FormState::default();
        state.begin_submit();
        let all = render(&state);
        assert_eq!(attribute(&all, "email", "aria-invalid"), Some("true"));
        assert_eq!(
            attribute(&all, "acceptTerms", "aria-describedby"),
            Some("acceptTerms-message")
        );
        assert!(
            props(&all, "acceptTerms-message")
                .contains(&Prop::Text("Accept the terms to continue.".to_owned()))
        );
        assert!(props(&all, "acceptTerms").contains(&Prop::Checked(false)));
    }

    fn filled() -> FormState {
        let mut state = FormState::default();
        state.set_text(Field::FullName, " Ada Lovelace ");
        state.set_text(Field::Email, "ada@example.com");
        state.set_text(Field::Password, "Analytical#Engine1");
        state.set_text(Field::ConfirmPassword, "Analytical#Engine1");
        state.set_accept_terms(true);
        state
    }

    #[test]
    fn locks_the_controls_while_submitting() {
        let mut state = filled();
        state.begin_submit();
        let all = render(&state);
        assert_eq!(attribute(&all, "form", "aria-busy"), Some("true"));
        assert_eq!(attribute(&all, "fullName", "readonly"), Some(""));
        assert_eq!(
            attribute(&all, "acceptTerms", "aria-disabled"),
            Some("true")
        );
        assert_eq!(attribute(&all, SUBMIT, "disabled"), Some(""));
        assert!(props(&all, SUBMIT).contains(&Prop::Text(copy::SUBMITTING.to_owned())));

        state.finish_submit(Err(CallError::Http(502)));
        let all = render(&state);
        assert_eq!(attribute(&all, "fullName", "readonly"), None);
        assert_eq!(attribute(&all, SUBMIT, "disabled"), None);
        assert!(rect(&all, "error").is_some());
        assert!(props(&all, "error").contains(&Prop::Text(copy::NETWORK_ERROR.to_owned())));
    }

    #[test]
    fn shows_only_the_success_panel_after_signing_up() {
        let mut state = filled();
        state.begin_submit();
        state.finish_submit(Ok(SignUpResponse::default()));
        let page = layout(PHONE, &state, &FixedMeasure);
        let all = updates(&page, &state);
        let Content::Success(success) = &page.content else {
            panic!("expected the success panel");
        };
        assert_eq!(rect(&all, "form"), None);
        assert_eq!(rect(&all, "title"), None);
        assert_eq!(rect(&all, "success"), Some(Rect::default()));
        assert_eq!(rect(&all, SUCCESS_TITLE), Some(success.title.rect));
        assert!(props(&all, "success-body").contains(&Prop::Text(
            "Welcome aboard, Ada Lovelace. Your account is ready.".to_owned()
        )));
    }
}
