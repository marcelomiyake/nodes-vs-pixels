//! Input on the canvas, independent of the browser. The canvas is the only
//! element that takes focus; keys, mouse buttons and the clipboard move
//! focus between the controls it paints, edit the focused input and submit
//! the form, as Chrome's native controls do in the React app.

use std::collections::HashMap;

use form_core::Field;

use crate::editing::{Editor, InputText, Motion, content_box, single_line};
use crate::form::{FormState, Status, Submission};
use crate::grpc_web::CallError;
use crate::history::{Edit, History, Snapshot};
use crate::layout::{Content, Layout, Rect, TEXT_FIELDS};
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
    /// The main mouse button went down at a point of the page; `clicks`
    /// counts the clicks of a double or triple click.
    Press {
        x: f64,
        y: f64,
        shift: bool,
        clicks: u32,
    },
    /// The mouse button came up at a point of the page.
    Release {
        x: f64,
        y: f64,
    },
    /// The pointer moved to a point of the page, or left it.
    Pointer(Option<(f64, f64)>),
    /// The mouse wheel turned, or a trackpad swiped, `dx` and `dy` pixels
    /// over a point of the page.
    Wheel {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
        shift: bool,
    },
    Copy,
    Cut,
    Paste(String),
    /// The caret blink timer fired.
    Blink,
    /// An animation frame came while [`App::autoscrolling`].
    Autoscroll,
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
    /// Scroll the page to the focused input's caret, as Chrome does once a
    /// key edits the input or moves its selection.
    Reveal,
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

/// How far either side of the pointer a selection drag scrolls its input
/// into view, on each animation frame, as Chrome's autoscroll does.
const AUTOSCROLL_MARGIN: f64 = 20.0;

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Input(Field),
    /// A field's label; the terms label toggles the checkbox.
    Label(Field),
    Check,
    Button,
}

/// What the device the page runs on can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// Whether this is macOS, whose shortcuts use Option and Command.
    pub mac: bool,
    /// Whether the pointer can hover, so the button shows it.
    pub hover: bool,
}

/// The main mouse button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mouse {
    #[default]
    Up,
    /// Down on a target, or on nothing.
    Down(Option<Target>),
    /// Down on an input, so moving the pointer extends its selection.
    Selecting(Field),
    /// Down on an input after a double click selected a word, from its start
    /// to its end, so moving the pointer extends the selection by words.
    Words(Field, usize, usize),
}

impl Mouse {
    /// What the button went down on.
    const fn target(self) -> Option<Target> {
        match self {
            Self::Up => None,
            Self::Down(target) => target,
            Self::Selecting(field) | Self::Words(field, ..) => Some(Target::Input(field)),
        }
    }
}

/// Where the page's focus is, as the canvas holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Canvas {
    /// Outside the page, in the browser's own controls, or never in it yet.
    #[default]
    Outside,
    /// On the canvas, in a window in the background. The canvas keeps it
    /// after a click on no control too.
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
    history: History,
    /// The text input that last had focus, which Chrome's selection stays
    /// in while the checkbox or the button has focus: undoing its edits
    /// leaves focus where it is.
    last_input: Option<Field>,
    pointer: Option<(f64, f64)>,
    mouse: Mouse,
    /// Where Tab continues from after a click on no control, as from
    /// Chrome's sequential focus navigation starting point: the element
    /// clicked, given by [`rank`] as the first control in it or after it.
    start: Option<usize>,
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
                // Not from a click: Tab or a script bring focus into the page
                // at its first control, and the window regaining focus brings
                // it back.
                if self.mouse == Mouse::Up {
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
                // The release of a button outside the window never comes.
                self.mouse = Mouse::Up;
                if let Some(Control::Field(field)) = self.focus {
                    self.state.blur(field);
                }
                Outcome::default()
            }
            Event::Key(key) => self.key(&key, page, measure),
            Event::KeyUp(key) => self.key_up(&key, page, measure),
            Event::Press {
                x,
                y,
                shift,
                clicks,
            } => self.press((x, y), shift, clicks, page, measure),
            Event::Release { x, y } => self.release((x, y), page, measure),
            Event::Pointer(at) => {
                self.pointer = at;
                let dragged = self.dragged(page, measure);
                self.drag(dragged.map(|(field, _, editor)| (field, editor)))
            }
            Event::Wheel {
                x,
                y,
                dx,
                dy,
                shift,
            } => {
                // Chrome turns a vertical wheel sideways with Shift.
                let dx = if shift && dx == 0.0 { dy } else { dx };
                self.wheel((x, y), dx, page, measure)
            }
            Event::Copy => self.copy(false, page, measure),
            Event::Cut => self.copy(true, page, measure),
            Event::Paste(text) => match self.editing() {
                Some(field) => self.edit(field, page, measure, |editor, text_now| {
                    Some((Edit::Replace, editor.replace(text_now, &single_line(&text))))
                }),
                None => Outcome::default(),
            },
            Event::Blink => {
                self.caret_visible = !self.caret_visible;
                Outcome::default()
            }
            Event::Autoscroll => {
                let scrolled = self.autoscroll(page, measure);
                self.drag(scrolled)
            }
            // Chrome keeps an input's scroll when it narrows, even with the
            // caret out of view, and within the text when it widens.
            Event::Resize => {
                for (&field, editor) in &mut self.editors {
                    let text = InputText::new(field, self.state.text(field), measure);
                    editor.scroll_by(&text, content_width(page, field), 0.0);
                }
                Outcome::default()
            }
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
    pub fn interaction(&self, page: &Layout) -> Interaction {
        let focus = self.focus.filter(|_| self.canvas == Canvas::Focused);
        let editor = self
            .editing()
            .map(|field| self.editor(field))
            .unwrap_or_default();
        let range = editor.range();
        // Every input keeps its own scroll, which the focused one only shows
        // while the canvas has focus.
        let scroll_left = TEXT_FIELDS.map(|field| {
            let hidden = self.focus == Some(Control::Field(field)) && self.editing() != Some(field);
            if hidden {
                0.0
            } else {
                self.editor(field).scroll
            }
        });
        let button = match &page.content {
            Content::Form(form) => Some(form.button),
            Content::Success(_) => None,
        };
        Interaction {
            focus,
            focus_visible: self.focus_visible,
            hovered: self.device.hover
                && self
                    .pointer
                    .zip(button)
                    .is_some_and(|(at, rect)| inside(rect, at)),
            caret_visible: self.caret_visible,
            scroll_left,
            selection: (range.start, range.end),
        }
    }

    /// The mouse cursor over the canvas, as the React app's elements set it.
    #[must_use]
    pub fn cursor(&self, page: &Layout, measure: &impl Measure) -> &'static str {
        let target = self.pointer.and_then(|at| hit(page, at, measure));
        match target {
            _ if matches!(self.mouse, Mouse::Selecting(_) | Mouse::Words(..)) => "text",
            Some(Target::Input(_)) => "text",
            Some(Target::Button) if self.state.status() == Status::Submitting => "wait",
            _ => "default",
        }
    }

    /// Whether focus came in at the first control without a click, until the
    /// release of the key that brought it tells which way it went.
    #[must_use]
    pub const fn entering_first(&self) -> bool {
        matches!(self.entering, Some(Entering::First { .. }))
    }

    /// The text input being edited, if the canvas has focus in one.
    #[must_use]
    pub fn editing(&self) -> Option<Field> {
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
        self.history.close();
        if let Some(Control::Field(field)) = control
            && field != Field::AcceptTerms
        {
            self.last_input = Some(field);
        }
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
        let next = match (current, self.start.take()) {
            (Some(index), _) if backward => index.checked_sub(1),
            (Some(index), _) => Some(index + 1),
            (None, Some(start)) if backward => controls
                .iter()
                .rposition(|&(control, _)| rank(control) < start),
            (None, Some(start)) => controls
                .iter()
                .position(|&(control, _)| rank(control) >= start),
            (None, None) if backward => controls.len().checked_sub(1),
            (None, None) => Some(0),
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
        // Tabbing into a text input selects its value, and keeps its scroll.
        if let Control::Field(field) = control {
            let text = InputText::new(field, self.state.text(field), measure);
            let editor = self.editors.entry(field).or_default();
            editor.select(0..text.len());
        }
        true
    }

    fn key(&mut self, key: &Key, page: &Layout, measure: &impl Measure) -> Outcome {
        self.entering = None;
        // Keyboard use makes the focused control match `:focus-visible`, but
        // not shortcuts with Ctrl, Alt or the Command key.
        if !(key.ctrl || key.alt || key.meta) {
            self.focus_visible = true;
        }
        if key.key == "Tab" && !(key.ctrl || key.alt || key.meta) {
            let handled = self.tab(page, measure, key.shift);
            return Outcome {
                handled,
                blink: handled,
                effect: None,
            };
        }
        if let Some(redo) = self.undo_key(key) {
            return self.undo(redo, page, measure);
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

    /// Whether `key` undoes, `Some(false)`, or redoes, `Some(true)`, with
    /// Chrome's shortcuts: Ctrl+Y redoes too outside macOS.
    fn undo_key(&self, key: &Key) -> Option<bool> {
        let command = if self.device.mac {
            key.meta && !key.ctrl
        } else {
            key.ctrl && !key.meta
        };
        if !command || key.alt {
            return None;
        }
        match key.key.as_str() {
            "z" | "Z" => Some(key.shift),
            "y" if !self.device.mac && !key.shift => Some(true),
            _ => None,
        }
    }

    /// Undoes, or redoes, the last step of the page's history, which may be
    /// in another input. Redoing focuses the input, and so does undoing,
    /// unless it was the last input to have focus, as in Chrome.
    fn undo(&mut self, redo: bool, page: &Layout, measure: &impl Measure) -> Outcome {
        // The form is read-only while it submits, and gone after.
        if matches!(self.state.status(), Status::Submitting | Status::Success) {
            return Outcome::handled();
        }
        let snapshot = if redo {
            self.history.redo()
        } else {
            self.history.undo()
        };
        let Some(Snapshot {
            field,
            text,
            anchor,
            head,
        }) = snapshot
        else {
            return Outcome::handled();
        };
        self.state.set_text(field, &text);
        if redo || self.last_input != Some(field) {
            self.focus_on(Some(Control::Field(field)));
        }
        let mut editor = self.editor(field);
        (editor.anchor, editor.head) = (anchor, head);
        if self.focus == Some(Control::Field(field)) {
            editor.reveal(
                &InputText::new(field, &text, measure),
                content_width(page, field),
            );
        }
        self.editors.insert(field, editor);
        Outcome {
            handled: true,
            blink: true,
            effect: None,
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
        self.start = None;
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
                let editor = self.editors.entry(field).or_default();
                let before = *editor;
                editor.select(0..text.len());
                if *editor != before {
                    self.history.close();
                }
                Outcome::handled()
            }
            "Backspace" | "Delete" => {
                let forward = key.key == "Delete";
                let outcome = self.edit(field, page, measure, |editor, text| {
                    // Command deletes to the start, or the end, of the line.
                    if line && editor.range().is_empty() {
                        let motion = if forward {
                            Motion::LineEnd
                        } else {
                            Motion::LineStart
                        };
                        editor.go(text, motion, true);
                    }
                    let edit = if forward {
                        Edit::Delete
                    } else {
                        Edit::Backspace
                    };
                    editor
                        .delete(text, forward, word)
                        .map(|value| (edit, value))
                });
                // Chrome scrolls to the caret even with nothing to delete.
                Outcome {
                    effect: (!self.state.locked()).then_some(Effect::Reveal),
                    ..outcome
                }
            }
            typed if self.inserts(key) => self.edit(field, page, measure, |editor, text| {
                Some((Edit::Type, editor.replace(text, typed)))
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
        change: impl FnOnce(&mut Editor, &InputText) -> Option<(Edit, String)>,
    ) -> Outcome {
        let mut text = InputText::new(field, self.state.text(field), measure);
        let before = self.editor(field);
        let mut editor = before;
        let changed = change(&mut editor, &text);
        let moved = (editor.anchor, editor.head) != (before.anchor, before.head);
        let edited = changed.is_some();
        if let Some((edit, value)) = changed {
            let old = self.state.text(field).to_owned();
            // A read-only input, while the form submits, keeps its value.
            if !self.state.set_text(field, &value) {
                return Outcome::handled();
            }
            text = InputText::new(field, &value, measure);
            let (before, after) = (snapshot(field, old, before), snapshot(field, value, editor));
            self.history.record(edit, before, after);
        } else if moved {
            // Moving the selection ends the typing in a row.
            self.history.close();
        }
        editor.reveal(&text, content_width(page, field));
        self.editors.insert(field, editor);
        Outcome {
            handled: true,
            blink: true,
            // Not in a read-only input, while the form submits.
            effect: ((edited || moved) && !self.state.locked()).then_some(Effect::Reveal),
        }
    }

    /// Puts the caret of `field` where `x` is on the page.
    fn place(
        &mut self,
        field: Field,
        x: f64,
        extend: bool,
        page: &Layout,
        measure: &impl Measure,
    ) -> Outcome {
        let at = x - content_x(page, field) + self.editor(field).scroll;
        self.edit(field, page, measure, |editor, text| {
            editor.place(text.offset_at(at), extend);
            None
        })
    }

    /// The dragged input, its text, and its selection extended to the
    /// pointer, by words after a double click.
    fn dragged(&self, page: &Layout, measure: &impl Measure) -> Option<(Field, InputText, Editor)> {
        let field = match self.mouse {
            Mouse::Selecting(field) | Mouse::Words(field, ..)
                if self.focus == Some(Control::Field(field)) =>
            {
                field
            }
            _ => return None,
        };
        let (x, _) = self.pointer?;
        let text = InputText::new(field, self.state.text(field), measure);
        let mut editor = self.editor(field);
        let at = x - content_x(page, field) + editor.scroll;
        if let Mouse::Words(_, start, end) = self.mouse {
            let under = text.word_at(at);
            (editor.anchor, editor.head) = if under.start < start {
                (end, under.start)
            } else {
                (start, under.end.max(end))
            };
        } else {
            editor.place(text.offset_at(at), true);
        }
        Some((field, text, editor))
    }

    /// Scrolls the text input under the pointer sideways by `dx`, focused or
    /// not, as Chrome scrolls one under a wheel or a trackpad; then the page
    /// must not scroll too.
    fn wheel(&mut self, at: (f64, f64), dx: f64, page: &Layout, measure: &impl Measure) -> Outcome {
        let Some(Target::Input(field)) = hit(page, at, measure) else {
            return Outcome::default();
        };
        let text = InputText::new(field, self.state.text(field), measure);
        let editor = self.editors.entry(field).or_default();
        let before = editor.scroll;
        editor.scroll_by(&text, content_width(page, field), dx);
        let moved = editor.scroll - before;
        Outcome {
            handled: moved != 0.0,
            ..Outcome::default()
        }
    }

    /// Puts a dragged selection in place. Only the autoscroll scrolls the
    /// input, as in Chrome.
    fn drag(&mut self, dragged: Option<(Field, Editor)>) -> Outcome {
        let Some((field, editor)) = dragged else {
            return Outcome::default();
        };
        let before = self.editor(field);
        if (editor.anchor, editor.head) != (before.anchor, before.head) {
            self.history.close();
        }
        self.editors.insert(field, editor);
        Outcome {
            handled: true,
            blink: true,
            effect: None,
        }
    }

    /// Whether a selection drag holds the pointer near or past its input's
    /// sides, where the input can still scroll, so that each animation frame
    /// must bring an [`Event::Autoscroll`].
    #[must_use]
    pub fn autoscrolling(&self, page: &Layout, measure: &impl Measure) -> bool {
        self.autoscroll(page, measure).is_some()
    }

    /// The dragged input with its selection and scroll after the next
    /// animation frame, unless they stay: the selection follows the pointer,
    /// then the input scrolls the least that shows [`AUTOSCROLL_MARGIN`]
    /// either side of the pointer.
    fn autoscroll(&self, page: &Layout, measure: &impl Measure) -> Option<(Field, Editor)> {
        let (field, text, mut editor) = self.dragged(page, measure)?;
        let (x, _) = self.pointer?;
        let content = content_box(input_of(page, field));
        // The pointer covers the pixel from `x` to `x + 1`.
        let step = (x - AUTOSCROLL_MARGIN - content.x).floor().min(0.0)
            + (x + 1.0 + AUTOSCROLL_MARGIN - (content.x + content.width))
                .ceil()
                .max(0.0);
        editor.scroll_by(&text, content.width, step);
        (editor != self.editor(field)).then_some((field, editor))
    }

    fn press(
        &mut self,
        at: (f64, f64),
        shift: bool,
        clicks: u32,
        page: &Layout,
        measure: &impl Measure,
    ) -> Outcome {
        self.pointer = Some(at);
        self.focus_visible = false;
        self.entering = None;
        let target = hit(page, at, measure);
        self.mouse = Mouse::Down(target);
        let submitting = self.state.status() == Status::Submitting;
        match target {
            Some(Target::Input(field)) => {
                let extend = shift && self.focus == Some(Control::Field(field));
                self.focus_on(Some(Control::Field(field)));
                if clicks == 1 {
                    self.mouse = Mouse::Selecting(field);
                }
                let x = at.0 - content_x(page, field) + self.editor(field).scroll;
                let outcome = match clicks {
                    0 | 1 => self.place(field, at.0, extend, page, measure),
                    2 => {
                        let mut word = 0..0;
                        let outcome = self.edit(field, page, measure, |editor, text| {
                            word = text.word_at(x);
                            editor.select(word.clone());
                            None
                        });
                        self.mouse = Mouse::Words(field, word.start, word.end);
                        outcome
                    }
                    _ => self.edit(field, page, measure, |editor, text| {
                        editor.select(0..text.len());
                        None
                    }),
                };
                // Unlike a key, a click leaves the page where it is.
                Outcome {
                    effect: None,
                    ..outcome
                }
            }
            Some(Target::Check) => {
                self.focus_on(Some(Control::Field(Field::AcceptTerms)));
                Outcome::default()
            }
            Some(Target::Button) if !submitting => {
                self.focus_on(Some(Control::Button));
                Outcome::default()
            }
            // Nothing that takes focus: the focused control loses it, and
            // Chrome's selection leaves the last input.
            _ => {
                self.focus_on(None);
                self.last_input = None;
                self.start = Some(start_at(page, at));
                Outcome::default()
            }
        }
    }

    /// Ends a press: a click when it ends where it began.
    fn release(&mut self, at: (f64, f64), page: &Layout, measure: &impl Measure) -> Outcome {
        self.pointer = Some(at);
        let pressed = std::mem::take(&mut self.mouse).target();
        let target = hit(page, at, measure);
        if pressed != target {
            return Outcome::default();
        }
        match target {
            Some(Target::Check | Target::Label(Field::AcceptTerms)) => {
                self.focus_on(Some(Control::Field(Field::AcceptTerms)));
                self.state
                    .set_accept_terms(!self.state.values().accept_terms);
                Outcome::default()
            }
            Some(Target::Label(field)) => {
                self.focus_field(field, page, measure);
                Outcome {
                    blink: true,
                    ..Outcome::default()
                }
            }
            Some(Target::Button) => self.submit(page, measure),
            _ => Outcome::default(),
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
                editor
                    .delete(text, false, false)
                    .map(|value| (Edit::Replace, value))
            })
        } else {
            Outcome::handled()
        };
        // Chrome does not scroll to the caret after a cut.
        outcome.effect = Some(Effect::Clipboard(selected));
        outcome
    }
}

/// The control, or label, at a point of the page.
fn hit(page: &Layout, at: (f64, f64), measure: &impl Measure) -> Option<Target> {
    let Content::Form(form) = &page.content else {
        return None;
    };
    for field in &form.fields {
        if inside(field.input, at) {
            return Some(Target::Input(field.field));
        }
        if inside(field.label.rect, at) {
            return Some(Target::Label(field.field));
        }
    }
    let terms = &form.terms;
    // The terms label is as wide as its text, or its column once it wraps.
    let label = &terms.label;
    let width = match label.lines.as_slice() {
        [line] => measure.width(line, label.style),
        _ => label.rect.width,
    };
    if inside(
        Rect {
            width,
            ..label.rect
        },
        at,
    ) {
        return Some(Target::Label(Field::AcceptTerms));
    }
    if inside(terms.check, at) {
        return Some(Target::Check);
    }
    inside(form.button, at).then_some(Target::Button)
}

/// A control's place in Tab order, kept while the button cannot take focus.
fn rank(control: Control) -> usize {
    match control {
        Control::Field(field) => TEXT_FIELDS
            .iter()
            .position(|&text| text == field)
            .unwrap_or(TEXT_FIELDS.len()),
        Control::Button => TEXT_FIELDS.len() + 1,
    }
}

/// The [`rank`] of the first control in or after the React app's element at
/// a point on no control: Tab goes on into that element, and Shift+Tab back
/// from before it.
fn start_at(page: &Layout, (x, y): (f64, f64)) -> usize {
    let Content::Form(form) = &page.content else {
        return 0;
    };
    let within = |top: f64, bottom: f64| (top..bottom).contains(&y);
    // Each field's box spans the column, as its message and the button do;
    // the card and the form around them hold every control.
    let button = form.button;
    if !(button.x..button.x + button.width).contains(&x) {
        return 0;
    }
    let terms = &form.terms;
    let fields = form
        .fields
        .iter()
        .map(|field| (field.label.rect.y, field.input.bottom(), &field.message));
    let row = (
        terms.check.y,
        terms.check.bottom().max(terms.label.rect.bottom()),
        &terms.message,
    );
    for (rank, (top, bottom, message)) in fields.chain([row]).enumerate() {
        if let Some(message) = message
            && within(message.rect.y, message.rect.bottom())
        {
            return rank + 1;
        }
        if within(top, message.as_ref().map_or(bottom, |m| m.rect.bottom())) {
            return rank;
        }
    }
    // A disabled button and the alert after it come after every control.
    let alert = form
        .error
        .as_ref()
        .map_or(Rect::default(), |error| error.rect);
    if within(button.y, button.bottom()) || within(alert.y, alert.bottom()) {
        rank(Control::Button) + 1
    } else {
        0
    }
}

fn inside(rect: Rect, (x, y): (f64, f64)) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.bottom()
}

const fn snapshot(field: Field, text: String, editor: Editor) -> Snapshot {
    Snapshot {
        field,
        text,
        anchor: editor.anchor,
        head: editor.head,
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

fn content_x(page: &Layout, field: Field) -> f64 {
    content_box(input_of(page, field)).x
}

fn content_width(page: &Layout, field: Field) -> f64 {
    content_box(input_of(page, field)).width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{FormLayout, Viewport, layout};
    use crate::proto;
    use crate::style::TextStyle;
    use crate::text::FontMetrics;
    use crate::text::testing::FixedMeasure;

    const DESKTOP: Viewport = Viewport {
        width: 1280.0,
        height: 900.0,
        media_width: 1280.0,
    };

    const DESKTOP_MOUSE: Device = Device {
        mac: false,
        hover: true,
    };

    fn page(app: &App) -> Layout {
        layout(DESKTOP, &app.state, &FixedMeasure)
    }

    fn form(app: &App) -> FormLayout {
        match page(app).content {
            Content::Form(form) => *form,
            Content::Success(_) => panic!("expected the form"),
        }
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

    fn center(rect: Rect) -> (f64, f64) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn click_with(app: &mut App, (x, y): (f64, f64), clicks: u32) -> Outcome {
        let pressed = send(
            app,
            Event::Press {
                x,
                y,
                shift: false,
                clicks,
            },
        );
        send(app, Event::Focus);
        let released = send(app, Event::Release { x, y });
        if released.effect.is_some() {
            released
        } else {
            pressed
        }
    }

    fn click(app: &mut App, at: (f64, f64)) -> Outcome {
        click_with(app, at, 1)
    }

    fn focus(app: &App) -> Option<Control> {
        app.interaction(&page(app)).focus
    }

    fn selection(app: &App) -> (usize, usize) {
        app.interaction(&page(app)).selection
    }

    /// An app whose canvas has focus in the full name input.
    fn focused() -> App {
        let mut app = App::new(DESKTOP_MOUSE);
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
        assert!(app.interaction(&page(&app)).focus_visible);
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
        let mut app = App::new(DESKTOP_MOUSE);
        app.state.set_text(Field::FullName, "Ada");
        // Focus comes in at the first control, until the key that brought it
        // comes up: Shift+Tab enters at the last.
        send(&mut app, Event::Focus);
        assert_eq!(selection(&app), (0, 3));
        assert!(app.entering_first());
        assert!(send(&mut app, tab_up(true)).blink);
        assert!(!app.entering_first());
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
        let mut app = App::new(Device {
            mac: true,
            hover: true,
        });
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
    fn clicks_place_the_caret_and_select() {
        let mut app = focused();
        type_text(&mut app, "Ada Lovelace");
        let input = form(&app).fields[0].input;
        let content = content_box(input);
        // 16px BODY text: each character is 8px wide.
        click(&mut app, (content.x + 13.0, input.y + 10.0));
        assert_eq!(selection(&app), (2, 2));
        assert!(!app.interaction(&page(&app)).focus_visible);
        send(
            &mut app,
            Event::Press {
                x: content.x + 30.0,
                y: input.y + 10.0,
                shift: false,
                clicks: 1,
            },
        );
        send(
            &mut app,
            Event::Pointer(Some((content.x + 70.0, input.y + 10.0))),
        );
        assert_eq!(app.cursor(&page(&app), &FixedMeasure), "text");
        send(
            &mut app,
            Event::Release {
                x: content.x + 70.0,
                y: input.y + 10.0,
            },
        );
        assert_eq!(selection(&app), (4, 9));
        send(
            &mut app,
            Event::Press {
                x: content.x,
                y: input.y,
                shift: true,
                clicks: 1,
            },
        );
        assert_eq!(selection(&app), (0, 4));
        click_with(&mut app, (content.x + 50.0, input.y + 10.0), 2);
        assert_eq!(selection(&app), (4, 12));
        click_with(&mut app, (content.x + 50.0, input.y + 10.0), 3);
        assert_eq!(selection(&app), (0, 12));

        // A label focuses its input as it was; on the way, focus leaves it.
        let email = form(&app).fields[1].clone();
        click(&mut app, center(email.input));
        send(
            &mut app,
            Event::Press {
                x: email.label.rect.x,
                y: email.label.rect.y,
                shift: false,
                clicks: 1,
            },
        );
        assert_eq!(focus(&app), None);
        assert!(app.state.error(Field::Email).is_some());
        send(
            &mut app,
            Event::Release {
                x: email.label.rect.x,
                y: email.label.rect.y,
            },
        );
        assert_eq!(focus(&app), Some(Control::Field(Field::Email)));
    }

    #[test]
    fn dragging_after_a_double_click_selects_words() {
        let mut app = focused();
        type_text(&mut app, "Ada King Lovelace");
        let input = form(&app).fields[0].input;
        let x = content_box(input).x;
        let y = input.y + 10.0;
        // In the middle of a character, each 8px wide.
        let at = |offset: f64| (x + offset * 8.0 + 4.0, y);
        let (x, y) = at(5.0);
        send(
            &mut app,
            Event::Press {
                x,
                y,
                shift: false,
                clicks: 2,
            },
        );
        assert_eq!(selection(&app), (4, 8));
        send(&mut app, Event::Pointer(Some(at(10.0))));
        assert_eq!(selection(&app), (4, 17));
        send(&mut app, Event::Pointer(Some(at(1.0))));
        let editor = app.editor(Field::FullName);
        assert_eq!((editor.anchor, editor.head), (8, 0));
        // The release outside the window never comes, so blur ends the drag.
        send(&mut app, Event::Blur);
        send(&mut app, Event::Focus);
        send(&mut app, Event::Pointer(Some(at(10.0))));
        assert_eq!(selection(&app), (0, 8));
    }

    #[test]
    fn dragging_near_or_past_an_input_side_scrolls_it_each_frame() {
        let mut app = focused();
        // 99 characters, 792px of text.
        type_text(&mut app, &"Ada King ".repeat(11));
        key(&mut app, "Home");
        let input = form(&app).fields[0].input;
        let content = content_box(input);
        let y = input.y + 10.0;
        let scroll = |app: &App| app.editor(Field::FullName).scroll;
        let autoscrolling = |app: &App| app.autoscrolling(&page(app), &FixedMeasure);
        send(
            &mut app,
            Event::Press {
                x: content.x + 2.0,
                y,
                shift: false,
                clicks: 1,
            },
        );
        assert!(!autoscrolling(&app));
        // The selection follows the pointer 10px past the right side, and
        // each frame then scrolls 31px.
        send(
            &mut app,
            Event::Pointer(Some(((content.x + content.width) + 10.0, y))),
        );
        assert_eq!((selection(&app), scroll(&app)), ((0, 44), 0.0));
        assert!(autoscrolling(&app));
        send(&mut app, Event::Autoscroll);
        assert_eq!((selection(&app), scroll(&app)), ((0, 44), 31.0));
        send(&mut app, Event::Autoscroll);
        assert_eq!((selection(&app), scroll(&app)), ((0, 48), 62.0));
        while autoscrolling(&app) {
            send(&mut app, Event::Autoscroll);
        }
        assert_eq!((selection(&app), scroll(&app)), ((0, 99), 452.0));
        assert_eq!(send(&mut app, Event::Autoscroll), Outcome::default());
        // 5px inside the left side, each frame scrolls back 15px.
        send(&mut app, Event::Pointer(Some((content.x + 5.0, y))));
        send(&mut app, Event::Autoscroll);
        assert_eq!((selection(&app), scroll(&app)), ((0, 57), 437.0));
        // Away from the sides, or once the drag ends, the input stays.
        send(&mut app, Event::Pointer(Some((content.x + 100.0, y))));
        assert!(!autoscrolling(&app));
        send(&mut app, Event::Pointer(Some((content.x - 10.0, y))));
        send(&mut app, Event::Blur);
        assert!(!autoscrolling(&app));
    }

    #[test]
    fn the_checkbox_and_its_label_toggle_on_click() {
        let mut app = App::new(DESKTOP_MOUSE);
        let terms = form(&app).terms;
        click(&mut app, center(terms.check));
        assert!(app.state.values().accept_terms);
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        assert!(!app.interaction(&page(&app)).focus_visible);
        // Shortcuts with Ctrl leave the focus ring hidden, but other keys,
        // even Shift alone, show it.
        press(&mut app, "c", |key| key.ctrl = true);
        assert!(!app.interaction(&page(&app)).focus_visible);
        key(&mut app, "Shift");
        assert!(app.interaction(&page(&app)).focus_visible);
        click(
            &mut app,
            (terms.label.rect.x + 1.0, terms.label.rect.y + 1.0),
        );
        assert!(!app.state.values().accept_terms);
        // A press that ends elsewhere is no click.
        let pressed = Event::Press {
            x: terms.check.x,
            y: terms.check.y,
            shift: false,
            clicks: 1,
        };
        send(&mut app, pressed);
        send(&mut app, Event::Release { x: 0.0, y: 0.0 });
        assert!(!app.state.values().accept_terms);
    }

    /// Text half as wide as [`FixedMeasure`] makes it.
    struct Narrow;

    impl Measure for Narrow {
        fn width(&self, text: &str, style: TextStyle) -> f64 {
            FixedMeasure.width(text, style) / 2.0
        }

        fn metrics(&self, style: TextStyle) -> FontMetrics {
            FixedMeasure.metrics(style)
        }
    }

    #[test]
    fn a_one_line_terms_label_ends_with_its_text() {
        let app = App::default();
        let page = layout(DESKTOP, &app.state, &Narrow);
        let Content::Form(form) = &page.content else {
            panic!("expected the form");
        };
        let label = &form.terms.label;
        assert_eq!(label.lines.len(), 1);
        let end = label.rect.x + Narrow.width(&label.lines[0], label.style);
        let terms = Some(Target::Label(Field::AcceptTerms));
        assert_eq!(hit(&page, (end - 1.0, label.rect.y + 1.0), &Narrow), terms);
        assert_eq!(hit(&page, (end + 1.0, label.rect.y + 1.0), &Narrow), None);
    }

    #[test]
    fn the_wheel_scrolls_the_input_under_it_sideways() {
        let mut app = focused();
        let long = "a.very.long.address.that.overflows@example.com";
        app.state.set_text(Field::Email, long);
        let email = center(form(&app).fields[1].input);
        let wheel = |app: &mut App, dx: f64, dy: f64, shift: bool| {
            let (x, y) = email;
            let handled = send(
                app,
                Event::Wheel {
                    x,
                    y,
                    dx,
                    dy,
                    shift,
                },
            )
            .handled;
            (handled, app.interaction(&page(app)).scroll_left[1])
        };
        // Unfocused, and as far as its end goes.
        let end = wheel(&mut app, 1000.0, 0.0, false);
        assert!(end.0 && end.1 > 0.0);
        assert_eq!(wheel(&mut app, -10.0, 0.0, false), (true, end.1 - 10.0));
        // Shift turns the wheel sideways; down alone scrolls the page.
        assert_eq!(wheel(&mut app, 0.0, -1000.0, true), (true, 0.0));
        assert_eq!(wheel(&mut app, 0.0, 40.0, false), (false, 0.0));
        assert_eq!(wheel(&mut app, -10.0, 0.0, false), (false, 0.0));
        // Focus keeps the scroll, from a click or from Tab.
        wheel(&mut app, 20.0, 0.0, false);
        click(&mut app, email);
        assert_eq!(focus(&app), Some(Control::Field(Field::Email)));
        assert_eq!(app.interaction(&page(&app)).scroll_left[1], 20.0);
        key(&mut app, "Tab");
        wheel(&mut app, 20.0, 0.0, false);
        press(&mut app, "Tab", |key| key.shift = true);
        assert_eq!(app.interaction(&page(&app)).scroll_left[1], 20.0);
    }

    #[test]
    fn keys_that_edit_or_move_the_caret_scroll_the_page_to_it() {
        let mut app = focused();
        let reveal = Some(Effect::Reveal);
        assert_eq!(key(&mut app, "a").effect, reveal);
        key(&mut app, "b");
        assert_eq!(key(&mut app, "Home").effect, reveal);
        assert_eq!(key(&mut app, "ArrowLeft").effect, None);
        // Backspace and Delete do even with nothing to delete.
        assert_eq!(key(&mut app, "Backspace").effect, reveal);
        assert_eq!(press(&mut app, "a", |key| key.ctrl = true).effect, None);
        let cut = send(&mut app, Event::Cut).effect;
        assert_eq!(cut, Some(Effect::Clipboard("ab".to_owned())));
        assert_eq!(send(&mut app, Event::Paste("xy".to_owned())).effect, reveal);
        let input = form(&app).fields[0].input;
        let start = (input.x + 14.0, input.y + input.height / 2.0);
        assert_eq!(click(&mut app, start).effect, None);
        assert_eq!(selection(&app), (0, 0));
        assert_eq!(press(&mut app, "z", |key| key.ctrl = true).effect, None);
        // Nor does a key in a read-only input, while the form submits.
        fill(&mut app);
        assert!(matches!(
            key(&mut app, "Enter").effect,
            Some(Effect::Send(_))
        ));
        assert_eq!(key(&mut app, "End").effect, None);
        assert_eq!(selection(&app), (12, 12));
    }

    #[test]
    fn tab_continues_from_the_element_clicked() {
        let mut app = focused();
        // An empty submit shows every message.
        key(&mut app, "Enter");
        let form = form(&app);
        let password = &form.fields[2];
        let messages = [&password.message, &form.terms.message].map(|message| {
            message
                .as_ref()
                .map_or_else(Rect::default, |message| message.rect)
        });
        let x = password.input.x + 4.0;
        let field = |field| Some(Control::Field(field));
        for (at, next, previous) in [
            // Between a label and its input is the field's own box.
            (
                (x, password.input.y - 2.0),
                field(Field::Password),
                field(Field::Email),
            ),
            (
                (x, messages[0].y + 1.0),
                field(Field::ConfirmPassword),
                field(Field::Password),
            ),
            (
                (x, messages[1].y + 1.0),
                Some(Control::Button),
                field(Field::AcceptTerms),
            ),
            // Between fields is the form, and beside them the card.
            (
                (x, password.label.rect.y - 2.0),
                field(Field::FullName),
                None,
            ),
            ((x - 8.0, password.input.y), field(Field::FullName), None),
        ] {
            for (shift, focused) in [(false, next), (true, previous)] {
                click(&mut app, at);
                assert_eq!(focus(&app), None);
                press(&mut app, "Tab", |key| key.shift = shift);
                assert_eq!(focus(&app), focused, "{at:?}, shift {shift}");
            }
        }
    }

    #[test]
    fn the_button_submits_and_waits_for_the_backend() {
        let mut app = focused();
        fill(&mut app);
        let button = form(&app).button;
        send(&mut app, Event::Pointer(Some(center(button))));
        assert!(app.interaction(&page(&app)).hovered);
        let Some(Effect::Send(request)) = click(&mut app, center(button)).effect else {
            panic!("expected a request");
        };
        assert_eq!(request.full_name, "Ada Lovelace");
        assert_eq!(app.cursor(&page(&app), &FixedMeasure), "wait");
        // Locked: no second request, no typing, and no button to tab to.
        assert_eq!(click(&mut app, center(button)).effect, None);
        let confirm = form(&app).fields[3].input;
        click(&mut app, center(confirm));
        type_text(&mut app, "x");
        assert_eq!(app.state.text(Field::ConfirmPassword), "Analytical#Engine1");
        key(&mut app, "Tab");
        assert!(!key(&mut app, "Tab").handled);
        assert_eq!(focus(&app), None);

        let outcome = send(&mut app, Event::Response(Ok(email_violation())));
        assert_eq!(outcome.effect, Some(Effect::Focus));
        send(&mut app, Event::Focus);
        assert_eq!(focus(&app), Some(Control::Field(Field::Email)));
        click(&mut app, center(button));
        assert_eq!(
            send(&mut app, Event::Response(Err(CallError::Http(503)))),
            Outcome::default()
        );
        click(&mut app, center(button));
        send(&mut app, Event::Response(Ok(SignUpResponse::default())));
        assert_eq!(app.state.status(), Status::Success);
        assert_eq!(focus(&app), None);
        assert!(!app.interaction(&page(&app)).hovered);
        assert!(!key(&mut app, "Tab").handled);
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
    fn undo_and_redo_go_through_the_page_s_edits() {
        let mut app = focused();
        let undo = |app: &mut App| press(app, "z", |key| key.ctrl = true);
        let redo = |app: &mut App| {
            press(app, "Z", |key| {
                key.ctrl = true;
                key.shift = true;
            })
        };
        let name = |app: &App| (app.state.text(Field::FullName).to_owned(), selection(app));
        // Moving the caret ends the typing in a row.
        type_text(&mut app, "Ada");
        key(&mut app, "ArrowLeft");
        type_text(&mut app, "n");
        assert!(undo(&mut app).handled);
        assert_eq!(name(&app), ("Ada".to_owned(), (2, 2)));
        redo(&mut app);
        key(&mut app, "Tab");
        type_text(&mut app, "ada@x.io");
        undo(&mut app);
        assert_eq!(app.state.text(Field::Email), "");
        // The step before is in another input, which takes focus.
        undo(&mut app);
        assert_eq!(focus(&app), Some(Control::Field(Field::FullName)));
        assert_eq!(name(&app), ("Ada".to_owned(), (2, 2)));
        press(&mut app, "y", |key| key.ctrl = true);
        assert_eq!(name(&app), ("Adna".to_owned(), (3, 3)));
        // Chrome's selection stays in the last input while the checkbox has
        // focus, so undoing its edit leaves focus there.
        for _ in 0..3 {
            key(&mut app, "Tab");
        }
        type_text(&mut app, "x");
        key(&mut app, "Tab");
        undo(&mut app);
        assert_eq!(app.state.text(Field::ConfirmPassword), "");
        assert_eq!(focus(&app), Some(Control::Field(Field::AcceptTerms)));
        // Redoing focuses it anyway.
        redo(&mut app);
        assert_eq!(focus(&app), Some(Control::Field(Field::ConfirmPassword)));
        // A click on the page takes the selection out of it.
        click(&mut app, (1.0, 1.0));
        undo(&mut app);
        assert_eq!(focus(&app), Some(Control::Field(Field::ConfirmPassword)));
        // Nothing changes while the form submits.
        fill(&mut app);
        key(&mut app, "Enter");
        undo(&mut app);
        assert_eq!(app.state.text(Field::FullName), "Ada Lovelace");

        let mut mac = App::new(Device {
            mac: true,
            hover: true,
        });
        send(&mut mac, Event::Focus);
        type_text(&mut mac, "a");
        press(&mut mac, "z", |key| key.meta = true);
        assert_eq!(mac.state.text(Field::FullName), "");
        press(&mut mac, "z", |key| {
            key.meta = true;
            key.shift = true;
        });
        assert_eq!(mac.state.text(Field::FullName), "a");
    }

    #[test]
    fn a_resize_keeps_the_scroll_within_the_text() {
        let phone = Viewport {
            width: 390.0,
            height: 844.0,
            media_width: 390.0,
        };
        let mut app = focused();
        // 792px of text.
        type_text(&mut app, &"Ada King ".repeat(11));
        let scroll = |app: &App| app.editor(Field::FullName).scroll;
        let wide = scroll(&app);
        assert_eq!(wide, 792.0 - content_width(&page(&app), Field::FullName));
        let narrow = layout(phone, &app.state, &FixedMeasure);
        app.handle(Event::Resize, &narrow, &FixedMeasure);
        assert_eq!(scroll(&app), wide);
        let end = Key {
            key: "End".to_owned(),
            ..Key::default()
        };
        app.handle(Event::Key(end), &narrow, &FixedMeasure);
        assert!(scroll(&app) > wide);
        send(&mut app, Event::Resize);
        assert_eq!(scroll(&app), wide);
    }

    #[test]
    fn the_caret_blinks_in_the_focused_input() {
        let mut app = focused();
        assert!(key(&mut app, "a").blink);
        assert!(app.interaction(&page(&app)).caret_visible);
        send(&mut app, Event::Blink);
        assert!(!app.interaction(&page(&app)).caret_visible);
        assert_eq!(send(&mut app, Event::Resize), Outcome::default());
        // It shows again at once when the window regains focus.
        send(&mut app, Event::Blur);
        assert!(send(&mut app, Event::Focus).blink);
        assert!(app.interaction(&page(&app)).caret_visible);
        // Out of the page, no caret blinks.
        for _ in 0..5 {
            assert!(key(&mut app, "Tab").blink);
        }
        assert!(!key(&mut app, "Tab").blink);
    }

    #[test]
    fn the_pointer_sets_the_cursor_and_hovers() {
        let mut app = focused();
        let form = form(&app);
        send(&mut app, Event::Pointer(Some(center(form.fields[0].input))));
        assert_eq!(app.cursor(&page(&app), &FixedMeasure), "text");
        send(&mut app, Event::Pointer(Some(center(form.button))));
        assert_eq!(app.cursor(&page(&app), &FixedMeasure), "default");
        send(&mut app, Event::Pointer(None));
        assert!(!app.interaction(&page(&app)).hovered);
        let mut touch = App::new(Device::default());
        send(&mut touch, Event::Pointer(Some(center(form.button))));
        assert!(!touch.interaction(&page(&touch)).hovered);
        // A click on the page leaves no caret to blink.
        assert!(!click(&mut app, (1.0, 1.0)).blink);
    }
}
