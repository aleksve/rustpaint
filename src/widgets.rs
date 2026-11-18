//! Small custom widgets that give the app its own modern look (egui's stock
//! widgets are used for text input / sliders only).

use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::theme::Theme;

#[inline]
pub(crate) fn rr(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

#[inline]
pub(crate) fn st(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

fn hover_t(ui: &Ui, resp: &Response, enabled: bool) -> f32 {
    ui.ctx()
        .animate_bool(resp.id.with("hov"), enabled && resp.hovered())
}

/// Square icon button (tool rail, top bar).
pub(crate) fn icon_button(
    ui: &mut Ui,
    th: &Theme,
    icon: Icon,
    size: f32,
    active: bool,
) -> Response {
    icon_button_enabled(ui, th, icon, size, active, true)
}

/// Square icon button with an explicit enabled state.
pub(crate) fn icon_button_enabled(
    ui: &mut Ui,
    th: &Theme,
    icon: Icon,
    size: f32,
    active: bool,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let t = hover_t(ui, &resp, enabled);
    let radius = rr((size * 0.26).round().clamp(4.0, 14.0) as u8);
    let p = ui.painter();
    let bg = if active && enabled {
        th.accent_soft
    } else if t > 0.0 {
        blend(th.panel, th.hover, t)
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        p.rect_filled(rect, radius, bg);
    }
    if enabled && resp.is_pointer_button_down_on() && !active {
        p.rect_filled(rect, radius, th.hover);
    }
    let fg = if !enabled {
        th.text_dim.gamma_multiply(0.45)
    } else if active {
        th.accent
    } else {
        blend(th.text_dim, th.text, t)
    };
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(size * 0.58));
    icons::draw(p, icon, glyph, fg, (size * 0.075).clamp(1.2, 1.9));
    resp
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    Secondary,
    Primary,
    Danger,
}

/// Label button used in dialogs and panels (lays out text only once per frame).
pub(crate) fn button(ui: &mut Ui, th: &Theme, label: &str, primary: bool) -> Response {
    let kind = if primary {
        ButtonKind::Primary
    } else {
        ButtonKind::Secondary
    };
    button_styled(ui, th, label, kind, true)
}

pub(crate) fn button_styled(
    ui: &mut Ui,
    th: &Theme,
    label: &str,
    kind: ButtonKind,
    enabled: bool,
) -> Response {
    let fg = if !enabled {
        th.text_dim.gamma_multiply(0.55)
    } else {
        match kind {
            ButtonKind::Primary => th.on_accent,
            ButtonKind::Danger => th.danger,
            ButtonKind::Secondary => th.text,
        }
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), fg);
    let galley_size = galley.size();
    let size = Vec2::new(galley_size.x + 26.0, 32.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let t = hover_t(ui, &resp, enabled);
    let (bg, border) = if !enabled {
        (th.panel_alt, th.border)
    } else {
        match kind {
            ButtonKind::Primary => (
                blend(th.accent, th.on_accent, t * 0.12),
                Color32::TRANSPARENT,
            ),
            ButtonKind::Danger => (
                blend(th.panel, th.danger.gamma_multiply(0.18), t),
                blend(th.border, th.danger, 0.45 + 0.35 * t),
            ),
            ButtonKind::Secondary => (blend(blend(th.panel, th.raised, t), th.hover, t), th.border),
        }
    };
    let p = ui.painter();
    p.rect_filled(rect, rr(8), bg);
    if border != Color32::TRANSPARENT {
        p.rect_stroke(rect, rr(8), st(1.0, border), StrokeKind::Inside);
    }
    p.galley(
        Pos2::new(
            rect.center().x - galley_size.x * 0.5,
            rect.center().y - galley_size.y * 0.5,
        ),
        galley,
        fg,
    );
    resp
}

/// Toggle switch with a sliding knob.
pub(crate) fn toggle(ui: &mut Ui, th: &Theme, on: &mut bool, label: &str) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool(resp.id.with("on"), *on);
    let knob_x = rect.left() + 9.0 + 14.0 * t;
    let track = Rect::from_min_size(
        Pos2::new(rect.left(), rect.center().y - 8.0),
        Vec2::new(32.0, 16.0),
    );
    let p = ui.painter();
    p.rect_filled(track, rr(8), blend(th.raised, th.accent, t));
    p.circle_filled(Pos2::new(knob_x, rect.center().y), 6.0, Color32::WHITE);
    p.text(
        Pos2::new(rect.left() + 40.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        th.text,
    );
    resp
}

/// Slider row with label on the left and value on the right.
pub(crate) fn slider(
    ui: &mut Ui,
    th: &Theme,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.0).color(th.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{value:.decimals$}"))
                    .size(12.0)
                    .color(th.text),
            );
        });
    });
    ui.spacing_mut().slider_width = (ui.available_width() - 6.0).max(40.0);
    ui.add(egui::Slider::new(value, range).show_value(false))
        .changed()
}

/// Percentage slider row mapping a `0.0..=1.0` value to `0..=100%`.
pub(crate) fn slider_pct(ui: &mut Ui, th: &Theme, label: &str, value: &mut f32) -> bool {
    let mut pct = (*value * 100.0).clamp(0.0, 100.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.0).color(th.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{pct:.0}%"))
                    .size(12.0)
                    .color(th.text),
            );
        });
    });
    ui.spacing_mut().slider_width = (ui.available_width() - 6.0).max(40.0);
    let changed = ui
        .add(egui::Slider::new(&mut pct, 0.0..=100.0).show_value(false))
        .changed();
    if changed {
        *value = (pct / 100.0).clamp(0.0, 1.0);
    }
    changed
}

/// Section heading.
pub(crate) fn section(ui: &mut Ui, th: &Theme, title: &str) {
    ui.add_space(2.0);
    ui.painter().text(
        ui.cursor().min,
        Align2::LEFT_TOP,
        title.to_uppercase(),
        FontId::proportional(10.5),
        th.text_dim,
    );
    ui.add_space(16.0);
}

pub(crate) fn divider(ui: &mut Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.left(), rect.center().y),
            Pos2::new(rect.right(), rect.center().y),
        ],
        st(1.0, th.border),
    );
}

/// Short vertical divider for horizontal toolbars.
pub(crate) fn vsep(ui: &mut Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 22.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.center().x, rect.top()),
            Pos2::new(rect.center().x, rect.bottom()),
        ],
        st(1.0, th.border),
    );
}

/// Framed card container used in the right inspector panel.
pub(crate) fn card(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(th.panel_alt)
        .corner_radius(rr(12))
        .stroke(st(1.0, th.border))
        .inner_margin(egui::Margin::same(11))
        .show(ui, add);
}

/// Draw a colour chip over a subtle checkerboard when translucent.
pub(crate) fn paint_color_chip(
    p: &egui::Painter,
    th: &Theme,
    rect: Rect,
    radius: CornerRadius,
    c: Color32,
) {
    if c.a() < 255 {
        let (a, b) = th.checker_colors();
        p.rect_filled(rect, radius, a);
        let mid = rect.center();
        p.rect_filled(
            Rect::from_min_max(Pos2::new(mid.x, rect.top()), Pos2::new(rect.right(), mid.y)),
            CornerRadius::ZERO,
            b,
        );
        p.rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.left(), mid.y),
                Pos2::new(mid.x, rect.bottom()),
            ),
            CornerRadius::ZERO,
            b,
        );
    }
    p.rect_filled(rect, radius, c);
}

/// Colour swatch with selection ring.
pub(crate) fn swatch(ui: &mut Ui, th: &Theme, c: Color32, size: f32, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    let inner = rect.shrink(1.5);
    paint_color_chip(p, th, inner, rr(4), c);
    p.rect_stroke(
        inner,
        rr(4),
        st(1.0, Color32::from_black_alpha(40)),
        StrokeKind::Inside,
    );
    if selected {
        p.rect_stroke(
            rect.shrink(0.5),
            rr(5),
            st(2.0, th.accent),
            StrokeKind::Inside,
        );
    } else if resp.hovered() {
        p.rect_stroke(
            rect.shrink(0.5),
            rr(5),
            st(1.0, th.text_dim),
            StrokeKind::Inside,
        );
    }
    resp
}

/// Build a 2×2 repeating checkerboard image so the canvas backdrop draws as a
/// single textured quad at any resolution or zoom level.
pub(crate) fn checker_texture_image(a: Color32, b: Color32) -> ColorImage {
    ColorImage {
        size: [2, 2],
        source_size: Vec2::splat(2.0),
        pixels: vec![a, b, b, a],
    }
}

pub(crate) fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

/// A menu row: label on the left, shortcut hint on the right.
pub(crate) fn menu_item(
    ui: &mut Ui,
    th: &Theme,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> bool {
    let width = ui.available_width().clamp(180.0, 280.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), sense);
    let text_color = if enabled {
        th.text
    } else {
        th.text_dim.gamma_multiply(0.6)
    };
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_filled(rect, rr(6), th.hover);
    }
    p.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        text_color,
    );
    if !shortcut.is_empty() {
        p.text(
            Pos2::new(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(11.5),
            th.text_dim,
        );
    }
    resp.clicked() && enabled
}

pub(crate) fn menu_sep(ui: &mut Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(180.0), 9.0),
        Sense::hover(),
    );
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + 6.0, rect.center().y),
            Pos2::new(rect.right() - 6.0, rect.center().y),
        ],
        st(1.0, th.border),
    );
}

/// Modal shell built on `egui::Modal`: traps focus/clicks, dims the backdrop,
/// centres a themed card, and returns the closure result.
pub(crate) fn modal<R>(
    ctx: &egui::Context,
    th: &Theme,
    id: &str,
    size: Vec2,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let backdrop = Color32::from_black_alpha(if th.is_dark() { 160 } else { 90 });
    let frame = egui::Frame::NONE
        .fill(th.panel)
        .corner_radius(rr(16))
        .stroke(st(1.0, th.border))
        .shadow(egui::epaint::Shadow {
            offset: [0, 14],
            blur: 36,
            spread: 0,
            color: Color32::from_black_alpha(if th.is_dark() { 140 } else { 60 }),
        })
        .inner_margin(egui::Margin::same(18));

    egui::Modal::new(egui::Id::unique(("modal", id)))
        .backdrop_color(backdrop)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width((size.x - 36.0).max(160.0));
            add(ui)
        })
        .inner
}

/// Title row with a close button, used by the modal dialogs.
pub(crate) fn modal_header(ui: &mut Ui, th: &Theme, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(15.0)
                .strong()
                .color(th.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, th, Icon::Close, 26.0, false).clicked() {
                close = true;
            }
        });
    });
    ui.add_space(4.0);
    close
}

/// Compact HSV colour picker with persistent `[h, s, v]` state (so picking
/// white, black, or grey never loses the chosen hue), an exact two-pass SV
/// gradient mesh, and a single-mesh rainbow hue strip.
pub(crate) fn color_picker(
    ui: &mut Ui,
    th: &Theme,
    color: &mut Color32,
    hsv_cache: &mut (Color32, [f32; 3]),
) -> bool {
    use eframe::egui::TextureId;
    use eframe::egui::ecolor::Hsva;
    use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};

    // Resync `[h, s, v]` only when the colour was modified outside the picker
    // (palette swatch, hex box, eyedropper, swap/reset).
    if hsv_cache.0 != *color {
        let converted = Hsva::from_srgb([color.r(), color.g(), color.b()]);
        let prev_h = hsv_cache.1[0];
        let h = if converted.s <= 1e-4 || converted.v <= 1e-4 {
            prev_h
        } else {
            converted.h
        };
        hsv_cache.0 = *color;
        hsv_cache.1 = [h, converted.s, converted.v];
    }

    let [mut h, mut s, mut v] = hsv_cache.1;
    let w = ui.available_width();
    let mut changed = false;

    // --- saturation / value square (two stacked quads = exact bilinear HSV) ---
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 118.0), Sense::click_and_drag());
    let hue_c: Color32 = Hsva::new(h, 1.0, 1.0, 1.0).into();
    let mut sv_mesh = Mesh {
        texture_id: TextureId::default(),
        vertices: Vec::with_capacity(8),
        indices: Vec::with_capacity(12),
    };
    // Pass 1: horizontal white -> pure hue.
    for (p, c) in [
        (rect.left_top(), Color32::WHITE),
        (rect.right_top(), hue_c),
        (rect.left_bottom(), Color32::WHITE),
        (rect.right_bottom(), hue_c),
    ] {
        sv_mesh.vertices.push(Vertex {
            pos: p,
            uv: WHITE_UV,
            color: c,
        });
    }
    // Pass 2: vertical transparent -> opaque black (multiplies value cleanly).
    for (p, c) in [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), Color32::TRANSPARENT),
        (rect.left_bottom(), Color32::BLACK),
        (rect.right_bottom(), Color32::BLACK),
    ] {
        sv_mesh.vertices.push(Vertex {
            pos: p,
            uv: WHITE_UV,
            color: c,
        });
    }
    sv_mesh
        .indices
        .extend_from_slice(&[0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]);
    ui.painter().add(egui::Shape::mesh(sv_mesh));
    ui.painter()
        .rect_stroke(rect, rr(2), st(1.0, th.border), StrokeKind::Inside);

    if (resp.is_pointer_button_down_on() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos()
    {
        s = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        v = 1.0 - ((p.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
        changed = true;
    }
    let knob = Pos2::new(
        rect.left() + s * rect.width(),
        rect.top() + (1.0 - v) * rect.height(),
    );
    ui.painter()
        .circle_stroke(knob, 6.0, st(2.0, Color32::WHITE));
    ui.painter()
        .circle_stroke(knob, 7.5, st(1.0, Color32::from_black_alpha(90)));

    ui.add_space(8.0);

    // --- hue strip (exact 6-segment piecewise-linear HSV rainbow mesh) -------
    let (hrect, hresp) = ui.allocate_exact_size(Vec2::new(w, 12.0), Sense::click_and_drag());
    let mut hue_mesh = Mesh {
        texture_id: TextureId::default(),
        vertices: Vec::with_capacity(14),
        indices: Vec::with_capacity(36),
    };
    const STOPS: u32 = 6;
    for i in 0..=STOPS {
        let t = i as f32 / STOPS as f32;
        let x = hrect.left() + t * hrect.width();
        let c: Color32 = Hsva::new(t.min(0.9999), 1.0, 1.0, 1.0).into();
        hue_mesh.vertices.push(Vertex {
            pos: Pos2::new(x, hrect.top()),
            uv: WHITE_UV,
            color: c,
        });
        hue_mesh.vertices.push(Vertex {
            pos: Pos2::new(x, hrect.bottom()),
            uv: WHITE_UV,
            color: c,
        });
        if i < STOPS {
            let b = i * 2;
            hue_mesh
                .indices
                .extend_from_slice(&[b, b + 2, b + 1, b + 1, b + 2, b + 3]);
        }
    }
    ui.painter().add(egui::Shape::mesh(hue_mesh));
    ui.painter()
        .rect_stroke(hrect, rr(2), st(1.0, th.border), StrokeKind::Inside);

    if (hresp.is_pointer_button_down_on() || hresp.dragged())
        && let Some(p) = hresp.interact_pointer_pos()
    {
        h = ((p.x - hrect.left()) / hrect.width().max(1.0)).clamp(0.0, 1.0);
        changed = true;
    }
    let hx = hrect.left() + h * hrect.width();
    let mark = Rect::from_min_max(
        Pos2::new(hx - 3.0, hrect.top() - 2.0),
        Pos2::new(hx + 3.0, hrect.bottom() + 2.0),
    );
    ui.painter()
        .rect_stroke(mark, rr(2), st(2.0, Color32::WHITE), StrokeKind::Inside);
    ui.painter().rect_stroke(
        mark,
        rr(2),
        st(1.0, Color32::from_black_alpha(90)),
        StrokeKind::Outside,
    );

    if changed {
        let [r, g, b] = Hsva::new(h, s, v, 1.0).to_srgb();
        let next = Color32::from_rgba_unmultiplied(r, g, b, color.a());
        hsv_cache.0 = next;
        hsv_cache.1 = [h, s, v];
        let modified = next != *color;
        *color = next;
        modified
    } else {
        false
    }
}

/// Text field look-alike that opens a popup when clicked (font picker).
pub(crate) fn drop_field(ui: &mut Ui, th: &Theme, label: &str) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let t = hover_t(ui, &resp, true);
    let p = ui.painter();
    p.rect_filled(rect, rr(7), blend(th.panel_alt, th.hover, t));
    p.rect_stroke(
        rect,
        rr(7),
        st(1.0, if resp.hovered() { th.accent } else { th.border }),
        StrokeKind::Inside,
    );
    p.text(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        th.text,
    );
    let c = Pos2::new(rect.right() - 12.0, rect.center().y);
    let mut mesh = egui::epaint::Mesh::default();
    for (dx, dy) in [(-4.0, -2.0), (4.0, -2.0), (0.0, 3.0)] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(c.x + dx, c.y + dy),
            uv: egui::epaint::WHITE_UV,
            color: th.text_dim,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2]);
    p.add(egui::Shape::mesh(mesh));
    resp
}
