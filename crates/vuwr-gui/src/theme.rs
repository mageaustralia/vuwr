//! The palette, type scale and spacing, in one place.
//!
//! Every colour on screen comes from here. The names and values are the
//! design's own: one cool-grey ramp (hue 250, chroma ≤ 0.012) so nothing
//! reads as tinted, one accent blue for *active* and *selected*, and amber
//! reserved for unsaved and warnings so it means something when it appears.
//!
//! The OKLCH values are kept in the comments rather than converted at
//! runtime: they are what a ramp would be regenerated from, and the hex is
//! what egui takes.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// Neutrals — surfaces.
/// Panel and table background. `0.995 0.001 250`
pub const SURFACE: Color32 = rgb(0xFDFDFE);
/// Toolbar, inspector. `0.985 0.002 250`
pub const SURFACE_SUNK: Color32 = rgb(0xF9FAFB);
/// Column headers, status line. `0.975 0.002 250`
pub const SURFACE_HEADER: Color32 = rgb(0xF6F7F8);
/// Title bar. `0.965 0.003 250`
pub const SURFACE_CHROME: Color32 = rgb(0xF3F4F6);
/// Keyboard hint row, segmented track. `0.955 0.003 250`
pub const SURFACE_HINT: Color32 = rgb(0xF0F1F3);
/// Panel dividers, 1px rules. `0.9 0.006 250`
pub const BORDER: Color32 = rgb(0xDFE1E5);
/// Outlined button edges. `0.89 0.006 250`
pub const BORDER_CONTROL: Color32 = rgb(0xDCDFE3);
/// Row separators. `0.96 0.003 250`
pub const BORDER_FAINT: Color32 = rgb(0xF4F5F6);

// Neutrals — text.
/// Headings only. `0.24 0.012 250`
pub const TEXT: Color32 = rgb(0x292D33);
/// Data cells, filename. `0.3 0.012 250`
pub const TEXT_BODY: Color32 = rgb(0x383D44);
/// Button labels. `0.35 0.012 250`
pub const TEXT_CONTROL: Color32 = rgb(0x454B52);
/// Column headers, secondary meta. `0.5 0.012 250`
pub const TEXT_MUTED: Color32 = rgb(0x6B7280);
/// Hint labels, path. `0.6 0.012 250`
pub const TEXT_DIM: Color32 = rgb(0x868D97);
/// Redo when there is nothing to redo. `0.72 0.008 250`
pub const TEXT_DISABLED: Color32 = rgb(0xABB1BA);

// Accent — one blue, three roles.
/// Save fill, selected row bar. `0.5 0.13 250`
pub const ACCENT: Color32 = rgb(0x1E6FBF);
/// Active segment label, paths, IDs. `0.42 0.13 250`
pub const ACCENT_TEXT: Color32 = rgb(0x17568F);
/// Active filter fill. `0.96 0.03 250`
pub const ACCENT_TINT: Color32 = rgb(0xE8F0FB);
/// Active filter edge. `0.72 0.09 250`
pub const ACCENT_BORDER: Color32 = rgb(0x7FA8D8);
/// Selected table row. `0.955 0.02 250`
pub const ROW_SELECTED: Color32 = rgb(0xE9EFF8);

// State.
/// Unsaved dot, outlier marker. `0.7 0.13 75`
pub const WARN: Color32 = rgb(0xC9891F);
/// Issue bar fill. `0.97 0.025 75`
pub const WARN_TINT: Color32 = rgb(0xFBF3E3);
/// Issue bar edge. `0.85 0.06 75`
pub const WARN_BORDER: Color32 = rgb(0xE0C48F);
/// Issue bar label and actions. `0.45 0.11 55`
pub const WARN_TEXT: Color32 = rgb(0x8A5A1E);
/// The 2px focus ring on the cell being edited. `0.6 0.14 55`
pub const EDIT_RING: Color32 = rgb(0xB4671F);
/// The white a filled control's label sits in.
pub const ON_ACCENT: Color32 = rgb(0xFCFCFD);

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

/// Install the palette, the type scale and the spacing rhythm.
///
/// Called once. Everything drawn afterwards assumes it: a colour set at a
/// call site that is not in this module is a bug.
pub fn install(ctx: &egui::Context) {
    let mut s = (*ctx.style()).clone();

    // The design is a light theme throughout; following the system into
    // dark mode would leave half these colours illegible.
    s.visuals = egui::Visuals::light();

    let sans = egui::FontFamily::Proportional;
    let mono = egui::FontFamily::Monospace;
    s.text_styles = [
        (heading(), egui::FontId::new(13.5, sans.clone())),
        (egui::TextStyle::Body, egui::FontId::new(12.5, sans.clone())),
        (egui::TextStyle::Button, egui::FontId::new(12.5, sans)),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(12.0, mono.clone()),
        ),
        (meta(), egui::FontId::new(11.5, mono.clone())),
        (micro(), egui::FontId::new(11.0, mono)),
    ]
    .into();

    // Everything is a multiple of the 4px grid.
    s.spacing.item_spacing = egui::vec2(4.0, 4.0);
    s.spacing.button_padding = egui::vec2(9.0, 4.0);
    s.spacing.interact_size.y = CONTROL_HEIGHT;
    s.spacing.scroll.bar_width = 8.0;
    s.spacing.menu_margin = egui::Margin::symmetric(6, 6);

    let v = &mut s.visuals;
    v.panel_fill = SURFACE;
    v.window_fill = SURFACE;
    v.extreme_bg_color = SURFACE_SUNK;
    v.faint_bg_color = SURFACE_HEADER;
    v.override_text_color = Some(TEXT_CONTROL);
    v.selection.bg_fill = ROW_SELECTED;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT_TEXT);
    v.window_stroke = Stroke::new(1.0_f32, BORDER);

    // Actions are outlined, not filled: the filled treatment belongs to
    // Save alone, and a toolbar of filled boxes has nothing to say.
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = CornerRadius::same(5);
        w.bg_stroke = Stroke::new(1.0_f32, BORDER_CONTROL);
        w.fg_stroke = Stroke::new(1.0_f32, TEXT_CONTROL);
        w.expansion = 0.0;
    }
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.hovered.weak_bg_fill = SURFACE_HEADER;
    v.widgets.active.weak_bg_fill = SURFACE_HINT;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_MUTED);
    // Disabled drops to a lighter fill rather than the same box greyed.
    v.widgets.noninteractive.weak_bg_fill = SURFACE_HEADER;

    ctx.set_style(s);
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
        TEXT_CONTROL,
    );
    let size = egui::vec2(galley.size().x + 12.0, galley.size().y + 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), SURFACE);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, BORDER_CONTROL),
        egui::StrokeKind::Inside,
    );
    let at = rect.center() - galley.size() / 2.0;
    ui.painter().galley(at, galley, TEXT_CONTROL);
}

/// One segment of a segmented control. Returns true when it was clicked.
///
/// The active segment gets the surface colour and the accent label; egui
/// has no per-widget shadow, and at this size it is not missed.
pub fn segment(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let (fill, fg) = if active {
        (SURFACE, ACCENT_TEXT)
    } else {
        (Color32::TRANSPARENT, TEXT_CONTROL)
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
        .fill(SURFACE_HINT)
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
    let colour = if enabled { TEXT_CONTROL } else { TEXT_DISABLED };
    let button = egui::Button::new(egui::RichText::new(label).color(colour))
        .fill(if enabled { SURFACE } else { SURFACE_HEADER })
        .stroke(Stroke::new(
            1.0_f32,
            if enabled {
                BORDER_CONTROL
            } else {
                BORDER_FAINT
            },
        ))
        .corner_radius(CornerRadius::same(5));
    ui.add_enabled(enabled, button)
}

/// The one filled control: Save.
pub fn primary(ui: &mut egui::Ui, label: &str, shortcut: &str) -> egui::Response {
    let mut job = egui::text::LayoutJob::default();
    let body = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Button)
        .cloned()
        .unwrap_or_default();
    let key = ui
        .style()
        .text_styles
        .get(&micro())
        .cloned()
        .unwrap_or_default();
    job.append(label, 0.0, egui::TextFormat::simple(body, ON_ACCENT));
    if !shortcut.is_empty() {
        job.append(
            shortcut,
            7.0,
            egui::TextFormat::simple(key, ON_ACCENT.gamma_multiply(0.7)),
        );
    }
    ui.add(
        egui::Button::new(job)
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6)),
    )
}

/// A vertical rule between toolbar groups.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 18.0), egui::Sense::hover());
    let x = rect.center().x.round();
    ui.painter()
        .vline(x, rect.top()..=rect.bottom(), Stroke::new(1.0_f32, BORDER));
}
