//! Input on the canvas, independent of the browser. The canvas is the only
//! element that takes focus; keys and the clipboard move
//! focus between the controls it paints, edit the focused input and submit
//! the form, as Chrome's native controls do in the React app.

use std::collections::HashMap;

use form_core::Field;

use crate::editing::{Editor, InputText, Motion, content_box, single_line};
use crate::form::{FormState, Status, Submission};
use crate::grpc_web::CallError;
use crate::layout::{Content, Layout, Rect};
use crate::proto::{SignUpRequest, SignUpResponse};
use crate::scene::{Control, Interaction};
use crate::text::Measure;

/// A key press, as a `KeyboardEvent` describes it.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the modifier keys of a KeyboardEvent"
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    /// The canvas gained focus.
    Focus,
    /// The canvas lost focus, to the browser or another window.
    Blur,
    Key(Key),
    /// A key came up. The page also sees the release of a key pressed in the
    /// browser's own controls, such as the Tab that brought focus in.
    KeyUp(Key),
    Copy,
    Cut,
    Paste(String),
    /// The caret blink timer fired.
    Blink,
    /// The viewport changed size, so only the layout needs updating.
    Resize,
    Response(Result<SignUpResponse, CallError>),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Give the canvas focus, as focusing a control would.
    Focus,
    /// Put this text on the clipboard.
    Clipboard(String),
    /// Send the request, then report the outcome as [`Event::Response`].
    Send(SignUpRequest),
}

/// What the browser must do after an event.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Whether the app used the event, so the browser's own action, such as
    /// scrolling for Space or leaving the page for Tab, must not run.
    pub handled: bool,
    /// Whether the caret was shown, so its blinking starts over.
    pub blink: bool,
    pub effect: Option<Effect>,
}

impl Outcome {
    const fn handled() -> Self {
        Self {
            handled: true,
            blink: false,
            effect: None,
        }
    }
}

/// What the device the page runs on can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// Whether this is macOS, whose shortcuts use Option and Command.
    pub mac: bool,
}

/// Where the page's focus is, as the canvas holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Canvas {
    /// Outside the page, in the browser's own controls, or never in it yet.
    #[default]
    Outside,
    /// On the canvas, in a window in the background.
    Background,
    /// On the canvas, which has focus.
    Focused,
}

/// How focus came to the canvas without a click. The browser does not say
/// which way Tab brought it in from the browser's own controls; the release
/// of the key that did tells, and `shift` whether Shift came up before it.
#[derive(Debug)]
enum Entering {
    /// Into the page, to its first control, with the inputs' editors as they
    /// were before.
    First {
        editors: HashMap<Field, Editor>,
        shift: bool,
    },
    /// Back to the control that had it, as when the window regains focus.
    Back { shift: bool },
}

#[derive(Debug, Default)]
pub struct App {
    pub state: FormState,
    device: Device,
    /// The focused control. It keeps focus while the canvas has none, as an
    /// element does while its window is in the background.
    focus: Option<Control>,
    /// The control Tab continues from while none has focus, as from Chrome's
    /// sequential focus navigation starting point: the button, after being
    /// disabled took its focus.
    resume: Option<Control>,
    /// The checkbox or button that Space went down on, which its release
    /// activates if it still has focus, as in Chrome.
    space: Option<Control>,
    canvas: Canvas,
    entering: Option<Entering>,
    /// Whether the browser would match `:focus-visible`: after keyboard use.
    focus_visible: bool,
    caret_visible: bool,
    editors: HashMap<Field, Editor>,
}

impl App {
    #[must_use]
    pub fn new(device: Device) -> Self {
        Self {
            device,
            ..Self::default()
        }
    }

    /// Applies `event` to the app as laid out in `page`.
    pub fn handle(&mut self, event: Event, page: &Layout, measure: &impl Measure) -> Outcome {
        let mut outcome = match event {
            Event::Focus => {
                self.entering = None;
                // Tab or a script bring focus into the page at its first
                // control, and the window regaining focus brings it back.
                if self.canvas != Canvas::Outside || self.focus.is_some() {
                    self.entering = Some(Entering::Back { shift: false });
                } else {
                    self.focus_visible = true;
                    let editors = self.editors.clone();
                    self.tab(page, measure, false);
                    self.entering = Some(Entering::First {
                        editors,
                        shift: false,
                    });
                }
                self.canvas = Canvas::Focused;
                // The caret shows at once.
                Outcome {
                    blink: true,
                    ..Outcome::default()
                }
            }
            Event::Blur => {
                if self.canvas == Canvas::Focused {
                    self.canvas = Canvas::Background;
                }
                self.entering = None;
                self.space = None;
                if let Some(Control::Field(field)) = self.focus {
                    self.state.blur(field);
                }
                Outcome::default()
            }
            Event::Key(key) => self.key(&key, page, measure),
            Event::KeyUp(key) => self.key_up(&key, page, measure),
            Event::Copy => self.copy(false, page, measure),
            Event::Cut => self.copy(true, page, measure),
            Event::Paste(text) => match self.editing() {
                Some(field) => self.edit(field, page, measure, |editor, text_now| {
                    Some(editor.replace(text_now, &single_line(&text)))
                }),
                None => Outcome::default(),
            },
            Event::Blink => {
                self.caret_visible = !self.caret_visible;
                Outcome::default()
            }
            Event::Resize => Outcome::default(),
            Event::Response(result) => self.respond(result, page, measure),
        };
        if outcome.blink {
            self.caret_visible = true;
        }
        if self.focus.is_none() {
            outcome.blink = false;
        }
        outcome
    }

    /// What painting needs to know.
    #[must_use]
    pub fn interaction(&self) -> Interaction {
        let focus = self.focus.filter(|_| self.canvas == Canvas::Focused);
        let editor = self
            .editing()
            .map(|field| self.editor(field))
            .unwrap_or_default();
        let range = editor.range();
        Interaction {
            focus,
            focus_visible: self.focus_visible,
            hovered: false,
            caret_visible: self.caret_visible,
            scroll_left: editor.scroll,
            selection: (range.start, range.end),
        }
    }

    /// The text input being edited, if the canvas has focus in one.
    fn editing(&self) -> Option<Field> {
        match self.focus.filter(|_| self.canvas == Canvas::Focused)? {
            Control::Field(Field::AcceptTerms) | Control::Button => None,
            Control::Field(field) => Some(field),
        }
    }

    fn editor(&self, field: Field) -> Editor {
        self.editors.get(&field).copied().unwrap_or_default()
    }

    /// The focusable controls in Tab order, with their boxes.
    fn controls(&self, page: &Layout) -> Vec<(Control, Rect)> {
        let Content::Form(form) = &page.content else {
            return Vec::new();
        };
        let mut all: Vec<(Control, Rect)> = form
            .fields
            .iter()
            .map(|field| (Control::Field(field.field), field.input))
            .collect();
        all.push((Control::Field(Field::AcceptTerms), form.terms.check));
        // A disabled button cannot take focus.
        if self.state.status() != Status::Submitting {
            all.push((Control::Button, form.button));
        }
        all
    }

    /// Moves focus, and blurs the control that had it as React's `onBlur`
    /// sees it: an input shows its error and scrolls back to its start.
    fn focus_on(&mut self, control: Option<Control>) {
        self.resume = None;
        if self.focus == control {
            return;
        }
        self.space = None;
        if let Some(Control::Field(field)) = self.focus {
            self.state.blur(field);
            self.editors.entry(field).or_default().scroll = 0.0;
        }
        self.focus = control;
    }

    /// Focuses a text input the way a script or its label does: its
    /// selection stays as it was, scrolled into view.
    fn focus_field(&mut self, field: Field, page: &Layout, measure: &impl Measure) {
        self.focus_on(Some(Control::Field(field)));
        let text = InputText::new(field, self.state.text(field), measure);
        let mut editor = self.editor(field);
        editor.reveal(&text, content_width(page, field));
        self.editors.insert(field, editor);
    }

    /// Moves focus to the next or previous control; returns whether there
    /// is one, or focus leaves the page.
    fn tab(&mut self, page: &Layout, measure: &impl Measure, backward: bool) -> bool {
        let controls = self.controls(page);
        let current = self
            .focus
            .or(self.resume.take())
            .and_then(|focus| controls.iter().position(|&(control, _)| control == focus));
        let next = match current {
            Some(index) if backward => index.checked_sub(1),
            Some(index) => Some(index + 1),
            None if backward => controls.len().checked_sub(1),
            None => Some(0),
        };
        let Some(&(control, _)) = next.and_then(|index| controls.get(index)) else {
            self.focus_on(None);
            self.canvas = Canvas::Outside;
            return false;
        };
        self.focus_on(Some(control));
        if self.canvas == Canvas::Outside {
            self.canvas = Canvas::Focused;
        }
        // Tabbing into a text input selects its value.
        if let Control::Field(field) = control {
            let text = InputText::new(field, self.state.text(field), measure);
            let editor = self.editors.entry(field).or_default();
            editor.select(0..text.len());
            editor.scroll = 0.0;
        }
        true
    }

    fn key(&mut self, key: &Key, page: &Layout, measure: &impl Measure) -> Outcome {
        self.entering = None;
        // Keyboard use makes the focused control match `:focus-visible`.
        self.focus_visible = true;
        if key.key == "Tab" && !(key.ctrl || key.alt || key.meta) {
            let handled = self.tab(page, measure, key.shift);
            return Outcome {
                handled,
                blink: handled,
                effect: None,
            };
        }
        let enter = key.key == "Enter";
        let space = key.key == " ";
        match self.focus.filter(|_| self.canvas == Canvas::Focused) {
            // Space activates the checkbox and the button when it comes up.
            Some(control @ (Control::Field(Field::AcceptTerms) | Control::Button)) if space => {
                self.space = Some(control);
                Outcome::handled()
            }
            // Enter submits from any field and presses the button, again as
            // it repeats.
            Some(Control::Field(_) | Control::Button) if enter => self.submit(page, measure),
            Some(Control::Field(field)) if field != Field::AcceptTerms => {
                self.edit_key(field, key, page, measure)
            }
            _ => Outcome::default(),
        }
    }

    /// Learns from the release of Tab which way it brought focus in from the
    /// browser's own controls: Shift+Tab enters at the last control, and
    /// either leaves the control the page had focused.
    fn key_up(&mut self, key: &Key, page: &Layout, measure: &impl Measure) -> Outcome {
        match key.key.as_str() {
            " " => {
                let focused = self.focus.filter(|_| self.canvas == Canvas::Focused);
                return match self
                    .space
                    .take()
                    .filter(|&control| Some(control) == focused)
                {
                    Some(Control::Button) => self.submit(page, measure),
                    Some(_) => {
                        self.state
                            .set_accept_terms(!self.state.values().accept_terms);
                        Outcome::handled()
                    }
                    None => Outcome::default(),
                };
            }
            "Shift" => {
                if let Some(Entering::First { shift, .. } | Entering::Back { shift }) =
                    &mut self.entering
                {
                    *shift = true;
                }
                return Outcome::default();
            }
            "Tab" if !(key.ctrl || key.alt || key.meta) => {}
            // A browser shortcut, such as Ctrl+Tab, brought focus in.
            "Tab" | "Control" | "Alt" | "Meta" => {
                self.entering = None;
                return Outcome::default();
            }
            _ => return Outcome::default(),
        }
        let backward = key.shift
            || matches!(
                self.entering,
                Some(Entering::First { shift: true, .. } | Entering::Back { shift: true })
            );
        match self.entering.take() {
            Some(Entering::First { editors, .. }) if backward => {
                // The first control never really had focus.
                self.focus = None;
                self.editors = editors;
            }
            Some(Entering::Back { .. }) => self.focus_on(None),
            _ => return Outcome::default(),
        }
        self.focus_visible = true;
        let blink = self.tab(page, measure, backward);
        Outcome {
            handled: false,
            blink,
            effect: None,
        }
    }

    fn edit_key(
        &mut self,
        field: Field,
        key: &Key,
        page: &Layout,
        measure: &impl Measure,
    ) -> Outcome {
        // Outside macOS, Alt and the Windows key make the browser's and the
        // system's shortcuts, such as Alt+ArrowLeft to go back, and
        // Shift+Delete is the browser's cut, which comes back as a cut event.
        let cut = key.shift && key.key == "Delete";
        if !self.device.mac && (key.meta || (key.alt && !key.ctrl) || cut) {
            return Outcome::default();
        }
        let (word, command) = if self.device.mac {
            (key.alt, key.meta)
        } else {
            (key.ctrl, key.ctrl)
        };
        let line = self.device.mac && key.meta;
        let motion = match key.key.as_str() {
            "ArrowLeft" if line => Some(Motion::LineStart),
            "ArrowRight" if line => Some(Motion::LineEnd),
            "ArrowLeft" if word => Some(Motion::WordLeft),
            "ArrowRight" if word => Some(Motion::WordRight),
            "ArrowLeft" => Some(Motion::Left),
            "ArrowRight" => Some(Motion::Right),
            "Home" => Some(Motion::LineStart),
            "End" => Some(Motion::LineEnd),
            "ArrowUp" => Some(Motion::Start),
            "ArrowDown" => Some(Motion::End),
            _ => None,
        };
        if let Some(motion) = motion {
            return self.edit(field, page, measure, |editor, text| {
                editor.go(text, motion, key.shift);
                None
            });
        }
        match key.key.as_str() {
            "a" | "A" if command && !key.alt => {
                let text = InputText::new(field, self.state.text(field), measure);
                self.editors.entry(field).or_default().select(0..text.len());
                Outcome::handled()
            }
            "Backspace" | "Delete" => {
                let forward = key.key == "Delete";
                self.edit(field, page, measure, |editor, text| {
                    // Command deletes to the start, or the end, of the line.
                    if line && editor.range().is_empty() {
                        let motion = if forward {
                            Motion::LineEnd
                        } else {
                            Motion::LineStart
                        };
                        editor.go(text, motion, true);
                    }
                    editor.delete(text, forward, word)
                })
            }
            typed if self.inserts(key) => self.edit(field, page, measure, |editor, text| {
                Some(editor.replace(text, typed))
            }),
            _ => Outcome::default(),
        }
    }

    /// Whether `key` types its character rather than a shortcut: Ctrl and
    /// Command make shortcuts, and so does Alt outside macOS, unless Ctrl
    /// and Alt together stand for `AltGr`.
    fn inserts(&self, key: &Key) -> bool {
        let printable =
            key.key.chars().count() == 1 || !key.key.chars().all(|c| c.is_ascii_alphanumeric());
        let shortcut = key.meta || (key.ctrl != key.alt && (key.ctrl || !self.device.mac));
        printable && !shortcut
    }

    /// Edits the focused input with `change`, which may move its selection
    /// and returns its new value, if any; then scrolls the caret into view.
    fn edit(
        &mut self,
        field: Field,
        page: &Layout,
        measure: &impl Measure,
        change: impl FnOnce(&mut Editor, &InputText) -> Option<String>,
    ) -> Outcome {
        let mut text = InputText::new(field, self.state.text(field), measure);
        let mut editor = self.editor(field);
        if let Some(value) = change(&mut editor, &text) {
            // A read-only input, while the form submits, keeps its value.
            if !self.state.set_text(field, &value) {
                return Outcome::handled();
            }
            text = InputText::new(field, &value, measure);
        }
        editor.reveal(&text, content_width(page, field));
        self.editors.insert(field, editor);
        Outcome {
            handled: true,
            blink: true,
            effect: None,
        }
    }

    fn submit(&mut self, page: &Layout, measure: &impl Measure) -> Outcome {
        // The disabled button blocks submission while one is pending.
        if self.state.locked() {
            return Outcome::handled();
        }
        let effect = match self.state.begin_submit() {
            Submission::Invalid(field) => {
                self.focus_field(field, page, measure);
                Effect::Focus
            }
            Submission::Send(request) => {
                // Disabled while the request is pending, the button loses
                // focus to the page, as Chrome's focus fixup takes it.
                if self.focus == Some(Control::Button) {
                    self.focus_on(None);
                    self.resume = Some(Control::Button);
                }
                Effect::Send(request)
            }
        };
        Outcome {
            handled: true,
            blink: true,
            effect: Some(effect),
        }
    }

    fn respond(
        &mut self,
        result: Result<SignUpResponse, CallError>,
        page: &Layout,
        measure: &impl Measure,
    ) -> Outcome {
        let field = self.state.finish_submit(result);
        if self.state.status() == Status::Success {
            // The success panel has nothing to focus but its heading.
            self.focus_on(None);
            return Outcome::default();
        }
        let Some(field) = field else {
            return Outcome::default();
        };
        self.focus_field(field, page, measure);
        Outcome {
            handled: false,
            blink: true,
            effect: Some(Effect::Focus),
        }
    }

    /// Copies or cuts the focused input's selection; Chrome never copies a
    /// password.
    fn copy(&mut self, cut: bool, page: &Layout, measure: &impl Measure) -> Outcome {
        let Some(field) = self.editing() else {
            return Outcome::default();
        };
        let text = InputText::new(field, self.state.text(field), measure);
        let selected = self.editor(field).selected(&text).to_owned();
        if text.masked() || selected.is_empty() {
            return Outcome::default();
        }
        let mut outcome = if cut {
            self.edit(field, page, measure, |editor, text| {
                editor.delete(text, false, false)
            })
        } else {
            Outcome::handled()
        };
        outcome.effect = Some(Effect::Clipboard(selected));
        outcome
    }
}

fn input_of(page: &Layout, field: Field) -> Rect {
    match &page.content {
        Content::Form(form) => form
            .fields
            .iter()
            .find(|layout| layout.field == field)
            .map_or_else(Rect::default, |layout| layout.input),
        Content::Success(_) => Rect::default(),
    }
}

fn content_width(page: &Layout, field: Field) -> f64 {
    content_box(input_of(page, field)).width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Viewport, layout};
    use crate::proto;
    use crate::text::testing::FixedMeasure;

    const DESKTOP: Viewport = Viewport {
        width: 1280.0,
        height: 900.0,
        media_width: 1280.0,
    };

    fn page(app: &App) -> Layout {
        layout(DESKTOP, &app.state, &FixedMeasure)
    }

    fn send(app: &mut App, event: Event) -> Outcome {
        let page = page(app);
        app.handle(event, &page, &FixedMeasure)
    }

    fn press(app: &mut App, key: &str, modify: impl FnOnce(&mut Key)) -> Outcome {
        let mut event = Key {
            key: key.to_owned(),
            ..Key::default()
        };
        modify(&mut event);
        send(app, Event::Key(event))
    }

    fn key(app: &mut App, key: &str) -> Outcome {
        press(app, key, |_| {})
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            assert!(key(app, &c.to_string()).handled);
        }
    }

    fn focus(app: &App) -> Option<Control> {
        app.interaction().focus
    }

    fn selection(app: &App) -> (usize, usize) {
        app.interaction().selection
    }

    /// An app whose canvas has focus in the full name input.
    fn focused() -> App {
        let mut app = App::new(Device::default());
        send(&mut app, Event::Focus);
        app
    }

    /// Fills the form in with a valid sign-up.
    fn fill(app: &mut App) {
        for (field, value) in [
            (Field::FullName, "Ada Lovelace"),
            (Field::Email, "ada@example.com"),
            (Field::Password, "Analytical#Engine1"),
            (Field::ConfirmPassword, "Analytical#Engine1"),
        ] {
            app.state.set_text(field, value);
        }
        app.state.set_accept_terms(true);
    }

    fn email_violation() -> SignUpResponse {
        SignUpResponse {
            account_id: String::new(),
            violations: vec![proto::FieldViolation {
                field: proto::Field::Email.into(),
                code: proto::ViolationCode::InvalidFormat.into(),
            }],
        }
    }

    #[test]
    fn tab_moves_through_the_controls_and_out_of_the_page() {
        let mut app = focused();
        assert_eq!(focus(&app), Some(Control::Field(Field::FullName)));
        assert!(app.interaction().focus_visible);
        for expected in [
            Control::Field(Field::Email),
            Control::Field(Field::Password),
            Control::Field(Field::ConfirmPassword),
            Control::Field(Field::AcceptTerms),
            Control::Button,
        ] {
            assert!(key(&mut app, "Tab").handled);
            assert_eq!(focus(&app), Some(expected));
        }
        // Past the last control, the browser takes focus out of the page.
        assert!(!key(&mut app, "Tab").handled);
        assert_eq!(focus(&app), None);
        assert!(!press(&mut app, "Tab", |key| key.ctrl = true).handled);
        assert!(press(&mut app, "Tab", |key| key.shift = true).handled);
        assert_eq!(focus(&app), Some(Control::Button));
        // Another window takes focus, then gives it back.
        send(&mut app, Event::Blur);
        assert_eq!(focus(&app), None);
        send(&mut app, Event::Focus);
        assert_eq!(focus(&app), Some(Control::Button));
        assert_eq!(key(&mut app, "x"), Outcome::default());
    }

    fn tab_up(shift: bool) -> Event {
        Event::KeyUp(Key {
            key: "Tab".to_owned(),
            shift,
            ..Key::default()
        })
    }

    #[test]
    fn tab_from_the_browser_enters_the_page_from_its_side() {
        let mut app = App::new(Device::default());
        app.state.set_text(Field::FullName, "Ada");
        // Focus comes in at the first control, until the key that brought it
        // comes up: Shift+Tab enters at the last.
        send(&mut app, Event::Focus);
        assert_eq!(selection(&app), (0, 3));
        assert!(send(&mut app, tab_up(true)).blink);
        assert_eq!(focus(&app), Some(Control::Button));
        // The first input never really had focus.
        assert_eq!(app.state.error(Field::FullName), None);
        assert_eq!(app.editor(Field::FullName), Editor::default());
        // The release of a key pressed in the page changes nothing.
        assert!(press(&mut app, "Tab", |key| key.shift = true).handled);
        assert_eq!(send(&mut app, tab_up(true)), Outcome::default());
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        // Focus comes back where it was with the window, but Tab from the
        // browser's controls brings it to the first control.
        send(&mut app, Event::Blur);
        send(&mut app, Event::Focus);
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        send(&mut app, Event::Blur);
        send(&mut app, Event::Focus);
        send(
            &mut app,
            Event::KeyUp(Key {
                key: "Tab".to_owned(),
                alt: true,
                ..Key::default()
            }),
        );
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        send(&mut app, Event::Focus);
        send(&mut app, tab_up(false));
        assert_eq!(focus(&app), Some(Control::Field(Field::FullName)));
        // Shift may come up before the Tab, but another modifier makes a
        // browser shortcut.
        let up = |key: &str| {
            Event::KeyUp(Key {
                key: key.to_owned(),
                ..Key::default()
            })
        };
        send(&mut app, Event::Blur);
        send(&mut app, Event::Focus);
        send(&mut app, up("a"));
        send(&mut app, up("Shift"));
        send(&mut app, tab_up(false));
        assert_eq!(focus(&app), Some(Control::Button));
        send(&mut app, Event::Blur);
        send(&mut app, Event::Focus);
        send(&mut app, up("Control"));
        send(&mut app, tab_up(false));
        assert_eq!(focus(&app), Some(Control::Button));
    }

    #[test]
    fn typing_edits_the_focused_input() {
        let mut app = focused();
        type_text(&mut app, "Ada Byron");
        press(&mut app, "Backspace", |key| key.ctrl = true);
        type_text(&mut app, "Lovelace");
        assert_eq!(app.state.text(Field::FullName), "Ada Lovelace");
        key(&mut app, "Home");
        press(&mut app, "ArrowRight", |key| key.ctrl = true);
        key(&mut app, "Delete");
        press(&mut app, "End", |key| key.shift = true);
        assert_eq!(selection(&app), (3, 11));
        // Shortcuts are left to the browser; AltGr types.
        assert_eq!(
            press(&mut app, "r", |key| key.ctrl = true),
            Outcome::default()
        );
        assert!(!press(&mut app, "f", |key| key.alt = true).handled);
        assert!(!press(&mut app, "ArrowLeft", |key| key.alt = true).handled);
        assert!(!press(&mut app, "ArrowRight", |key| key.meta = true).handled);
        assert!(!press(&mut app, "Delete", |key| key.shift = true).handled);
        assert_eq!(selection(&app), (3, 11));
        assert!(!key(&mut app, "Dead").handled);
        press(&mut app, "@", |key| {
            key.ctrl = true;
            key.alt = true;
        });
        assert_eq!(app.state.text(Field::FullName), "Ada@");
        press(&mut app, "a", |key| key.ctrl = true);
        assert_eq!(selection(&app), (0, 4));
        key(&mut app, "ArrowDown");
        key(&mut app, "ArrowLeft");
        key(&mut app, "ArrowUp");
        assert_eq!(selection(&app), (0, 0));
        // Enter submits; the invalid email takes focus.
        let submitted = key(&mut app, "Enter");
        assert_eq!(submitted.effect, Some(Effect::Focus));
        assert_eq!(focus(&app), Some(Control::Field(Field::FullName)));
        assert!(app.state.error(Field::Email).is_some());
    }

    #[test]
    fn macos_moves_with_option_and_command() {
        let mut app = App::new(Device { mac: true });
        send(&mut app, Event::Focus);
        type_text(&mut app, "ab cd");
        press(&mut app, "é", |key| key.alt = true);
        assert_eq!(app.state.text(Field::FullName), "ab cdé");
        press(&mut app, "ArrowLeft", |key| key.alt = true);
        assert_eq!(selection(&app), (3, 3));
        press(&mut app, "ArrowLeft", |key| key.meta = true);
        assert_eq!(selection(&app), (0, 0));
        press(&mut app, "ArrowRight", |key| key.meta = true);
        assert_eq!(selection(&app), (6, 6));
        assert!(!press(&mut app, "a", |key| key.ctrl = true).handled);
        press(&mut app, "a", |key| key.meta = true);
        assert_eq!(selection(&app), (0, 6));
        // Command+Backspace and Command+Delete delete to the line's edges.
        key(&mut app, "ArrowLeft");
        press(&mut app, "ArrowRight", |key| key.alt = true);
        press(&mut app, "Delete", |key| key.meta = true);
        assert_eq!(app.state.text(Field::FullName), "ab");
        key(&mut app, "ArrowLeft");
        press(&mut app, "Backspace", |key| key.meta = true);
        assert_eq!(app.state.text(Field::FullName), "b");
    }

    #[test]
    fn tab_continues_from_the_button_after_a_request() {
        let mut app = focused();
        fill(&mut app);
        for _ in 0..5 {
            key(&mut app, "Tab");
        }
        key(&mut app, "Enter");
        send(&mut app, Event::Response(Err(CallError::Http(503))));
        assert_eq!(focus(&app), None);
        // Shift+Tab goes back from the button, and Tab from it leaves.
        press(&mut app, "Tab", |key| key.shift = true);
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        key(&mut app, "Tab");
        key(&mut app, "Enter");
        send(&mut app, Event::Response(Err(CallError::Http(503))));
        assert!(!key(&mut app, "Tab").handled);
        assert_eq!(focus(&app), None);
    }

    fn space_up() -> Event {
        Event::KeyUp(Key {
            key: " ".to_owned(),
            ..Key::default()
        })
    }

    #[test]
    fn the_checkbox_toggles_with_space_and_enter_submits() {
        let mut app = focused();
        for _ in 0..4 {
            key(&mut app, "Tab");
        }
        // Space toggles it once it comes up, however long it repeats.
        assert!(key(&mut app, " ").handled);
        assert!(key(&mut app, " ").handled);
        assert!(!app.state.values().accept_terms);
        key(&mut app, "a");
        assert!(send(&mut app, space_up()).handled);
        assert!(app.state.values().accept_terms);
        // Not if focus moved away meanwhile.
        key(&mut app, " ");
        key(&mut app, "Tab");
        press(&mut app, "Tab", |key| key.shift = true);
        assert_eq!(send(&mut app, space_up()), Outcome::default());
        assert!(app.state.values().accept_terms);
        assert!(key(&mut app, "Enter").handled);
        assert_eq!(focus(&app), Some(Control::Field(Field::FullName)));
        assert_eq!(app.state.error(Field::AcceptTerms), None);
    }

    #[test]
    fn the_keyboard_submits_and_waits_for_the_backend() {
        let mut app = focused();
        fill(&mut app);
        for _ in 0..5 {
            key(&mut app, "Tab");
        }
        assert_eq!(key(&mut app, " ").effect, None);
        let Some(Effect::Send(request)) = send(&mut app, space_up()).effect else {
            panic!("expected a request");
        };
        assert_eq!(request.email, "ada@example.com");
        // The disabled button loses focus, so Tab starts over.
        assert_eq!(focus(&app), None);
        // Locked: no second request and no typing, and the disabled button
        // leaves the Tab order.
        assert_eq!(key(&mut app, "Enter").effect, None);
        press(&mut app, "Tab", |key| key.shift = true);
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        press(&mut app, "Tab", |key| key.shift = true);
        type_text(&mut app, "x");
        assert_eq!(app.state.text(Field::ConfirmPassword), "Analytical#Engine1");

        let outcome = send(&mut app, Event::Response(Ok(email_violation())));
        assert_eq!(outcome.effect, Some(Effect::Focus));
        assert_eq!(focus(&app), Some(Control::Field(Field::Email)));
        assert!(matches!(
            key(&mut app, "Enter").effect,
            Some(Effect::Send(_))
        ));
        send(&mut app, Event::Response(Ok(SignUpResponse::default())));
        assert_eq!(app.state.status(), Status::Success);
        assert_eq!(focus(&app), None);
    }

    #[test]
    fn the_clipboard_copies_cuts_and_pastes() {
        let mut app = focused();
        assert_eq!(send(&mut app, Event::Copy), Outcome::default());
        type_text(&mut app, "Ada");
        press(&mut app, "Home", |key| key.shift = true);
        let copied = send(&mut app, Event::Copy);
        assert_eq!(copied.effect, Some(Effect::Clipboard("Ada".to_owned())));
        let cut = send(&mut app, Event::Cut);
        assert_eq!(cut.effect, Some(Effect::Clipboard("Ada".to_owned())));
        assert_eq!(app.state.text(Field::FullName), "");
        send(&mut app, Event::Paste("Ada\r\nLovelace".to_owned()));
        assert_eq!(app.state.text(Field::FullName), "Ada Lovelace");
        // Chrome never copies a password.
        key(&mut app, "Tab");
        key(&mut app, "Tab");
        type_text(&mut app, "secret");
        press(&mut app, "a", |key| key.ctrl = true);
        assert_eq!(send(&mut app, Event::Copy), Outcome::default());
        key(&mut app, "Tab");
        key(&mut app, "Tab");
        assert_eq!(
            send(&mut app, Event::Paste("x".to_owned())),
            Outcome::default()
        );
    }

    #[test]
    fn the_caret_blinks_in_the_focused_input() {
        let mut app = focused();
        assert!(key(&mut app, "a").blink);
        assert!(app.interaction().caret_visible);
        send(&mut app, Event::Blink);
        assert!(!app.interaction().caret_visible);
        assert_eq!(send(&mut app, Event::Resize), Outcome::default());
        // It shows again at once when the window regains focus.
        send(&mut app, Event::Blur);
        assert!(send(&mut app, Event::Focus).blink);
        assert!(app.interaction().caret_visible);
        // Out of the page, no caret blinks.
        for _ in 0..5 {
            assert!(key(&mut app, "Tab").blink);
        }
        assert!(!key(&mut app, "Tab").blink);
    }
}
