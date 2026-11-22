//! The control that draws a canvas's scene ([UI-SCENE], [NATIVE-SHM-SCENE]): every checked scene
//! row drawn back to front with FerroUI's drawing context (on the GPU through the platform's
//! renderer), and the pointer, keys and the region's size sent back to the control number the
//! framework issued, in scene units, as typed host events over the shared-memory boundary.
//!
//! Placement follows Osprey's GPUI host: the scene's origin at the control's top left at its
//! `zoom`, never magnified past what the region fits whole; zoom 0 fits it whole; a negative zoom is
//! that scale exactly, whatever the region's size, so resizing the window never resizes what is drawn
//! ([DAW-LAYOUT-FIXED-SCALE]); a scene of zero width is in window pixels.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use ferroui_base::input::{
    InputElement, InputElementImpl, Key, KeyEventArgs, KeyModifiers, PointerEventArgs,
    PointerPressedEventArgs, PointerReleasedEventArgs,
};
use ferroui_base::interactivity::InteractiveImpl;
use ferroui_base::layout::{HorizontalAlignment, LayoutableImpl, VerticalAlignment};
use ferroui_base::media::imaging::Bitmap;
use ferroui_base::media::{
    Color, DashStyle, DrawingContext, FlowDirection, FontFamily, FontStretch, FontStyle,
    FontWeight, FormattedText, GradientStop, GradientStops, IBrush, IPen, LinearGradientBrush,
    MediaContext, Pen, PenLineCap, PenLineJoin, SolidColorBrush, StreamGeometry,
    StreamGeometryContext, SweepDirection, Typeface,
};
use ferroui_base::platform::IGeometryContext;
use ferroui_base::utilities::Locale;
use ferroui_base::{
    ferro_class, ferro_class_info, ferro_impl_classes, ferro_properties, instantiate,
    FerroObjectImpl, FerroProperty, Matrix, Point, Rect, Ref, RelativePoint, RelativeUnit, Size,
    StyledElementImpl, StyledProperty, Visual, VisualImpl,
};
use ferroui_controls::{Control, ControlImpl};
use ferroui_view::prelude::*;
use ferroui_view::ViewClass;

use crate::events::Touch;
use crate::host::Host;
use crate::rows::Scene;
use crate::scene::{Access, Outline, Paint, Role, Shape, Step, Tint};

/// A scene ready to draw: its extent, zoom, atlas and shapes stably sorted back to front.
#[derive(Default)]
pub struct Prepared {
    width: f64,
    height: f64,
    zoom: f64,
    atlas: Vec<String>,
    shapes: Vec<Shape>,
    controls: Vec<Access>,
}

impl Prepared {
    fn of(scene: &Scene) -> Self {
        let mut shapes = scene.shapes.clone();
        shapes.sort_by(|left, right| left.depth().total_cmp(&right.depth()));
        Self {
            width: f64::from(scene.width),
            height: f64::from(scene.height),
            zoom: f64::from(scene.zoom),
            atlas: scene.atlas.clone(),
            shapes,
            controls: scene
                .access
                .iter()
                .filter(|area| area.role != Role::Label)
                .cloned()
                .collect(),
        }
    }

    /// Whether a control the user can press covers `(x, y)`, in scene units.
    fn control_at(&self, x: f64, y: f64) -> bool {
        self.controls
            .iter()
            .any(|area| x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h)
    }

    fn in_window_pixels(&self) -> bool {
        self.width <= 0.0
    }
}

/// The scene drawn; equal only to itself, so a new frame's scene always draws.
#[derive(Clone, Default)]
pub struct SceneData(pub Rc<Prepared>);

impl PartialEq for SceneData {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where the scene's input goes: the host and the control number the framework issued.
#[derive(Clone, Default)]
pub struct SceneLink {
    host: Weak<Host>,
    ordinal: i64,
}

impl PartialEq for SceneLink {
    fn eq(&self, other: &Self) -> bool {
        self.ordinal == other.ordinal && Weak::ptr_eq(&self.host, &other.host)
    }
}

impl SceneLink {
    fn send(&self, touch: impl FnOnce(i64) -> Touch) {
        if let Some(host) = self.host.upgrade() {
            host.send(touch(self.ordinal));
        }
    }
}

/// The canvas element for the scene at view row `row`, delivering input to control `ordinal`.
pub fn scene_element(host: &Rc<Host>, _row: usize, ordinal: i64, scene: &Scene) -> Element {
    let link = SceneLink {
        host: Rc::downgrade(host),
        ordinal,
    };
    El::<SceneSurface>::new()
        .set(SceneSurface::scene_property(), SceneData(Rc::new(Prepared::of(scene))))
        .set(SceneSurface::link_property(), link)
        .clip_to_bounds(true)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Stretch)
        .into()
}

/// Draws one scene and sends its input.
#[repr(C)]
pub struct SceneSurface {
    base: Control,
    /// The region's size in scene units the application was last told.
    told: Rc<Cell<Option<(f64, f64)>>>,
    pressed: Cell<bool>,
    /// The latest pointer move not yet sent: moves are coalesced to one per animation frame, so a
    /// slow application never falls behind a fast mouse and its other events (an audio worker's
    /// exchange) are not queued behind stale moves.
    moved: Rc<RefCell<Option<Touch>>>,
}

ferro_class!(SceneSurface: Control);
ferro_impl_classes!(SceneSurface: FerroObjectImpl, StyledElementImpl, InteractiveImpl);
ferro_class_info!(SceneSurface {
    new: SceneSurface::new
});

ferro_properties! {
    impl SceneSurface {
        /// The scene drawn.
        pub fn scene_property() -> StyledProperty<SceneData> {
            FerroProperty::register::<SceneSurface, _>("Scene", SceneData::default())
        }

        /// Where input goes.
        pub fn link_property() -> StyledProperty<SceneLink> {
            FerroProperty::register::<SceneSurface, _>("Link", SceneLink::default())
        }
    }
}

impl ViewClass for SceneSurface {
    fn create() -> Ref<Self> {
        Self::new()
    }
}

/// Whether the application draws the whole window itself (`OSPREY_NATIVE_CHROME=bare`): its
/// content runs under a transparent title bar, as the GPUI host's bare chrome does.
pub(crate) fn bare() -> bool {
    std::env::var("OSPREY_NATIVE_CHROME").is_ok_and(|chrome| chrome == "bare")
}

/// The height of a bare window's title bar, `OSPREY_NATIVE_TITLE_BAR` in points (default -1, the
/// system's): the close, minimise and zoom buttons are centred in it, so a bar of 50 points puts
/// their top 18 points down, where the GPUI host's `OSPREY_GPUI_LIGHTS=16,18` puts them.
pub(crate) fn title_bar() -> f64 {
    std::env::var("OSPREY_NATIVE_TITLE_BAR")
        .ok()
        .and_then(|height| height.parse().ok())
        .unwrap_or(-1.0)
}

/// The system's title bar height in points, used when the application names none.
const SYSTEM_TITLE_BAR: f64 = 28.0;

/// The height of the strip a bare window is moved by, or `None` when the window keeps the
/// system's title bar, which moves it already.
fn caption() -> Option<f64> {
    let height = title_bar();
    bare().then_some(if height > 0.0 { height } else { SYSTEM_TITLE_BAR })
}

/// Window pixels per scene unit, and the region's extent in scene units.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    scale: f64,
    viewed: (f64, f64),
}

impl Placement {
    fn of(scene: &Prepared, size: Size) -> Self {
        if scene.in_window_pixels() {
            return Self {
                scale: 1.0,
                viewed: (size.width, size.height),
            };
        }
        let whole = (size.width / scene.width).min(size.height / scene.height);
        let scale = match scene.zoom {
            zoom if zoom > 0.0 => zoom.min(whole),
            zoom if zoom < 0.0 => -zoom,
            _ => whole,
        };
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        Self {
            scale,
            viewed: (size.width / scale, size.height / scale),
        }
    }
}

impl SceneSurface {
    fn static_constructor() {
        Visual::affects_render::<SceneSurface>(&[Self::scene_property().as_property()]);
    }

    pub fn construct() -> Self {
        Self {
            base: Control::construct(),
            told: Rc::new(Cell::new(None)),
            pressed: Cell::new(false),
            moved: Rc::new(RefCell::new(None)),
        }
    }

    pub fn new() -> Ref<Self> {
        let this = instantiate(Self::construct());
        this.set_focusable(true);
        this
    }

    fn scene(&self) -> Rc<Prepared> {
        self.get_value(Self::scene_property()).0
    }

    fn link(&self) -> SceneLink {
        self.get_value(Self::link_property())
    }

    fn placement(&self) -> Placement {
        Placement::of(&self.scene(), self.bounds().size())
    }

    /// Tells the application the region's size in scene units when it changed, on the next
    /// animation frame: never from inside a render.
    fn tell_size(&self) {
        let scene = self.scene();
        let size = self.bounds().size();
        if scene.in_window_pixels() || size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let viewed = Placement::of(&scene, size).viewed;
        if self.told.replace(Some(viewed)) == Some(viewed) {
            return;
        }
        let link = self.link();
        MediaContext::instance().request_animation_frame(move |_| {
            link.send(|ordinal| Touch::viewed(ordinal, viewed.0, viewed.1));
        });
    }

    /// A bare window has no title bar of its own, so a press in the title strip that lands on no
    /// control of the scene moves the window; the operating system owns the gesture from there.
    fn moves_window(&self, e: &PointerPressedEventArgs) -> bool {
        let Some(strip) = caption() else {
            return false;
        };
        let scale = self.placement().scale;
        let at = e.get_position(Some(self));
        if e.get_position(None).y >= strip || self.scene().control_at(at.x / scale, at.y / scale) {
            return false;
        }
        let window = ferroui_controls::TopLevel::get_top_level(Some(self))
            .and_then(|top| top.cast::<ferroui_controls::Window>());
        window.is_some_and(|window| {
            window.begin_move_drag(e);
            true
        })
    }

    fn touch(&self, position: Point, down: bool, modifiers: KeyModifiers) -> Touch {
        let scale = self.placement().scale;
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let ordinal = self.link().ordinal;
        Touch::pointer(ordinal, position.x / scale, position.y / scale, down, shift)
    }

    /// Sends a press or release at once, after any move still waiting, so the order holds.
    fn pointer(&self, position: Point, down: bool, modifiers: KeyModifiers) {
        let touch = self.touch(position, down, modifiers);
        let waiting = self.moved.borrow_mut().take();
        let link = self.link();
        if let Some(waiting) = waiting {
            link.send(|_| waiting);
        }
        link.send(|_| touch);
    }

    /// Keeps the latest move and sends it on the next animation frame.
    fn coalesce(&self, touch: Touch) {
        if self.moved.borrow_mut().replace(touch).is_some() {
            return;
        }
        let (moved, link) = (self.moved.clone(), self.link());
        MediaContext::instance().request_animation_frame(move |_| {
            let waiting = moved.borrow_mut().take();
            if let Some(touch) = waiting {
                link.send(|_| touch);
            }
        });
    }
}

impl ControlImpl for SceneSurface {}

impl LayoutableImpl for SceneSurface {
    /// A canvas takes the space its region gives it; unbounded, it asks for its size at its zoom.
    fn measure_override(this: &Self, available: Size) -> Size {
        let scene = this.scene();
        let zoom = if scene.zoom == 0.0 { 1.0 } else { scene.zoom.abs() };
        let length = |given: f64, natural: f64| if given.is_finite() { given } else { natural };
        Size::new(
            length(available.width, scene.width * zoom).max(0.0),
            length(available.height, scene.height * zoom).max(0.0),
        )
    }
}

impl InputElementImpl for SceneSurface {
    fn on_pointer_pressed(this: &Self, e: &PointerPressedEventArgs) {
        e.set_handled(true);
        if e.get_current_point(Some(this)).properties.is_right_button_pressed
            || (cfg!(target_os = "macos") && e.key_modifiers().contains(KeyModifiers::CONTROL))
        {
            let at = e.get_position(Some(this));
            let scale = this.placement().scale;
            this.link().send(|ordinal| Touch::secondary(ordinal, at.x / scale, at.y / scale));
            return;
        }
        if this.moves_window(e) {
            return;
        }
        this.focus();
        let element: Ref<InputElement> = this.to_ref().upcast();
        e.pointer().capture(Some(&element));
        this.pressed.set(true);
        this.pointer(e.get_position(Some(this)), true, e.key_modifiers());
    }

    /// Drag and hover moves are sent at most once per animation frame, the latest position.
    fn on_pointer_moved(this: &Self, e: &PointerEventArgs) {
        let touch = this.touch(e.get_position(Some(this)), this.pressed.get(), e.key_modifiers());
        this.coalesce(touch);
    }

    fn on_pointer_released(this: &Self, e: &PointerReleasedEventArgs) {
        if this.pressed.replace(false) {
            e.set_handled(true);
            e.pointer().capture(None);
            this.pointer(e.get_position(Some(this)), false, e.key_modifiers());
        }
    }

    fn on_key_down(this: &Self, e: &KeyEventArgs) {
        if let Some(name) = key_name(e) {
            e.set_handled(true);
            this.link().send(|ordinal| Touch::key(ordinal, &name, true));
        }
    }

    fn on_key_up(this: &Self, e: &KeyEventArgs) {
        if let Some(name) = key_name(e) {
            e.set_handled(true);
            this.link().send(|ordinal| Touch::key(ordinal, &name, false));
        }
    }
}

/// The host-neutral name of a key, as the GPUI host names it.
fn key_name(e: &KeyEventArgs) -> Option<String> {
    let named = match e.key {
        Key::Left => "left",
        Key::Right => "right",
        Key::Up => "up",
        Key::Down => "down",
        Key::Space => "space",
        Key::Return => "enter",
        Key::Escape => "escape",
        Key::Tab => "tab",
        Key::Back => "backspace",
        Key::Delete => "delete",
        _ => {
            let symbol = e.key_symbol.as_deref()?;
            let mut chars = symbol.chars();
            let first = chars.next()?.to_ascii_lowercase();
            return (chars.next().is_none() && first.is_ascii_alphanumeric())
                .then(|| first.to_string());
        }
    };
    Some(named.to_owned())
}

impl VisualImpl for SceneSurface {
    fn render(this: &Self, context: &mut DrawingContext) {
        let started = std::time::Instant::now();
        this.tell_size();
        let scene = this.scene();
        let scale = this.placement().scale;
        let placed = context.push_transform(Matrix::create_scale(scale, scale));
        for shape in &scene.shapes {
            draw(context, shape, &scene.atlas);
        }
        context.pop(placed);
        PAINT_MS.with(|total| total.set(total.get() + started.elapsed().as_secs_f64() * 1000.));
    }
}

thread_local! {
    /// Milliseconds spent recording scene draw calls since the bench last read it.
    pub static PAINT_MS: Cell<f64> = const { Cell::new(0.) };
}

/// A packed `0xRRGGBBAA` colour.
fn packed(value: u32) -> Color {
    let [r, g, b, a] = value.to_be_bytes();
    Color::from_argb(a, r, g, b)
}

fn solid(colour: Color) -> Rc<dyn IBrush> {
    SolidColorBrush::with_color(colour).into()
}

/// A float tint as a colour: RGB, or HSL converted as GPUI converts `hsla`.
fn tinted(tint: Tint) -> Color {
    let ([r, g, b], a) = match tint {
        Tint::Rgb([r, g, b, a]) => ([r, g, b], a),
        Tint::Hsl([h, s, l, a]) => (hsl(h, s, l), a),
    };
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_argb(byte(a), byte(r), byte(g), byte(b))
}

fn hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let sector = (h.rem_euclid(1.0)) * 6.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let m = l - chroma / 2.0;
    let [r, g, b] = match sector as u32 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    [r + m, g + m, b + m]
}

/// A rectangle of `w` by `h` centred on `x, y`, in scene units.
fn centred(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(
        f64::from(x - w / 2.0),
        f64::from(y - h / 2.0),
        f64::from(w),
        f64::from(h),
    )
}

/// Draws one shape in scene units.
fn draw(context: &mut DrawingContext, shape: &Shape, atlas: &[String]) {
    match shape {
        Shape::Rect {
            x,
            y,
            w,
            h,
            angle,
            colour,
            ..
        } => turned(context, (*x, *y, *angle), |context| {
            context.fill_rectangle(&solid(packed(*colour)), centred(0.0, 0.0, *w, *h), 0.0);
        }),
        Shape::Disc {
            x,
            y,
            radius,
            colour,
            ..
        } => {
            let r = f64::from(*radius);
            let centre = Point::new(f64::from(*x), f64::from(*y));
            context.draw_ellipse_at(Some(&solid(packed(*colour))), None, centre, r, r);
        }
        Shape::Strip {
            points,
            colour,
            stroke,
            ..
        } => strip(context, points, *colour, *stroke),
        Shape::Sprite {
            atlas: index,
            x,
            y,
            w,
            h,
            angle,
            tint,
            ..
        } => turned(context, (*x, *y, *angle), |context| {
            sprite(context, atlas.get(*index), centred(0.0, 0.0, *w, *h), (*index, *tint));
        }),
        Shape::Glyphs {
            text,
            x,
            y,
            size,
            colour,
            ..
        } => {
            let formatted = line(text, *size, *colour, FontWeight::Normal);
            context.draw_text(&formatted, Point::new(f64::from(*x), f64::from(*y)));
        }
        Shape::Lettering {
            text,
            x,
            y,
            size,
            colour,
            anchor,
            weight,
            ..
        } => {
            let formatted = line(text, *size, *colour, FontWeight(weight.round() as i32));
            let shift = formatted.width() * f64::from(*anchor) / 2.0;
            context.draw_text(&formatted, Point::new(f64::from(*x) - shift, f64::from(*y)));
        }
        Shape::Path {
            steps,
            outline,
            paint,
            transform,
            ..
        } => {
            let moved = transform.unwrap_or((0.0, 0.0, 1.0));
            let geometry = path(steps, matches!(outline, Outline::Fill), moved);
            let brush = painted(paint);
            match outline {
                Outline::Fill => context.draw_geometry(Some(&brush), None, &geometry.upcast()),
                Outline::Stroke {
                    width,
                    bevel,
                    dashes,
                } => {
                    let pen = stroke_pen(brush, *width, *bevel, dashes);
                    context.draw_geometry(None, Some(&pen), &geometry.upcast());
                }
            }
        }
    }
}

/// Runs `paint` with the origin at `x, y` and turned by `angle` radians.
fn turned(context: &mut DrawingContext, (x, y, angle): (f32, f32, f32), paint: impl FnOnce(&mut DrawingContext)) {
    let placed = context.push_transform(
        Matrix::create_rotation(f64::from(angle))
            * Matrix::create_translation(f64::from(x), f64::from(y)),
    );
    paint(context);
    context.pop(placed);
}

fn strip(context: &mut DrawingContext, points: &[(f32, f32)], colour: u32, stroke: f32) {
    let Some(((x, y), rest)) = points.split_first() else {
        return;
    };
    let geometry = StreamGeometry::new();
    {
        let mut path = geometry.open();
        path.begin_figure(Point::new(f64::from(*x), f64::from(*y)), false);
        for (x, y) in rest {
            path.line_to(Point::new(f64::from(*x), f64::from(*y)), true);
        }
        path.end_figure(false);
        path.dispose();
    }
    let pen: Rc<dyn IPen> = Pen::with_all(
        Some(solid(packed(colour))),
        f64::from(stroke),
        None,
        PenLineCap::Round,
        PenLineJoin::Round,
        10.0,
    )
    .into();
    context.draw_geometry(None, Some(&pen), &geometry.upcast());
}

thread_local! {
    /// Sprite images by path, decoded once; a path that failed stays failed.
    static IMAGES: RefCell<HashMap<String, Option<Rc<Bitmap>>>> = RefCell::new(HashMap::new());
}

fn image(path: &str) -> Option<Rc<Bitmap>> {
    IMAGES.with(|images| {
        images
            .borrow_mut()
            .entry(path.to_owned())
            .or_insert_with(|| Bitmap::from_file(path).ok().map(Rc::new))
            .clone()
    })
}

/// A sprite's image in `rect`; without one, its tinted quad, as the GPUI host draws it.
fn sprite(context: &mut DrawingContext, path: Option<&String>, rect: Rect, (index, tint): (usize, u32)) {
    if let Some(bitmap) = path.and_then(|path| image(path)) {
        context.draw_image(bitmap.as_ref(), rect);
        return;
    }
    let colour = match (tint, index) {
        (0xFFFF_FFFF, 0) => 0x87CE_EBFF,
        (0xFFFF_FFFF, 1) => 0xFFA5_00FF,
        (0xFFFF_FFFF, 2) => 0x55AA_55FF,
        (0xFFFF_FFFF, _) => 0x8888_88FF,
        (tint, _) => tint,
    };
    context.fill_rectangle(&solid(packed(colour)), rect, 0.0);
}

/// One line of text in the system font, the top of its line at the origin.
fn line(text: &str, size: f32, colour: u32, weight: FontWeight) -> FormattedText {
    let typeface = Typeface::with_style(
        FontFamily::default(),
        FontStyle::Normal,
        weight,
        FontStretch::Normal,
    );
    FormattedText::new(
        text,
        Locale::UNKNOWN,
        FlowDirection::LeftToRight,
        typeface,
        f64::from(size.max(0.1)),
        Some(solid(packed(colour))),
    )
}

/// A path's pen: its width, bevel or miter joins and dashes in scene units.
fn stroke_pen(brush: Rc<dyn IBrush>, width: f32, bevel: bool, dashes: &[f32]) -> Rc<dyn IPen> {
    let width = f64::from(width);
    let dash = (!dashes.is_empty()).then(|| {
        // A dash style counts in pen widths; the scene's dashes are in units.
        let relative: Vec<f64> = dashes
            .iter()
            .map(|dash| f64::from(*dash) / width.max(f64::EPSILON))
            .collect();
        DashStyle::with_dashes(Some(&relative), 0.0).into()
    });
    let join = if bevel { PenLineJoin::Bevel } else { PenLineJoin::Miter };
    Pen::with_all(Some(brush), width, dash, PenLineCap::Flat, join, 10.0).into()
}

/// Builds a path's geometry, its own transform applied as the GPUI host applies it: translated by
/// `x, y`, then scaled, so a stroke's width stays in scene units.
struct Builder {
    path: StreamGeometryContext,
    filled: bool,
    open: bool,
    at: Point,
    moved: (f32, f32, f32),
}

impl Builder {
    fn point(&self, (x, y): (f32, f32)) -> Point {
        let (dx, dy, scale) = self.moved;
        Point::new(f64::from((x + dx) * scale), f64::from((y + dy) * scale))
    }

    fn start(&mut self) {
        if !self.open {
            self.path.begin_figure(self.at, self.filled);
            self.open = true;
        }
    }

    fn finish(&mut self, closed: bool) {
        if self.open {
            self.path.end_figure(closed);
            self.open = false;
        }
    }

    fn step(&mut self, step: &Step) {
        match step {
            Step::Move(to) => {
                self.finish(false);
                self.at = self.point(*to);
                self.start();
            }
            Step::Line(to) => {
                self.start();
                self.at = self.point(*to);
                self.path.line_to(self.at, true);
            }
            Step::Curve(to, control) => {
                self.start();
                let control = self.point(*control);
                self.at = self.point(*to);
                self.path.quadratic_bezier_to(control, self.at, true);
            }
            Step::Cubic(to, first, second) => {
                self.start();
                let (first, second) = (self.point(*first), self.point(*second));
                self.at = self.point(*to);
                self.path.cubic_bezier_to(first, second, self.at, true);
            }
            Step::Arc {
                radii,
                rotation,
                large,
                sweep,
                to,
            } => self.arc(*radii, *rotation, (*large, *sweep), *to),
            Step::Polygon(corners, closed) => self.polygon(corners, *closed),
            Step::Close => self.finish(true),
        }
    }

    fn arc(&mut self, (rx, ry): (f32, f32), rotation: f32, (large, sweep): (bool, bool), to: (f32, f32)) {
        self.start();
        let scale = self.moved.2;
        self.at = self.point(to);
        let direction = if sweep {
            SweepDirection::Clockwise
        } else {
            SweepDirection::CounterClockwise
        };
        let radii = Size::new(f64::from(rx * scale), f64::from(ry * scale));
        self.path
            .arc_to(self.at, radii, f64::from(rotation), large, direction, true);
    }

    fn polygon(&mut self, corners: &[(f32, f32)], closed: bool) {
        let Some((first, rest)) = corners.split_first() else {
            return;
        };
        self.finish(false);
        self.at = self.point(*first);
        self.path.begin_figure(self.at, self.filled);
        for corner in rest {
            let point = self.point(*corner);
            self.path.line_to(point, true);
        }
        self.path.end_figure(closed);
    }
}

fn path(steps: &[Step], filled: bool, moved: (f32, f32, f32)) -> Ref<StreamGeometry> {
    let geometry = StreamGeometry::new();
    {
        let mut builder = Builder {
            path: geometry.open(),
            filled,
            open: false,
            at: Point::new(0.0, 0.0),
            moved,
        };
        for step in steps {
            builder.step(step);
        }
        builder.finish(false);
        builder.path.dispose();
    }
    geometry
}

/// A path's paint: a colour, or a two-stop linear gradient at an angle in degrees (0 points up,
/// 90 to the right, as CSS and the GPUI host draw it).
fn painted(paint: &Paint) -> Rc<dyn IBrush> {
    match paint {
        Paint::Solid(tint) => solid(tinted(*tint)),
        Paint::Linear {
            angle, from, to, ..
        } => {
            let radians = f64::from(*angle).to_radians();
            let (dx, dy) = (radians.sin() / 2.0, -radians.cos() / 2.0);
            let brush = LinearGradientBrush::new();
            let relative = |x, y| RelativePoint::new(x, y, RelativeUnit::Relative);
            brush.set_start_point(relative(0.5 - dx, 0.5 - dy));
            brush.set_end_point(relative(0.5 + dx, 0.5 + dy));
            brush.set_gradient_stops(GradientStops::from_items([
                GradientStop::with_color_and_offset(tinted(from.0), f64::from(from.1)),
                GradientStop::with_color_and_offset(tinted(to.0), f64::from(to.1)),
            ]));
            brush.into()
        }
    }
}
