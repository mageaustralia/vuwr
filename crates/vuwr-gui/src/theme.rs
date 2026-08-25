//! The palette, type scale and spacing, in one place.
//!
//! Every colour on screen comes from here. The names and values are the
//! design's own: one cool-grey ramp (hue 250, chroma ≤ 0.012) so nothing
//! reads as tinted, one accent blue for *active* and *selected*, and amber
//! reserved for unsaved and warnings so it means something when it appears.
//!
//! Each colour is a pair. The light values are the design's; the dark ones
//! are its terminal surface, which is the same palette seen against a dark
//! ground — so switching mode changes the ground, not the vocabulary.
//!
//! Colours are functions rather than constants because the mode is chosen
//! at runtime. The OKLCH values are kept in the comments: they are what a
//! ramp would be regenerated from, and the hex is what egui takes.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, Stroke};

static DARK: AtomicBool = AtomicBool::new(false);

/// Whether the dark ground is in use.
pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Choose the ground. The caller re-runs [`install`] afterwards.
pub fn set_dark(on: bool) {
    DARK.store(on, Ordering::Relaxed);
}

/// A colour from core's table, in egui's type.
pub fn from_rgb(c: vuwr_core::Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// The colour for one syntax token.
///
/// The design's own, in both grounds. Colour schemes are a terminal
/// feature: a window has a design, and a window that is sometimes
/// Monokai and sometimes not has two.
///
/// Read from the palette's own idea of the ground rather than from
/// `ui.visuals().dark_mode`, which is a different question and once
/// answered dark while the window was white — every piece of the file
/// drawn in a grey meant for a dark ground, on white.
pub fn token(token: vuwr_core::Token) -> Color32 {
    from_rgb(vuwr_core::Scheme::Vuwr.token(token, is_dark()))
}

/// The colour for a leaf's value on a tree row.
pub fn value(kind: vuwr_core::ValueKind) -> Color32 {
    from_rgb(vuwr_core::Scheme::Vuwr.value(kind, is_dark()))
}

/// The colour for a container's summary on a tree row.
pub fn placeholder() -> Color32 {
    from_rgb(vuwr_core::Scheme::Vuwr.placeholder(is_dark()))
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The light value, or the dark one when the dark ground is in use.
fn pick(light: u32, dark: u32) -> Color32 {
    if is_dark() { rgb(dark) } else { rgb(light) }
}

// Neutrals — surfaces.
/// Panel and table background. `0.995 0.001 250`
pub fn surface() -> Color32 {
    pick(0xFDFDFE, 0x13161A)
}
/// Toolbar and inspector. `0.985 0.002 250`
pub fn surface_sunk() -> Color32 {
    pick(0xF9FAFB, 0x181C21)
}
/// Column headers and the status line. `0.975 0.002 250`
pub fn surface_header() -> Color32 {
    pick(0xF6F7F8, 0x1B2027)
}
/// Title bar. `0.965 0.003 250`
pub fn surface_chrome() -> Color32 {
    pick(0xF3F4F6, 0x1E242B)
}
/// Keyboard hint row, segmented track. `0.955 0.003 250`
pub fn surface_hint() -> Color32 {
    pick(0xF0F1F3, 0x22282F)
}
/// Panel dividers, 1px rules. `0.9 0.006 250`
pub fn border() -> Color32 {
    pick(0xDFE1E5, 0x3C4147)
}
/// Outlined button edges. `0.89 0.006 250`
pub fn border_control() -> Color32 {
    pick(0xDCDFE3, 0x454B52)
}
/// Row separators. `0.96 0.003 250`
pub fn border_faint() -> Color32 {
    pick(0xF4F5F6, 0x272D35)
}

// Neutrals — text.
/// Headings only. `0.24 0.012 250`
pub fn text() -> Color32 {
    pick(0x292D33, 0xF2F4F6)
}
/// Data cells, filename. `0.3 0.012 250`
pub fn text_body() -> Color32 {
    pick(0x383D44, 0xE6E8EB)
}
/// Button labels. `0.35 0.012 250`
pub fn text_control() -> Color32 {
    pick(0x454B52, 0xC9CED4)
}
/// Column headers, secondary meta. `0.5 0.012 250`
pub fn text_muted() -> Color32 {
    pick(0x6B7280, 0xABB1BA)
}
/// Hint labels, path. `0.6 0.012 250`
pub fn text_dim() -> Color32 {
    pick(0x868D97, 0x8D939C)
}
/// Redo when there is nothing to redo. `0.72 0.008 250`
pub fn text_disabled() -> Color32 {
    pick(0xABB1BA, 0x5C636B)
}

// Accent — one blue, three roles.
/// Save fill, selected row bar. `0.5 0.13 250`
pub fn accent() -> Color32 {
    pick(0x1E6FBF, 0x2E7BC9)
}
/// Active segment label, paths, identifiers. `0.42 0.13 250`
pub fn accent_text() -> Color32 {
    pick(0x17568F, 0x6EA2E0)
}
/// Active filter fill. `0.96 0.03 250`
pub fn accent_tint() -> Color32 {
    pick(0xE8F0FB, 0x1B2B3E)
}
/// Active filter edge. `0.72 0.09 250`
pub fn accent_border() -> Color32 {
    pick(0x7FA8D8, 0x35506F)
}
/// The row the cursor is on. `0.955 0.02 250`
pub fn row_selected() -> Color32 {
    pick(0xE9EFF8, 0x1C2636)
}

// State.
/// Unsaved dot, outlier marker. `0.7 0.13 75`
pub fn warn() -> Color32 {
    pick(0xC9891F, 0xD9A23C)
}
/// Issue bar fill. `0.97 0.025 75`
pub fn warn_tint() -> Color32 {
    pick(0xFBF3E3, 0x2A2317)
}
/// Issue bar edge. `0.85 0.06 75`
pub fn warn_border() -> Color32 {
    pick(0xE0C48F, 0x5A4A2E)
}
/// Issue bar label and actions. `0.45 0.11 55`
pub fn warn_text() -> Color32 {
    pick(0x8A5A1E, 0xE0B970)
}
/// The 2px ring on the cell being edited. `0.6 0.14 55`
pub fn edit_ring() -> Color32 {
    pick(0xB4671F, 0xD98A3F)
}

/// The colour a filled control's label sits in.
pub fn on_accent() -> Color32 {
    rgb(0xFCFCFD)
}

/// Named text styles, so a size never appears at a call site.
///
/// Sans is for anything the reader takes as a label; mono for anything
/// that came out of the file, plus keys and positions. That split is what
/// makes the chrome recede — with everything mono at one size, nothing
/// could.
pub fn heading() -> egui::TextStyle {
    egui::TextStyle::Name("heading".into())
}

/// Status line, hint labels.
pub fn meta() -> egui::TextStyle {
    egui::TextStyle::Name("meta".into())
}

/// Column headers, keycaps, the ISSUE label.
pub fn micro() -> egui::TextStyle {
    egui::TextStyle::Name("micro".into())
}

/// Row height in the table. Fixed, because virtualisation needs to know
/// how tall a row is without laying it out.
pub const ROW_HEIGHT: f32 = 33.0;
/// Table header height.
pub const HEADER_HEIGHT: f32 = 28.0;
/// Cell padding: 7px vertical, 10px horizontal.
pub const CELL_PAD_X: f32 = 10.0;
/// Width of the marker down a selected or flagged row's left edge.
pub const ROW_MARKER: f32 = 2.0;
/// Height of a control: an outlined pill.
pub const CONTROL_HEIGHT: f32 = 26.0;

/// Whether the style in this context is one we installed.
///
/// The views ask for text styles by name, and egui aborts the process on
/// a name it does not know. Anything that can replace the style — a
/// restored session, a second context — has to be noticed before a view
/// draws rather than after.
pub fn is_installed(ctx: &egui::Context) -> bool {
    ctx.style().text_styles.contains_key(&meta())
}

/// Install the style if this context does not already have ours.
///
/// Cheap enough to call before drawing anything: one map lookup against
/// a style that is nearly always already right.
pub fn ensure(ctx: &egui::Context) {
    if !is_installed(ctx) {
        install(ctx);
    }
}

/// Install the palette, the type scale and the spacing rhythm.
///
/// Called once. Everything drawn afterwards assumes it: a colour set at a
/// call site that is not in this module is a bug.
pub fn install(ctx: &egui::Context) {
    let mut s = (*ctx.style()).clone();

    // egui's own visuals decide the parts we do not paint ourselves —
    // menus, tooltips, scrollbars — so they follow the same ground.
    s.visuals = if is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    // Named faces rather than weights: egui does not synthesise bold, so
    // a "semibold" heading in one file is the only way to have one.
    let sans = egui::FontFamily::Name("sans".into());
    let sans_medium = egui::FontFamily::Name("sans_medium".into());
    let sans_semi = egui::FontFamily::Name("sans_semi".into());
    let mono = egui::FontFamily::Name("mono".into());
    let mono_medium = egui::FontFamily::Name("mono_medium".into());
    let (sans2, mono2) = (sans_semi.clone(), mono.clone());
    // Inserted rather than assigned: replacing the map drops egui's own
    // Heading and Small, and anything still asking for one panics.
    let styles = [
        (heading(), egui::FontId::new(13.5, sans_semi.clone())),
        (egui::TextStyle::Body, egui::FontId::new(12.5, sans.clone())),
        (
            egui::TextStyle::Button,
            egui::FontId::new(12.5, sans_medium),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(12.0, mono.clone()),
        ),
        (meta(), egui::FontId::new(11.5, mono.clone())),
        (micro(), egui::FontId::new(11.0, mono_medium)),
    ];
    for (name, font) in styles {
        s.text_styles.insert(name, font);
    }
    // Our scale has one heading; egui's own is twice the size it wants.
    s.text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::new(15.0, sans2));
    s.text_styles
        .insert(egui::TextStyle::Small, egui::FontId::new(11.0, mono2));

    // Everything is a multiple of the 4px grid.
    s.spacing.item_spacing = egui::vec2(4.0, 4.0);
    s.spacing.button_padding = egui::vec2(9.0, 4.0);
    s.spacing.interact_size.y = CONTROL_HEIGHT;
    s.spacing.scroll.bar_width = 8.0;
    // More room on the right than the left: a menu item's shortcut is
    // right-aligned, and against the edge it reads as falling off it.
    s.spacing.menu_margin = egui::Margin {
        left: 6,
        right: 14,
        top: 6,
        bottom: 6,
    };

    let v = &mut s.visuals;
    v.panel_fill = surface();
    v.window_fill = surface();
    v.extreme_bg_color = surface_sunk();
    v.faint_bg_color = surface_header();
    v.override_text_color = Some(text_control());
    v.selection.bg_fill = row_selected();
    v.selection.stroke = Stroke::new(1.0_f32, accent_text());
    v.window_stroke = Stroke::new(1.0_f32, border());

    // Actions are outlined, not filled: the filled treatment belongs to
    // Save alone, and a toolbar of filled boxes has nothing to say.
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = CornerRadius::same(5);
        w.bg_stroke = Stroke::new(1.0_f32, border_control());
        w.fg_stroke = Stroke::new(1.0_f32, text_control());
        w.expansion = 0.0;
    }
    v.widgets.inactive.weak_bg_fill = surface();
    v.widgets.hovered.weak_bg_fill = surface_header();
    v.widgets.active.weak_bg_fill = surface_hint();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_muted());
    // Disabled drops to a lighter fill rather than the same box greyed.
    v.widgets.noninteractive.weak_bg_fill = surface_header();

    // Into *both* slots, and pin the preference to the ground we chose.
    //
    // egui keeps one style for light and one for dark, and `set_style`
    // fills only whichever is active. Install ours into one, let the
    // active theme flip — the system theme arriving a moment after
    // startup will do it — and the other slot is egui's default, with
    // none of the text styles named here. egui aborts the process on a
    // style name it does not know, so the app died on load rather than
    // looking wrong.
    ctx.set_style_of(egui::Theme::Light, s.clone());
    ctx.set_style_of(egui::Theme::Dark, s);
    ctx.set_theme(if is_dark() {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
}

/// A keycap: the key itself, boxed, the way the hint row shows it.
pub fn keycap(ui: &mut egui::Ui, key: &str) {
    let galley = ui.painter().layout_no_wrap(
        key.to_owned(),
        ui.style()
            .text_styles
            .get(&micro())
            .cloned()
            .unwrap_or_default(),
        text_control(),
    );
    let size = egui::vec2(galley.size().x + 12.0, galley.size().y + 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), surface());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, border_control()),
        egui::StrokeKind::Inside,
    );
    let at = rect.center() - galley.size() / 2.0;
    ui.painter().galley(at, galley, text_control());
}

/// One segment of a segmented control. Returns true when it was clicked.
///
/// The active segment gets the surface colour and the accent label; egui
/// has no per-widget shadow, and at this size it is not missed.
pub fn segment(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let (fill, fg) = if active {
        (surface(), accent_text())
    } else {
        (Color32::TRANSPARENT, text_control())
    };
    let button = egui::Button::new(egui::RichText::new(label).color(fg))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(5))
        .min_size(egui::vec2(0.0, 22.0));
    ui.add(button).clicked()
}

/// The track a group of segments sits in.
pub fn segmented<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(surface_hint())
        .corner_radius(CornerRadius::same(7))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| add(ui)).inner
        })
        .inner
}

/// An outlined action. The default treatment for everything but Save.
pub fn action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let colour = if enabled {
        text_control()
    } else {
        text_disabled()
    };
    let button = egui::Button::new(egui::RichText::new(label).color(colour))
        .fill(if enabled { surface() } else { surface_header() })
        .stroke(Stroke::new(
            1.0_f32,
            if enabled {
                border_control()
            } else {
                border_faint()
            },
        ))
        .corner_radius(CornerRadius::same(5));
    ui.add_enabled(enabled, button)
}

/// The one filled control: Save.
///
/// The shortcut lives on hover rather than inside the button: two type
/// sizes in a 60-pixel control is a badge, not a label.
pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label).color(on_accent());
    ui.add(
        egui::Button::new(text)
            .fill(accent())
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6))
            .min_size(egui::vec2(64.0, 24.0)),
    )
}

/// A vertical rule between toolbar groups.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 18.0), egui::Sense::hover());
    let x = rect.center().x.round();
    ui.painter().vline(
        x,
        rect.top()..=rect.bottom(),
        Stroke::new(1.0_f32, border()),
    );
}
