//! Event handling, independent of the browser: events from the overlay
//! controls update the form and the paint state, and name the effects the
//! browser glue must carry out, as the React app's handlers do.

use form_core::Field;

use crate::form::{FormState, Status, Submission};
use crate::grpc_web::CallError;
use crate::overlay::SUCCESS_TITLE;
use crate::proto::{SignUpRequest, SignUpResponse};
use crate::scene::{Control, Interaction};

#[derive(Debug, PartialEq)]
pub enum Event {
    /// A text input's value changed.
    Input(Field, String),
    /// The terms checkbox was toggled.
    Toggle(bool),
    /// A control gained focus; `visible` is whether `:focus-visible` matches.
    Focus {
        control: Control,
        visible: bool,
    },
    Blur(Control),
    /// The pointer entered or left the submit button.
    Hover(bool),
    /// The focused input's selection (in characters) or scroll changed.
    Caret {
        selection: (usize, usize),
        scroll_left: f64,
    },
    /// The caret blink timer fired.
    Blink,
    /// The viewport changed size, so only the layout needs updating.
    Resize,
    Submit,
    Response(Result<SignUpResponse, CallError>),
}

impl Event {
    /// Whether the event shows the caret, which restarts its blinking as
    /// Chrome does when the caret moves.
    #[must_use]
    pub fn shows_caret(&self) -> bool {
        matches!(
            self,
            Self::Input(..) | Self::Focus { .. } | Self::Caret { .. }
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Move focus to the overlay element with this id.
    Focus(&'static str),
    /// Send the request, then report the outcome as [`Event::Response`].
    Send(SignUpRequest),
}

#[derive(Debug, Default)]
pub struct App {
    pub state: FormState,
    pub ui: Interaction,
}

impl App {
    /// Applies `event` and returns what the browser must do next, if anything.
    pub fn handle(&mut self, event: Event) -> Option<Effect> {
        if event.shows_caret() {
            self.ui.caret_visible = true;
        }
        match event {
            Event::Input(field, value) => {
                self.state.set_text(field, &value);
            }
            Event::Toggle(checked) => {
                self.state.set_accept_terms(checked);
            }
            Event::Focus { control, visible } => {
                self.ui.focus = Some(control);
                self.ui.focus_visible = visible;
            }
            Event::Blur(control) => {
                if let Control::Field(field) = control {
                    self.state.blur(field);
                }
                if self.ui.focus == Some(control) {
                    self.ui.focus = None;
                }
            }
            Event::Hover(hovered) => self.ui.hovered = hovered,
            Event::Caret {
                selection,
                scroll_left,
            } => {
                self.ui.selection = selection;
                self.ui.scroll_left = scroll_left;
            }
            Event::Blink => self.ui.caret_visible = !self.ui.caret_visible,
            Event::Resize => {}
            // The disabled submit button blocks submission while pending.
            Event::Submit if self.state.locked() => {}
            Event::Submit => {
                return Some(match self.state.begin_submit() {
                    Submission::Invalid(field) => Effect::Focus(field.key()),
                    Submission::Send(request) => Effect::Send(request),
                });
            }
            Event::Response(result) => {
                let field = self.state.finish_submit(result);
                if self.state.status() == Status::Success {
                    return Some(Effect::Focus(SUCCESS_TITLE));
                }
                return field.map(|field| Effect::Focus(field.key()));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto;

    fn filled() -> App {
        let mut app = App::default();
        for (field, value) in [
            (Field::FullName, "Ada Lovelace"),
            (Field::Email, "ada@example.com"),
            (Field::Password, "Analytical#Engine1"),
            (Field::ConfirmPassword, "Analytical#Engine1"),
        ] {
            assert_eq!(app.handle(Event::Input(field, value.to_owned())), None);
        }
        app.handle(Event::Toggle(true));
        app
    }

    #[test]
    fn invalid_submissions_focus_the_first_problem() {
        let mut app = App::default();
        assert_eq!(app.handle(Event::Submit), Some(Effect::Focus("fullName")));
        assert!(app.state.error(Field::Email).is_some());
    }

    #[test]
    fn valid_submissions_send_once_and_focus_the_success_title() {
        let mut app = filled();
        let Some(Effect::Send(request)) = app.handle(Event::Submit) else {
            panic!("expected a request");
        };
        assert_eq!(request.full_name, "Ada Lovelace");
        assert!(app.state.locked());
        assert_eq!(app.handle(Event::Submit), None);
        assert_eq!(
            app.handle(Event::Response(Ok(SignUpResponse::default()))),
            Some(Effect::Focus(SUCCESS_TITLE))
        );
        assert_eq!(app.state.status(), Status::Success);
    }

    #[test]
    fn server_violations_focus_their_field_and_failures_focus_nothing() {
        let mut app = filled();
        app.handle(Event::Submit);
        let response = SignUpResponse {
            account_id: String::new(),
            violations: vec![proto::FieldViolation {
                field: proto::Field::Email.into(),
                code: proto::ViolationCode::InvalidFormat.into(),
            }],
        };
        assert_eq!(
            app.handle(Event::Response(Ok(response))),
            Some(Effect::Focus("email"))
        );
        app.handle(Event::Submit);
        assert_eq!(app.handle(Event::Response(Err(CallError::Http(503)))), None);
        assert_eq!(app.state.status(), Status::Failed);
    }

    #[test]
    fn tracks_focus_hover_and_the_caret() {
        let mut app = App::default();
        let email = Control::Field(Field::Email);
        app.handle(Event::Focus {
            control: email,
            visible: true,
        });
        assert_eq!(app.ui.focus, Some(email));
        assert!(app.ui.focus_visible && app.ui.caret_visible);
        app.handle(Event::Blink);
        assert!(!app.ui.caret_visible);
        app.handle(Event::Caret {
            selection: (1, 3),
            scroll_left: 4.0,
        });
        assert_eq!((app.ui.selection, app.ui.scroll_left), ((1, 3), 4.0));
        assert!(app.ui.caret_visible);
        assert!(!Event::Blink.shows_caret() && !Event::Hover(true).shows_caret());

        // A late blur from a control that already lost focus keeps the new one.
        app.handle(Event::Focus {
            control: Control::Button,
            visible: false,
        });
        app.handle(Event::Blur(email));
        assert_eq!(app.ui.focus, Some(Control::Button));
        assert!(app.state.error(Field::Email).is_some());
        app.handle(Event::Blur(Control::Button));
        assert_eq!(app.ui.focus, None);

        app.handle(Event::Hover(true));
        assert!(app.ui.hovered);
        assert_eq!(app.handle(Event::Resize), None);
    }
}
