//! Browser glue: paints [`scene`] on the page's only element, a canvas, and
//! feeds the keys, mouse buttons, focus and clipboard events it receives to
//! [`App`]. The logic it drives is tested natively; this module only talks
//! to the DOM.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use form_core::Field;
use js_sys::{Array, Function, Intl, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    CanvasRenderingContext2d, ClipboardEvent, Document, EventTarget, HtmlCanvasElement,
    KeyboardEvent, MouseEvent, Path2d, Request, RequestInit, Response, Window,
};

use crate::form::Status;
use crate::grpc_web::{self, CallError, RawResponse};
use crate::input::{App, Device, Effect, Event, Key, Outcome};
use crate::layout::{Content, Layout, Part, Rect, Viewport, layout};
use crate::proto::{SignUpRequest, SignUpResponse};
use crate::scene::{self, Control, Painter};
use crate::style::{Shadow, TextStyle};
use crate::text::{FontMetrics, Measure};
use crate::tokens::text;

const SIGN_UP: &str = "/signup.v1.SignupService/SignUp";
/// Chrome's caret blink interval.
const BLINK_MS: i32 = 500;

type Shared = Rc<RefCell<Browser>>;

#[wasm_bindgen(start)]
pub fn start() {
    spawn_local(async {
        if let Err(error) = run().await {
            web_sys::console::error_1(&error);
        }
    });
}

async fn run() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    // Paint only once the font is ready, as `font-display: block` does. If
    // it fails to load, the text falls back to the next font, as in the DOM.
    for style in [text::BODY, text::LABEL, text::TITLE] {
        if let Err(error) = JsFuture::from(document.fonts().load(&style.font())).await {
            web_sys::console::warn_1(&error);
        }
    }
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("app")
        .ok_or("no #app")?
        .dyn_into()?;
    // An opaque canvas lets Chrome antialias text with subpixels, as in the DOM.
    let options = Object::new();
    Reflect::set(&options, &"alpha".into(), &false.into())?;
    let context: CanvasRenderingContext2d = canvas
        .get_context_with_context_options("2d", &options)?
        .ok_or("no 2d context")?
        .dyn_into()?;
    let platform = window.navigator().platform().unwrap_or_default();
    let device = Device {
        mac: platform.starts_with("Mac") || platform.starts_with("iP"),
        hover: window
            .match_media("(hover: hover)")?
            .is_some_and(|query| query.matches()),
    };
    let measure = CanvasMeasure::new(context.clone());
    let app = App::new(device);
    let page = layout(viewport(&window, &document)?, &app.state, &measure);
    let browser = Browser {
        window,
        document,
        canvas,
        context,
        measure,
        app,
        page,
        blink: None,
        blink_timer: 0,
        frame: None,
        framing: false,
        entry_scroll: None,
    };
    let shared = Rc::new(RefCell::new(browser));
    listen(&shared)?;
    shared.borrow_mut().render()
}

struct Browser {
    window: Window,
    document: Document,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    measure: CanvasMeasure,
    app: App,
    /// The layout last painted, which events' points refer to.
    page: Layout,
    /// Toggles the painted caret, every [`BLINK_MS`] from `blink_timer`'s start.
    blink: Option<Function>,
    blink_timer: i32,
    /// Brings an [`Event::Autoscroll`] on the next animation frame.
    frame: Option<Function>,
    /// Whether `frame` is requested.
    framing: bool,
    /// How far the page was scrolled before focus came in at its first
    /// control, which the release of Shift+Tab takes back.
    entry_scroll: Option<f64>,
}

/// What Chrome's scroll anchoring holds in place while the layout changes.
struct Anchor {
    part: Part,
    rect: Rect,
    /// How far the page was scrolled.
    top: f64,
    viewport: Viewport,
}

impl Browser {
    /// Starts the caret's blink period over, so that it stays visible for a
    /// whole period after it moves, as in Chrome.
    fn restart_blink(&mut self) -> Result<(), JsValue> {
        let Some(blink) = &self.blink else {
            return Ok(());
        };
        self.window.clear_interval_with_handle(self.blink_timer);
        self.blink_timer = self
            .window
            .set_interval_with_callback_and_timeout_and_arguments_0(blink, BLINK_MS)?;
        Ok(())
    }

    /// Requests the next animation frame, while a selection drag scrolls.
    fn request_frame(&mut self) -> Result<(), JsValue> {
        if let Some(frame) = &self.frame {
            self.window.request_animation_frame(frame)?;
            self.framing = true;
        }
        Ok(())
    }

    /// Lays the page out for the viewport as it is now.
    fn lay_out(&mut self) -> Result<(), JsValue> {
        let viewport = viewport(&self.window, &self.document)?;
        self.page = layout(viewport, &self.app.state, &self.measure);
        Ok(())
    }

    fn render(&mut self) -> Result<(), JsValue> {
        self.lay_out()?;
        let page = &self.page;
        let ratio = self.window.device_pixel_ratio();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (width, height) = (
            (page.width * ratio).round() as u32,
            (page.height * ratio).round() as u32,
        );
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }
        // Zooming can change the CSS size alone, leaving the buffer's as is.
        let style = self.canvas.style();
        style.set_property("width", &px(page.width))?;
        style.set_property("height", &px(page.height))?;
        style.set_property("cursor", self.app.cursor(page, &self.measure))?;
        // Tells scripts, such as the browser tests, when the page is ready.
        let busy = self.app.state.status() == Status::Submitting;
        self.canvas
            .set_attribute("aria-busy", if busy { "true" } else { "false" })?;
        self.context
            .set_transform(ratio, 0.0, 0.0, ratio, 0.0, 0.0)?;
        let mut painter = CanvasPainter {
            context: &self.context,
            ratio,
        };
        scene::paint(
            page,
            &self.app.state,
            &self.app.interaction(page),
            &self.measure,
            &mut painter,
        );
        Ok(())
    }

    /// The part of the page in view: its top and its height.
    fn view(&self) -> Result<(f64, f64), JsValue> {
        let html = self.document.document_element().ok_or("no <html>")?;
        Ok((self.window.scroll_y()?, f64::from(html.client_height())))
    }

    /// The part of the page that Chrome's scroll anchoring keeps in place.
    fn anchor(&self) -> Result<Option<Anchor>, JsValue> {
        let (top, height) = self.view()?;
        let anchor = self.page.anchor(top, height);
        Ok(anchor.map(|(part, rect)| Anchor {
            part,
            rect,
            top,
            viewport: self.page.viewport,
        }))
    }

    /// Scrolls the page by as far as `anchor` moved with the layout, so that
    /// the page above it can change without moving what is in view.
    fn keep(&self, anchor: Option<Anchor>) -> Result<(), JsValue> {
        let viewport = self.page.viewport;
        let anchor = anchor.filter(|anchor| anchor.viewport.keeps_anchor(viewport));
        let moved = anchor.and_then(|anchor| {
            let parts = self.page.parts();
            let &(_, after) = parts.iter().find(|&&(part, _)| part == anchor.part)?;
            // From where the page was, which a shorter page may have clamped.
            let delta = after.y - anchor.rect.y;
            (delta != 0.0).then_some(anchor.top + delta)
        });
        if let Some(y) = moved {
            self.window
                .scroll_to_with_x_and_y(self.window.scroll_x()?, y);
        }
        Ok(())
    }

    fn rect(&self, control: Control) -> Rect {
        let Content::Form(form) = &self.page.content else {
            return Rect::default();
        };
        match control {
            Control::Field(Field::AcceptTerms) => form.terms.check,
            Control::Field(field) => form
                .fields
                .iter()
                .find(|layout| layout.field == field)
                .map_or_else(Rect::default, |layout| layout.input),
            Control::Button => form.button,
        }
    }

    /// Scrolls a control that focus moved to into view as Chrome scrolls a
    /// focused element: not at all when it is in view, to its nearest edge
    /// when it is partly in view, and to the middle of the view otherwise.
    fn reveal(&self, control: Control) -> Result<(), JsValue> {
        self.scroll_into_view(self.rect(control), true)
    }

    /// Scrolls the focused input's caret into view as Chrome does while
    /// editing: to its nearest edge, even when it is out of view.
    fn reveal_caret(&self) -> Result<(), JsValue> {
        let Some(field) = self.app.editing() else {
            return Ok(());
        };
        let input = self.rect(Control::Field(field));
        self.scroll_into_view(scene::caret_line(input, &self.measure), false)
    }

    fn scroll_into_view(&self, rect: Rect, center: bool) -> Result<(), JsValue> {
        let (top, height) = self.view()?;
        let bottom = top + height;
        let y = if center && (rect.bottom() <= top || rect.y >= bottom) {
            rect.y + (rect.height - height) / 2.0
        } else if rect.y < top {
            rect.y
        } else if rect.bottom() > bottom {
            rect.bottom() - height
        } else {
            return Ok(());
        };
        self.window
            .scroll_to_with_x_and_y(self.window.scroll_x()?, y);
        Ok(())
    }
}

/// The viewport, whose width excludes a vertical scrollbar.
fn viewport(window: &Window, document: &Document) -> Result<Viewport, JsValue> {
    let html = document.document_element().ok_or("no <html>")?;
    Ok(Viewport {
        width: f64::from(html.client_width()),
        height: window.inner_height()?.as_f64().unwrap_or_default(),
        media_width: window.inner_width()?.as_f64().unwrap_or_default(),
    })
}

fn px(value: f64) -> String {
    format!("{value}px")
}

/// Handles `event` with the browser state released before any DOM call that
/// could fire further events synchronously, and returns what the event's
/// listener must still do; `None` when it is handled later.
fn dispatch(shared: &Shared, event: Event) -> Option<Outcome> {
    let Ok(mut borrowed) = shared.try_borrow_mut() else {
        // Fired by a DOM call made while handling another event.
        let shared = Rc::clone(shared);
        spawn_local(async move {
            dispatch(&shared, event);
        });
        return None;
    };
    // Tab, from the page or from the browser's own controls, scrolls the
    // control it focuses into view; a click or an undo does not.
    let tab = matches!(&event, Event::Key(key) | Event::KeyUp(key) if key.key == "Tab");
    let (entering, released) = (event == Event::Focus, matches!(event, Event::KeyUp(_)));
    let browser = &mut *borrowed;
    let focus = browser.app.interaction(&browser.page).focus;
    let anchor = browser.anchor();
    // A resize applies to the new layout.
    let mut result = if event == Event::Resize {
        browser.lay_out()
    } else {
        Ok(())
    };
    let mut outcome = browser.app.handle(event, &browser.page, &browser.measure);
    result = result.and_then(|()| browser.render());
    result = result.and_then(|()| browser.keep(anchor?));
    if outcome.blink {
        result = result.and_then(|()| browser.restart_blink());
    }
    if !browser.framing && browser.app.autoscrolling(&browser.page, &browser.measure) {
        result = result.and_then(|()| browser.request_frame());
    }
    let keyboard = tab || (entering && browser.app.entering_first());
    let moved =
        browser.app.interaction(&browser.page).focus.filter(|&now| {
            Some(now) != focus && (keyboard || outcome.effect == Some(Effect::Focus))
        });
    if let Some(control) = moved {
        result = result.and_then(|()| {
            if entering {
                browser.entry_scroll = Some(browser.window.scroll_y()?);
            } else if let (true, Some(y)) = (released, browser.entry_scroll) {
                // Shift+Tab brought focus in at the last control instead.
                let x = browser.window.scroll_x()?;
                browser.window.scroll_to_with_x_and_y(x, y);
            }
            browser.reveal(control)
        });
    }
    if !browser.app.entering_first() {
        browser.entry_scroll = None;
    }
    if outcome.effect == Some(Effect::Reveal) {
        result = result.and_then(|()| browser.reveal_caret());
    }
    let canvas = browser.canvas.clone();
    drop(borrowed);
    match outcome.effect.take() {
        Some(Effect::Focus) => result = result.and_then(|()| canvas.focus()),
        Some(Effect::Send(request)) => {
            let shared = Rc::clone(shared);
            spawn_local(async move {
                let result = sign_up(request).await;
                dispatch(&shared, Event::Response(result));
            });
        }
        effect => outcome.effect = effect,
    }
    if let Err(error) = result {
        web_sys::console::error_1(&error);
    }
    Some(outcome)
}

fn on<T: JsCast>(
    target: &EventTarget,
    kind: &str,
    shared: &Shared,
    handler: impl Fn(&T) -> Option<Event> + 'static,
) -> Result<(), JsValue> {
    let shared = Rc::clone(shared);
    let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
        let Some(event_now) = event.dyn_ref::<T>() else {
            return;
        };
        let Some(outcome) = handler(event_now).and_then(|app_event| dispatch(&shared, app_event))
        else {
            return;
        };
        if let Some(Effect::Clipboard(text)) = &outcome.effect
            && let Some(data) = event.unchecked_ref::<ClipboardEvent>().clipboard_data()
        {
            let _ = data.set_data("text/plain", text);
        }
        // The browser's own action for the key or clipboard event, such as
        // leaving the page for Tab, runs unless the app used it; a mouse
        // button's must run, to focus the canvas.
        if outcome.handled && !event.type_().starts_with("mouse") {
            event.prevent_default();
        }
    });
    target.add_event_listener_with_callback(kind, closure.as_ref().unchecked_ref())?;
    // The listeners live as long as the page.
    closure.forget();
    Ok(())
}

fn key(event: &KeyboardEvent) -> Key {
    Key {
        key: event.key(),
        shift: event.shift_key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        meta: event.meta_key(),
    }
}

/// Where a mouse event is on the canvas, which the page scrolls with.
fn point(canvas: &HtmlCanvasElement, event: &MouseEvent) -> (f64, f64) {
    let rect = canvas.get_bounding_client_rect();
    (
        f64::from(event.client_x()) - rect.left(),
        f64::from(event.client_y()) - rect.top(),
    )
}

fn listen(shared: &Shared) -> Result<(), JsValue> {
    let browser = shared.borrow();
    let canvas = browser.canvas.clone();
    let window = browser.window.clone();
    on(&canvas, "focus", shared, |_: &web_sys::Event| {
        Some(Event::Focus)
    })?;
    on(&canvas, "blur", shared, |_: &web_sys::Event| {
        Some(Event::Blur)
    })?;
    on(&canvas, "keydown", shared, |event: &KeyboardEvent| {
        // Composition, with an input method, is the browser's.
        (!event.is_composing()).then(|| Event::Key(key(event)))
    })?;
    on(&canvas, "keyup", shared, |event: &KeyboardEvent| {
        Some(Event::KeyUp(key(event)))
    })?;
    let target = canvas.clone();
    on(&canvas, "mousedown", shared, move |event: &MouseEvent| {
        let (x, y) = point(&target, event);
        (event.button() == 0).then(|| Event::Press {
            x,
            y,
            shift: event.shift_key(),
            clicks: u32::try_from(event.detail()).unwrap_or(1),
        })
    })?;
    // The window keeps receiving the mouse while a button drags past the canvas.
    let target = canvas.clone();
    on(&window, "mouseup", shared, move |event: &MouseEvent| {
        let (x, y) = point(&target, event);
        (event.button() == 0).then_some(Event::Release { x, y })
    })?;
    let target = canvas.clone();
    on(&window, "mousemove", shared, move |event: &MouseEvent| {
        Some(Event::Pointer(Some(point(&target, event))))
    })?;
    on(&canvas, "mouseleave", shared, |_: &MouseEvent| {
        Some(Event::Pointer(None))
    })?;
    on(&canvas, "copy", shared, |_: &ClipboardEvent| {
        Some(Event::Copy)
    })?;
    on(&canvas, "cut", shared, |_: &ClipboardEvent| {
        Some(Event::Cut)
    })?;
    on(&canvas, "paste", shared, |event: &ClipboardEvent| {
        let data = event.clipboard_data()?;
        Some(Event::Paste(data.get_data("text/plain").ok()?))
    })?;

    // The layout follows the viewport, whose height alone can change.
    on(&window, "resize", shared, |_: &web_sys::Event| {
        Some(Event::Resize)
    })?;
    let resized = Rc::clone(shared);
    let observer = Closure::<dyn Fn()>::new(move || {
        dispatch(&resized, Event::Resize);
    });
    let html = browser.document.document_element().ok_or("no <html>")?;
    web_sys::ResizeObserver::new(observer.as_ref().unchecked_ref())?.observe(&html);
    observer.forget();

    let blinking = Rc::clone(shared);
    let blink = Closure::<dyn Fn()>::new(move || {
        let editing = blinking
            .try_borrow()
            .is_ok_and(|browser| browser.app.editing().is_some());
        if editing {
            dispatch(&blinking, Event::Blink);
        }
    });
    let framed = Rc::clone(shared);
    let frame = Closure::<dyn Fn()>::new(move || {
        if let Ok(mut browser) = framed.try_borrow_mut() {
            browser.framing = false;
        }
        dispatch(&framed, Event::Autoscroll);
    });
    drop(browser);
    let mut browser = shared.borrow_mut();
    browser.blink = Some(blink.into_js_value().unchecked_into());
    browser.frame = Some(frame.into_js_value().unchecked_into());
    browser.restart_blink()
}

/// Calls the backend over gRPC-web on the page's own origin, which proxies
/// it to the backend.
async fn sign_up(request: SignUpRequest) -> Result<SignUpResponse, CallError> {
    let response = fetch(&grpc_web::encode(&request)).await;
    let Ok((http_status, grpc_status, grpc_message, body)) = response else {
        return Err(CallError::Http(0));
    };
    grpc_web::decode(RawResponse {
        http_status,
        grpc_status: grpc_status.as_deref(),
        grpc_message: grpc_message.as_deref(),
        body: &body,
    })
}

type Fetched = (u16, Option<String>, Option<String>, Vec<u8>);

async fn fetch(body: &[u8]) -> Result<Fetched, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&Uint8Array::from(body));
    let request = Request::new_with_str_and_init(SIGN_UP, &init)?;
    let headers = request.headers();
    headers.set("content-type", grpc_web::CONTENT_TYPE)?;
    headers.set("x-grpc-web", "1")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await?
        .dyn_into()?;
    let headers = response.headers();
    let grpc_status = headers.get("grpc-status")?;
    let grpc_message = headers.get("grpc-message")?;
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    Ok((
        response.status(),
        grpc_status,
        grpc_message,
        Uint8Array::new(&bytes).to_vec(),
    ))
}

/// Draws on a 2D context whose transform maps CSS pixels to device pixels.
struct CanvasPainter<'a> {
    context: &'a CanvasRenderingContext2d,
    /// Device pixels per CSS pixel; shadows ignore the transform.
    ratio: f64,
}

impl CanvasPainter<'_> {
    /// `rect` on whole device pixels, at least one wide and high unless it
    /// is empty, as Chrome paints boxes, carets and highlights and clips.
    fn snap(&self, rect: Rect) -> Rect {
        let snap = |value: f64| (value * self.ratio).round() / self.ratio;
        let size = |start: f64, size: f64| {
            if size > 0.0 {
                (snap(start + size) - snap(start)).max(1.0 / self.ratio)
            } else {
                0.0
            }
        };
        Rect::new(
            snap(rect.x),
            snap(rect.y),
            size(rect.x, rect.width),
            size(rect.y, rect.height),
        )
    }
}

impl Painter for CanvasPainter<'_> {
    fn fill_rect(&mut self, rect: Rect, radius: f64, color: &str, shadow: Option<Shadow>) {
        let rect = self.snap(rect);
        let context = self.context;
        context.save();
        if let Some(shadow) = shadow {
            context.set_shadow_color(shadow.color);
            context.set_shadow_blur(shadow.blur * self.ratio);
            context.set_shadow_offset_x(shadow.x * self.ratio);
            context.set_shadow_offset_y(shadow.y * self.ratio);
        }
        context.set_fill_style_str(color);
        context.begin_path();
        if radius > 0.0 {
            // Infallible for a single non-negative radius.
            let _ = context.round_rect_with_f64(rect.x, rect.y, rect.width, rect.height, radius);
        } else {
            context.rect(rect.x, rect.y, rect.width, rect.height);
        }
        context.fill();
        context.restore();
    }

    fn stroke_path(&mut self, path: &str, origin: (f64, f64), width: f64, color: &str) {
        let Ok(path) = Path2d::new_with_path_string(path) else {
            return;
        };
        let context = self.context;
        context.save();
        let _ = context.translate(origin.0, origin.1);
        context.set_line_width(width);
        context.set_line_cap("round");
        context.set_line_join("round");
        context.set_stroke_style_str(color);
        context.stroke_with_path(&path);
        context.restore();
    }

    fn fill_text(&mut self, text: &str, x: f64, baseline: f64, style: TextStyle, color: &str) {
        let context = self.context;
        context.set_font(&style.font());
        context.set_text_baseline("alphabetic");
        context.set_fill_style_str(color);
        let _ = context.fill_text(text, x, baseline);
    }

    fn clip(&mut self, rect: Rect) {
        let rect = self.snap(rect);
        self.context.save();
        self.context.begin_path();
        self.context.rect(rect.x, rect.y, rect.width, rect.height);
        self.context.clip();
    }

    fn restore(&mut self) {
        self.context.restore();
    }
}

/// How many widths [`CanvasMeasure`] keeps; caret positions measure pieces
/// of an input's value, so typing adds new ones.
const CACHED_WIDTHS: usize = 4096;
/// How many texts' character bounds [`CanvasMeasure`] keeps: enough for the
/// inputs, so repainting an unchanged one, as the caret blinks, measures
/// nothing.
const CACHED_BOUNDS: usize = 8;

type Bounds = ((String, String), Vec<(f64, f64)>);

/// Measures with the canvas, which shapes text exactly as it paints it.
struct CanvasMeasure {
    context: CanvasRenderingContext2d,
    widths: RefCell<HashMap<(String, String), f64>>,
    /// The most recently placed texts last.
    bounds: RefCell<Vec<Bounds>>,
    words: Option<Intl::Segmenter>,
    graphemes: Option<Intl::Segmenter>,
}

impl CanvasMeasure {
    fn new(context: CanvasRenderingContext2d) -> Self {
        Self {
            context,
            widths: RefCell::default(),
            bounds: RefCell::default(),
            words: segmenter("word"),
            graphemes: segmenter("grapheme"),
        }
    }
}

fn segmenter(granularity: &str) -> Option<Intl::Segmenter> {
    let options = Object::new();
    Reflect::set(&options, &"granularity".into(), &granularity.into())
        .ok()
        .map(|_| Intl::Segmenter::new(&Array::new(), &options))
}

/// The pieces `segmenter` splits `text` into, or `text` whole.
fn segments(segmenter: Option<&Intl::Segmenter>, text: &str) -> Vec<String> {
    let Some(segmenter) = segmenter else {
        return vec![text.to_owned()];
    };
    let Ok(Some(segments)) = js_sys::try_iter(&segmenter.segment(text)) else {
        return vec![text.to_owned()];
    };
    segments
        .filter_map(Result::ok)
        .filter_map(|segment| Reflect::get(&segment, &"segment".into()).ok()?.as_string())
        .collect()
}

impl Measure for CanvasMeasure {
    fn width(&self, text: &str, style: TextStyle) -> f64 {
        let key = (style.font(), text.to_owned());
        if let Some(&width) = self.widths.borrow().get(&key) {
            return width;
        }
        self.context.set_font(&key.0);
        let width = self
            .context
            .measure_text(text)
            .map_or(0.0, |metrics| metrics.width());
        let mut widths = self.widths.borrow_mut();
        if widths.len() >= CACHED_WIDTHS {
            widths.clear();
        }
        widths.insert(key, width);
        width
    }

    /// The DOM rounds the font's ascent and descent to whole pixels.
    fn metrics(&self, style: TextStyle) -> FontMetrics {
        self.context.set_font(&style.font());
        self.context.measure_text("").map_or(
            FontMetrics {
                ascent: 0.0,
                descent: 0.0,
            },
            |metrics| FontMetrics {
                ascent: metrics.font_bounding_box_ascent().round(),
                descent: metrics.font_bounding_box_descent().round(),
            },
        )
    }

    fn words(&self, text: &str) -> Vec<String> {
        segments(self.words.as_ref(), text)
    }

    fn graphemes(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        segments(self.graphemes.as_ref(), text)
    }

    fn char_bounds(&self, text: &str, style: TextStyle) -> Vec<(f64, f64)> {
        let key = (style.font(), text.to_owned());
        if let Some((_, bounds)) = self
            .bounds
            .borrow()
            .iter()
            .find(|(cached, _)| *cached == key)
        {
            return bounds.clone();
        }
        let bounds = crate::text::char_bounds(text, style, self);
        let mut cache = self.bounds.borrow_mut();
        if cache.len() >= CACHED_BOUNDS {
            cache.remove(0);
        }
        cache.push((key, bounds.clone()));
        bounds
    }
}
