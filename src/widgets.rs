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

/// Icon + short label, used for the quick-action row in the top bar.
pub(crate) fn icon_text_button(
    ui: &mut Ui,
    th: &Theme,
    icon: Icon,
    label: &str,
    primary: bool,
) -> Response {
    let fg = if primary { th.on_accent } else { th.text };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(12.5), fg);
    let galley_size = galley.size();
    let size = Vec2::new(galley_size.x + 44.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = hover_t(ui, &resp, true);
    let (bg, border) = if primary {
        (
            blend(th.accent, th.on_accent, t * 0.12),
            Color32::TRANSPARENT,
        )
    } else {
        (
            blend(blend(th.panel, th.raised, t), th.hover, t * 0.6),
            th.border,
        )
    };
    let p = ui.painter();
    p.rect_filled(rect, rr(8), bg);
    if border != Color32::TRANSPARENT {
        p.rect_stroke(rect, rr(8), st(1.0, border), StrokeKind::Inside);
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        Vec2::splat(15.0),
    );
    icons::draw(p, icon, icon_rect, fg, 1.5);
    let text_pos = Pos2::new(rect.left() + 27.0, rect.center().y - galley_size.y * 0.5);
    p.galley(text_pos, galley, fg);
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
    let prev_slider_w = ui.spacing().slider_width;
    let prev_interact_y = ui.spacing().interact_size.y;
    ui.spacing_mut().slider_width = ui.available_width().max(40.0);
    ui.spacing_mut().interact_size.y = 20.0;
    let changed = ui
        .add(egui::Slider::new(value, range).show_value(false))
        .changed();
    ui.spacing_mut().slider_width = prev_slider_w;
    ui.spacing_mut().interact_size.y = prev_interact_y;
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

/// Colour swatch with selection ring, aligned flush to its allocated rect.
pub(crate) fn swatch(ui: &mut Ui, th: &Theme, c: Color32, size: f32, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    let chip = rect.shrink(0.5);
    paint_color_chip(p, th, chip, rr(4), c);
    p.rect_stroke(
        chip,
        rr(4),
        st(1.0, Color32::from_black_alpha(45)),
        StrokeKind::Inside,
    );
    if selected {
        p.rect_stroke(chip, rr(4), st(1.8, th.accent), StrokeKind::Outside);
    } else if resp.hovered() {
        p.rect_stroke(chip, rr(4), st(1.2, th.text_dim), StrokeKind::Outside);
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
        Vec2::new(ui.available_width().clamp(180.0, 280.0), 9.0),
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

pub(crate) fn srgb_to_hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    if delta <= 1e-6 || max <= 1e-6 {
        return [0.0, 0.0, max];
    }
    let s = delta / max;
    let h = if (max - rf).abs() <= 1e-6 {
        (gf - bf) / delta
    } else if (max - gf).abs() <= 1e-6 {
        2.0 + (bf - rf) / delta
    } else {
        4.0 + (rf - gf) / delta
    };
    [(h / 6.0).rem_euclid(1.0), s, max]
}

pub(crate) fn hsv_to_srgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let [hr, hg, hb] = hue_to_rgb(h);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0) * 255.0;
    [
        (v * (1.0 - s * (1.0 - hr))).round() as u8,
        (v * (1.0 - s * (1.0 - hg))).round() as u8,
        (v * (1.0 - s * (1.0 - hb))).round() as u8,
    ]
}

fn hue_to_rgb(h: f32) -> [f32; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor() as u32;
    let f = h6 - i as f32;
    match i {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// Anti-aliased rounded-rectangle coverage (`0..=255`) at pixel `(x, y)`.
#[inline]
fn rounded_corner_alpha(x: usize, y: usize, w: usize, h: usize, r: f32) -> u8 {
    let px = x as f32 + 0.5;
    let py = y as f32 + 0.5;
    let wf = w as f32;
    let hf = h as f32;
    if (px >= r && px <= wf - r) || (py >= r && py <= hf - r) {
        return 255;
    }
    let cx = if px < r { r } else { wf - r };
    let cy = if py < r { r } else { hf - r };
    // Tuck the fill arc by 0.6 px so it terminates cleanly inside the 1 pt
    // `StrokeKind::Inside` border with zero outer colour fringe.
    let dist = (px - cx).hypot(py - cy) - (r - 0.6).max(0.0);
    ((0.5 - dist).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn build_sv_image(h: f32, w: usize, height: usize, radius_px: f32) -> ColorImage {
    let [hr, hg, hb] = hue_to_rgb(h);
    let r_lim = radius_px.min(w.min(height) as f32 * 0.5);
    let inv_w = 1.0 / w.saturating_sub(1).max(1) as f32;
    let inv_h = 1.0 / height.saturating_sub(1).max(1) as f32;
    let mut pixels = Vec::with_capacity(w * height);
    for y in 0..height {
        let v = (1.0 - y as f32 * inv_h) * 255.0;
        let corner_row = (y as f32 + 0.5) < r_lim || (y as f32 + 0.5) > height as f32 - r_lim;
        for x in 0..w {
            let s = x as f32 * inv_w;
            let r = (v * (1.0 - s * (1.0 - hr))).round() as u8;
            let g = (v * (1.0 - s * (1.0 - hg))).round() as u8;
            let b = (v * (1.0 - s * (1.0 - hb))).round() as u8;
            let a = if corner_row {
                rounded_corner_alpha(x, y, w, height, r_lim)
            } else {
                255
            };
            pixels.push(Color32::from_rgba_unmultiplied(r, g, b, a));
        }
    }
    ColorImage {
        size: [w, height],
        source_size: Vec2::new(w as f32, height as f32),
        pixels,
    }
}

fn build_hue_image(w: usize, height: usize, radius_px: f32) -> ColorImage {
    let r_lim = radius_px.min(w.min(height) as f32 * 0.5);
    let inv_w = 1.0 / w.saturating_sub(1).max(1) as f32;
    let row_rgb: Vec<[u8; 3]> = (0..w)
        .map(|x| hsv_to_srgb((x as f32 * inv_w).min(0.9999), 1.0, 1.0))
        .collect();
    let mut pixels = Vec::with_capacity(w * height);
    for y in 0..height {
        let corner_row = (y as f32 + 0.5) < r_lim || (y as f32 + 0.5) > height as f32 - r_lim;
        for (x, &[r, g, b]) in row_rgb.iter().enumerate() {
            let a = if corner_row {
                rounded_corner_alpha(x, y, w, height, r_lim)
            } else {
                255
            };
            pixels.push(Color32::from_rgba_unmultiplied(r, g, b, a));
        }
    }
    ColorImage {
        size: [w, height],
        source_size: Vec2::new(w as f32, height as f32),
        pixels,
    }
}

#[derive(Clone)]
struct PickerTexCache {
    sv_key: (u16, usize, usize, u32),
    sv_tex: egui::TextureHandle,
    hue_key: (usize, usize, u32),
    hue_tex: egui::TextureHandle,
}

/// Full-width sRGB HSV colour picker with persistent `[h, s, v]` state (so
/// picking white, black, or grey never loses the chosen hue) and cached,
/// HiDPI-exact rounded SV and hue gradient textures.
pub(crate) fn color_picker(
    ui: &mut Ui,
    th: &Theme,
    color: &mut Color32,
    hsv_cache: &mut (Color32, [f32; 3]),
) -> bool {
    const SV_RADIUS: u8 = 5;
    const HUE_RADIUS: u8 = 6;
    const UV_FULL: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

    // Resync `[h, s, v]` only when the colour was modified outside the picker
    // (palette swatch, hex box, eyedropper, swap/reset).
    if hsv_cache.0 != *color {
        let [ch, cs, cv] = srgb_to_hsv([color.r(), color.g(), color.b()]);
        let prev_h = hsv_cache.1[0];
        let h = if cs <= 1e-4 || cv <= 1e-4 { prev_h } else { ch };
        hsv_cache.0 = *color;
        hsv_cache.1 = [h, cs, cv];
    }

    let [mut h, mut s, mut v] = hsv_cache.1;
    let w = ui.available_width().max(48.0);
    let mut changed = false;

    // --- saturation / value field --------------------------------------------
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 124.0), Sense::click_and_drag());
    if (resp.is_pointer_button_down_on() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos()
    {
        s = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        v = 1.0 - ((p.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
        changed = true;
    }

    ui.add_space(9.0);

    // --- rainbow hue strip ---------------------------------------------------
    let (hrect, hresp) = ui.allocate_exact_size(Vec2::new(w, 13.0), Sense::click_and_drag());
    if (hresp.is_pointer_button_down_on() || hresp.dragged())
        && let Some(p) = hresp.interact_pointer_pos()
    {
        h = ((p.x - hrect.left()) / hrect.width().max(1.0)).clamp(0.0, 1.0);
        changed = true;
    }

    // Build / reuse cached HiDPI-rounded SV & hue textures (recomputed only
    // when hue, width, or DPI scale changes).
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point().clamp(1.0, 4.0);
    let sv_r_px = SV_RADIUS as f32 * ppp;
    let hue_r_px = HUE_RADIUS as f32 * ppp;
    let sv_pw = ((rect.width() * ppp).round() as usize).clamp(16, 1024);
    let sv_ph = ((rect.height() * ppp).round() as usize).clamp(16, 1024);
    let hue_pw = ((hrect.width() * ppp).round() as usize).clamp(16, 1024);
    let hue_ph = ((hrect.height() * ppp).round() as usize).clamp(8, 256);
    let hue_q = ((h.rem_euclid(1.0) * 1536.0).round() as u16) % 1536;
    let sv_key = (hue_q, sv_pw, sv_ph, (sv_r_px * 16.0).round() as u32);
    let hue_key = (hue_pw, hue_ph, (hue_r_px * 16.0).round() as u32);

    let cache_id = ui.make_persistent_id("hsv-picker-tex");
    let cached = ctx.data(|data| data.get_temp::<PickerTexCache>(cache_id));
    let (sv_id, hue_id) = if let Some(mut entry) = cached {
        let mut updated = false;
        if entry.sv_key != sv_key {
            entry.sv_tex.set(
                build_sv_image(h, sv_pw, sv_ph, sv_r_px),
                egui::TextureOptions::LINEAR,
            );
            entry.sv_key = sv_key;
            updated = true;
        }
        if entry.hue_key != hue_key {
            entry.hue_tex.set(
                build_hue_image(hue_pw, hue_ph, hue_r_px),
                egui::TextureOptions::LINEAR,
            );
            entry.hue_key = hue_key;
            updated = true;
        }
        let ids = (entry.sv_tex.id(), entry.hue_tex.id());
        if updated {
            ctx.data_mut(|data| data.insert_temp(cache_id, entry));
        }
        ids
    } else {
        let sv_tex = ctx.load_texture(
            "picker-sv",
            build_sv_image(h, sv_pw, sv_ph, sv_r_px),
            egui::TextureOptions::LINEAR,
        );
        let hue_tex = ctx.load_texture(
            "picker-hue",
            build_hue_image(hue_pw, hue_ph, hue_r_px),
            egui::TextureOptions::LINEAR,
        );
        let ids = (sv_tex.id(), hue_tex.id());
        ctx.data_mut(|data| {
            data.insert_temp(
                cache_id,
                PickerTexCache {
                    sv_key,
                    sv_tex,
                    hue_key,
                    hue_tex,
                },
            );
        });
        ids
    };

    let p = ui.painter();
    p.image(sv_id, rect, UV_FULL, Color32::WHITE);
    p.rect_stroke(rect, rr(SV_RADIUS), st(1.0, th.border), StrokeKind::Inside);

    p.image(hue_id, hrect, UV_FULL, Color32::WHITE);
    p.rect_stroke(
        hrect,
        rr(HUE_RADIUS),
        st(1.0, th.border),
        StrokeKind::Inside,
    );

    let knob = Pos2::new(
        rect.left() + s * rect.width(),
        rect.top() + (1.0 - v) * rect.height(),
    );
    p.circle_stroke(knob, 7.0, st(1.0, Color32::from_black_alpha(110)));
    p.circle_stroke(knob, 5.5, st(2.0, Color32::WHITE));
    p.circle_stroke(knob, 4.2, st(1.0, Color32::from_black_alpha(65)));

    let hx = hrect.left() + h * hrect.width();
    let mark = Rect::from_min_max(
        Pos2::new(hx - 3.0, hrect.top() - 2.0),
        Pos2::new(hx + 3.0, hrect.bottom() + 2.0),
    );
    p.rect_stroke(
        mark,
        rr(3),
        st(1.0, Color32::from_black_alpha(110)),
        StrokeKind::Outside,
    );
    p.rect_stroke(mark, rr(3), st(2.0, Color32::WHITE), StrokeKind::Inside);

    if changed {
        let [r, g, b] = hsv_to_srgb(h, s, v);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::PALETTE;

    #[test]
    fn srgb_hsv_roundtrips_palette_and_extremes() {
        let extras = [
            Color32::from_rgb(0x1a, 0x1c, 0x22),
            Color32::BLACK,
            Color32::WHITE,
            Color32::from_rgb(0x80, 0x80, 0x80),
            Color32::from_rgb(0xff, 0x00, 0x00),
            Color32::from_rgb(0x00, 0xff, 0x00),
            Color32::from_rgb(0x00, 0x00, 0xff),
        ];
        for c in PALETTE.iter().copied().chain(extras) {
            let src = [c.r(), c.g(), c.b()];
            let [h, s, v] = srgb_to_hsv(src);
            assert_eq!(hsv_to_srgb(h, s, v), src, "roundtrip mismatch for {src:?}");
        }
        // Hue wraps seamlessly at both ends of the slider.
        assert_eq!(hsv_to_srgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_srgb(1.0, 1.0, 1.0), [255, 0, 0]);
    }

    #[test]
    fn picker_gradient_images_mask_outer_corners_cleanly() {
        let sv = build_sv_image(0.0, 128, 64, 10.0);
        assert_eq!(sv.pixels[0].a(), 0);
        assert_eq!(sv.pixels[127].a(), 0);
        assert_eq!(sv.pixels[64 * 128 - 1].a(), 0);
        assert_eq!(sv.pixels[32 * 128 + 64].a(), 255);

        let hue = build_hue_image(128, 26, 12.0);
        assert_eq!(hue.pixels[0].a(), 0);
        assert_eq!(hue.pixels[127].a(), 0);
        assert_eq!(hue.pixels[13 * 128 + 64].a(), 255);
    }
}
