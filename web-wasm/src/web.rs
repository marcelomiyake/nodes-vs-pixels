//! Browser glue: paints [`scene`] on a canvas, keeps the [`overlay`] controls
//! on top of it and feeds their events to [`App`]. The logic it drives is
//! tested natively; this module only talks to the DOM.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use form_core::Field;
use js_sys::{Array, Function, Intl, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    CanvasRenderingContext2d, Document, EventTarget, HtmlCanvasElement, HtmlElement,
    HtmlInputElement, Path2d, Request, RequestInit, Response, Window,
};

use crate::app::{App, Effect, Event};
use crate::grpc_web::{self, CallError, RawResponse};
use crate::layout::{BORDER, Rect, TEXT_FIELDS, Viewport, layout};
use crate::overlay::{self, Prop, SUBMIT, Update};
use crate::proto::{SignUpRequest, SignUpResponse};
use crate::scene::{self, Control, Painter, char_offset};
use crate::style::{Shadow, TextStyle};
use crate::text::{FontMetrics, Measure};
use crate::tokens::{FONT_FALLBACK, FONT_FAMILY, spacing, text};

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
    let root: HtmlElement = document
        .get_element_by_id("app")
        .ok_or("no #app")?
        .dyn_into()?;
    let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
    canvas.set_attribute("aria-hidden", "true")?;
    root.append_child(&canvas)?;
    // An opaque canvas lets Chrome antialias text with subpixels, as in the DOM.
    let options = Object::new();
    Reflect::set(&options, &"alpha".into(), &false.into())?;
    let context: CanvasRenderingContext2d = canvas
        .get_context_with_context_options("2d", &options)?
        .ok_or("no 2d context")?
        .dyn_into()?;
    let elements = build_overlay(&document, &root)?;
    // Lays out an input's text as the native control does, so the caret and
    // selection land where they would in bidirectional text. Its empty,
    // clipping container keeps long text from widening the page.
    let clip = document.create_element("div")?;
    clip.set_attribute("aria-hidden", "true")?;
    clip.set_attribute(
        "style",
        "position:absolute;top:0;left:0;width:0;height:0;overflow:hidden",
    )?;
    let probe: HtmlElement = document.create_element("span")?.dyn_into()?;
    probe.set_attribute(
        "style",
        "position:absolute;top:0;left:0;visibility:hidden;white-space:pre",
    )?;
    clip.append_child(&probe)?;
    root.append_child(&clip)?;
    let hover = window
        .match_media("(hover: hover)")?
        .is_some_and(|query| query.matches());
    let browser = Browser {
        measure: CanvasMeasure::new(context.clone(), document.clone(), probe),
        window,
        document,
        root,
        canvas,
        context,
        elements,
        app: App::default(),
        blink: None,
        blink_timer: 0,
    };
    let shared = Rc::new(RefCell::new(browser));
    listen(&shared, hover)?;
    shared.borrow_mut().render()
}

struct Browser {
    window: Window,
    document: Document,
    root: HtmlElement,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    measure: CanvasMeasure,
    elements: HashMap<String, HtmlElement>,
    app: App,
    /// Toggles the painted caret, every [`BLINK_MS`] from `blink_timer`'s start.
    blink: Option<Function>,
    blink_timer: i32,
}

impl Browser {
    fn element(&self, id: &str) -> Result<HtmlElement, JsValue> {
        self.elements
            .get(id)
            .cloned()
            .ok_or_else(|| JsValue::from_str(&format!("no overlay element #{id}")))
    }

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

    fn render(&mut self) -> Result<(), JsValue> {
        let html = self.document.document_element().ok_or("no <html>")?;
        let viewport = Viewport {
            width: f64::from(html.client_width()),
            height: self.window.inner_height()?.as_f64().unwrap_or_default(),
            media_width: self.window.inner_width()?.as_f64().unwrap_or_default(),
        };
        let page = layout(viewport, &self.app.state, &self.measure);
        self.root.style().set_property("height", &px(page.height))?;

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
        self.context
            .set_transform(ratio, 0.0, 0.0, ratio, 0.0, 0.0)?;
        let mut painter = CanvasPainter {
            context: &self.context,
            ratio,
        };
        scene::paint(
            &page,
            &self.app.state,
            &self.app.ui,
            &self.measure,
            &mut painter,
        );
        self.apply(overlay::updates(&page, &self.app.state))
    }

    fn apply(&self, updates: Vec<Update>) -> Result<(), JsValue> {
        for update in updates {
            let element = self.element(&update.id)?;
            for prop in update.props {
                match prop {
                    Prop::Rect(Some(rect)) => {
                        element.set_hidden(false);
                        place(&element, rect)?;
                    }
                    Prop::Rect(None) => element.set_hidden(true),
                    Prop::Text(content) => {
                        if element.text_content().as_deref() != Some(content.as_str()) {
                            element.set_text_content(Some(&content));
                        }
                    }
                    Prop::Attribute(name, Some(value)) => element.set_attribute(name, &value)?,
                    Prop::Attribute(name, None) => element.remove_attribute(name)?,
                    Prop::Value(value) => {
                        let input: &HtmlInputElement = element.unchecked_ref();
                        if input.value() != value {
                            input.set_value(&value);
                        }
                    }
                    Prop::Checked(checked) => {
                        element
                            .unchecked_ref::<HtmlInputElement>()
                            .set_checked(checked);
                    }
                }
            }
        }
        Ok(())
    }
}

fn px(value: f64) -> String {
    format!("{value}px")
}

fn place(element: &HtmlElement, rect: Rect) -> Result<(), JsValue> {
    let style = element.style();
    style.set_property("left", &px(rect.x))?;
    style.set_property("top", &px(rect.y))?;
    style.set_property("width", &px(rect.width))?;
    style.set_property("height", &px(rect.height))
}

fn css_font(style: TextStyle) -> String {
    format!(
        "{} {}px/{}px {FONT_FAMILY}, {FONT_FALLBACK}",
        style.weight, style.size, style.line_height
    )
}

/// Creates the transparent controls, styled like the React app's elements
/// so that input scrolling, caret and selection offsets match its own.
fn build_overlay(
    document: &Document,
    root: &HtmlElement,
) -> Result<HashMap<String, HtmlElement>, JsValue> {
    let mut elements: HashMap<String, HtmlElement> = HashMap::new();
    for template in overlay::templates() {
        let element: HtmlElement = document.create_element(template.tag)?.dyn_into()?;
        element.set_id(&template.id);
        element.set_class_name("overlay");
        for (name, value) in &template.attributes {
            element.set_attribute(name, value)?;
        }
        if !template.text.is_empty() {
            element.set_text_content(Some(template.text));
        }
        if let Some(style) = template.style {
            element.style().set_property("font", &css_font(style))?;
        }
        if template.tag == "input" && template.style.is_some() {
            let style = element.style();
            style.set_property("padding", &format!("0 {}px", spacing::CONTROL_X))?;
            style.set_property("border", &format!("{BORDER}px solid"))?;
        }
        let parent = template.parent.and_then(|id| elements.get(id));
        parent
            .map_or(root, |parent| parent)
            .append_child(&element)?;
        elements.insert(template.id, element);
    }
    Ok(elements)
}

/// Handles `event` with the browser state released before any DOM call that
/// could fire further events synchronously.
fn dispatch(shared: &Shared, event: Event) {
    let Ok(mut browser) = shared.try_borrow_mut() else {
        // Fired by a DOM change made while handling another event.
        let shared = Rc::clone(shared);
        spawn_local(async move { dispatch(&shared, event) });
        return;
    };
    let restart = event.shows_caret();
    let effect = browser.app.handle(event);
    let rendered = browser.render().and_then(|()| {
        if restart {
            browser.restart_blink()
        } else {
            Ok(())
        }
    });
    let focus = match &effect {
        Some(Effect::Focus(id)) => browser.element(id).ok(),
        _ => None,
    };
    drop(browser);
    if let Err(error) = rendered.and_then(|()| focus.map_or(Ok(()), |element| element.focus())) {
        web_sys::console::error_1(&error);
    }
    if let Some(Effect::Send(request)) = effect {
        let shared = Rc::clone(shared);
        spawn_local(async move {
            let result = sign_up(request).await;
            dispatch(&shared, Event::Response(result));
        });
    }
}

fn on(
    target: &EventTarget,
    kind: &str,
    shared: &Shared,
    handler: impl Fn(&web_sys::Event) -> Option<Event> + 'static,
) -> Result<(), JsValue> {
    let shared = Rc::clone(shared);
    let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
        if let Some(event) = handler(&event) {
            dispatch(&shared, event);
        }
    });
    target.add_event_listener_with_callback(kind, closure.as_ref().unchecked_ref())?;
    // The listeners live as long as the page.
    closure.forget();
    Ok(())
}

fn listen(shared: &Shared, hover: bool) -> Result<(), JsValue> {
    let browser = shared.borrow();
    let mut controls: Vec<(HtmlElement, Control)> = Field::ALL
        .into_iter()
        .map(|field| Ok((browser.element(field.key())?, Control::Field(field))))
        .collect::<Result<_, JsValue>>()?;
    let button = browser.element(SUBMIT)?;
    controls.push((button.clone(), Control::Button));
    for (element, control) in controls {
        let target = element.clone();
        on(&element, "focus", shared, move |_| {
            let visible = target.matches(":focus-visible").unwrap_or(false);
            Some(Event::Focus { control, visible })
        })?;
        on(&element, "blur", shared, move |_| {
            Some(Event::Blur(control))
        })?;
    }

    for field in TEXT_FIELDS {
        let input: HtmlInputElement = browser.element(field.key())?.dyn_into()?;
        let target = input.clone();
        on(&input, "input", shared, move |_| {
            Some(Event::Input(field, target.value()))
        })?;
        // A trackpad can scroll the text without changing the selection.
        let document = browser.document.clone();
        on(&input, "scroll", shared, move |_| caret(&document))?;
    }
    let document = browser.document.clone();
    on(&browser.document, "selectionchange", shared, move |_| {
        caret(&document)
    })?;

    let terms: HtmlInputElement = browser.element(Field::AcceptTerms.key())?.dyn_into()?;
    let target = terms.clone();
    on(&terms, "change", shared, move |_| {
        Some(Event::Toggle(target.checked()))
    })?;
    if hover {
        on(&button, "pointerenter", shared, |_| {
            Some(Event::Hover(true))
        })?;
        on(&button, "pointerleave", shared, |_| {
            Some(Event::Hover(false))
        })?;
    }
    let form = browser.element("form")?;
    on(&form, "submit", shared, |event| {
        event.prevent_default();
        Some(Event::Submit)
    })?;

    // The layout follows the viewport, whose height alone can change.
    on(&browser.window, "resize", shared, |_| Some(Event::Resize))?;
    let resized = Rc::clone(shared);
    let observer = Closure::<dyn Fn()>::new(move || dispatch(&resized, Event::Resize));
    let html = browser.document.document_element().ok_or("no <html>")?;
    web_sys::ResizeObserver::new(observer.as_ref().unchecked_ref())?.observe(&html);
    observer.forget();

    let blinking = Rc::clone(shared);
    let blink = Closure::<dyn Fn()>::new(move || {
        let editing = blinking.try_borrow().is_ok_and(|browser| {
            matches!(browser.app.ui.focus, Some(Control::Field(field)) if field != Field::AcceptTerms)
        });
        if editing {
            dispatch(&blinking, Event::Blink);
        }
    });
    drop(browser);
    let mut browser = shared.borrow_mut();
    browser.blink = Some(blink.into_js_value().unchecked_into());
    browser.restart_blink()
}

/// The selection and scroll position of the focused text input.
fn caret(document: &Document) -> Option<Event> {
    let input: HtmlInputElement = document.active_element()?.dyn_into().ok()?;
    if input.type_() == "checkbox" {
        return None;
    }
    let value = input.value();
    let offset = |units: Option<u32>| char_offset(&value, units.unwrap_or(0) as usize);
    let selection = (
        offset(input.selection_start().ok()?),
        offset(input.selection_end().ok()?),
    );
    let scroll_left = f64::from(input.scroll_left());
    Some(Event::Caret {
        selection,
        scroll_left,
    })
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

impl Painter for CanvasPainter<'_> {
    fn fill_rect(&mut self, rect: Rect, radius: f64, color: &str, shadow: Option<Shadow>) {
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
        self.context.save();
        self.context.begin_path();
        self.context.rect(rect.x, rect.y, rect.width, rect.height);
        self.context.clip();
    }

    fn restore(&mut self) {
        self.context.restore();
    }
}

/// Measures with the canvas, which shapes text exactly as it paints it, and
/// places carets with a hidden element that lays text out like an input.
struct CanvasMeasure {
    context: CanvasRenderingContext2d,
    widths: RefCell<HashMap<(String, String), f64>>,
    words: Option<Intl::Segmenter>,
    graphemes: Option<Intl::Segmenter>,
    document: Document,
    probe: HtmlElement,
}

impl CanvasMeasure {
    fn new(context: CanvasRenderingContext2d, document: Document, probe: HtmlElement) -> Self {
        Self {
            context,
            widths: RefCell::default(),
            words: segmenter("word"),
            graphemes: segmenter("grapheme"),
            document,
            probe,
        }
    }

    fn probe_bounds(&self, text: &str, style: TextStyle) -> Result<Vec<(f64, f64)>, JsValue> {
        self.probe.style().set_property("font", &css_font(style))?;
        self.probe.set_text_content(Some(text));
        let Some(node) = self.probe.first_child() else {
            return Ok(Vec::new());
        };
        let left = self.probe.get_bounding_client_rect().left();
        let range = self.document.create_range()?;
        let mut offset = 0;
        text.chars()
            .map(|c| {
                let units = u32::try_from(c.len_utf16()).unwrap_or(2);
                range.set_start(&node, offset)?;
                range.set_end(&node, offset + units)?;
                offset += units;
                let rect = range.get_bounding_client_rect();
                Ok((rect.left() - left, rect.right() - left))
            })
            .collect()
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
        self.widths.borrow_mut().insert(key, width);
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

    fn dictionary_words(&self, run: &str) -> Vec<String> {
        segments(self.words.as_ref(), run)
    }

    fn graphemes(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        segments(self.graphemes.as_ref(), text)
    }

    fn char_bounds(&self, text: &str, style: TextStyle) -> Vec<(f64, f64)> {
        self.probe_bounds(text, style).unwrap_or_default()
    }
}
