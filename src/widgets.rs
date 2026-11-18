//! Small custom widgets that give the app its own modern look (egui's stock
//! widgets are used for text input / sliders only).

use eframe::egui::{
    Align2, Color32, FontId, Pos2, Rect, Response, Rounding, Sense, Stroke, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::theme::Theme;

pub fn rr(r: f32) -> Rounding {
    Rounding::same(r)
}

pub fn st(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

fn hover_t(ui: &Ui, resp: &Response) -> f32 {
    ui.ctx().animate_bool(resp.id.with("hov"), resp.hovered())
}

/// Square icon button (tool rail, top bar).
pub fn icon_button(ui: &mut Ui, th: &Theme, icon: Icon, size: f32, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    let bg = if active {
        th.accent_soft
    } else if t > 0.0 {
        blend(th.panel, th.hover, t)
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        p.rect_filled(rect, rr(size * 0.26), bg);
    }
    if resp.is_pointer_button_down_on() && !active {
        p.rect_filled(rect, rr(size * 0.26), th.hover);
    }
    let fg = if active {
        th.accent
    } else {
        blend(th.text_dim, th.text, t)
    };
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(size * 0.58));
    icons::draw(p, icon, glyph, fg, (size * 0.075).clamp(1.2, 1.9));
    resp
}

/// Icon + label button used in dialogs and the top bar.
pub fn button(ui: &mut Ui, th: &Theme, label: &str, primary: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(13.0),
        if primary { th.on_accent } else { th.text },
    );
    let size = Vec2::new(galley.size().x + 26.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    let (bg, fg, border) = if primary {
        (
            blend(th.accent, th.on_accent, t * 0.12),
            th.on_accent,
            Color32::TRANSPARENT,
        )
    } else {
        (
            blend(blend(th.panel, th.raised, t), th.hover, t),
            th.text,
            th.border,
        )
    };
    p.rect_filled(rect, rr(8.0), bg);
    if border != Color32::TRANSPARENT {
        p.rect_stroke(rect, rr(8.0), st(1.0, border));
    }
    let g = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), fg);
    ui.painter().galley(
        Pos2::new(
            rect.center().x - g.size().x * 0.5,
            rect.center().y - g.size().y * 0.5,
        ),
        g,
        fg,
    );
    resp
}

/// Icon + short label, used for the quick-action row in the top bar.
pub fn icon_text_button(
    ui: &mut Ui,
    th: &Theme,
    icon: Icon,
    label: &str,
    primary: bool,
) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), th.text);
    let size = Vec2::new(galley.size().x + 44.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = hover_t(ui, &resp);
    let (bg, fg, border) = if primary {
        (
            blend(th.accent, th.on_accent, t * 0.12),
            th.on_accent,
            Color32::TRANSPARENT,
        )
    } else {
        (
            blend(blend(th.panel, th.raised, t), th.hover, t * 0.6),
            th.text,
            th.border,
        )
    };
    let p = ui.painter();
    p.rect_filled(rect, rr(8.0), bg);
    if border != Color32::TRANSPARENT {
        p.rect_stroke(rect, rr(8.0), st(1.0, border));
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        Vec2::splat(15.0),
    );
    icons::draw(p, icon, icon_rect, fg, 1.5);
    let g = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let text_pos = Pos2::new(rect.left() + 27.0, rect.center().y - g.size().y * 0.5);
    ui.painter().galley(text_pos, g, fg);
    resp
}

/// Checkbox-ish toggle with a sliding knob.
pub fn toggle(ui: &mut Ui, th: &Theme, on: &mut bool, label: &str) -> Response {
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
    p.rect_filled(track, rr(8.0), blend(th.raised, th.accent, t));
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
pub fn slider(
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
                egui::RichText::new(format!("{:.*}", decimals, value))
                    .size(12.0)
                    .color(th.text),
            );
        });
    });
    ui.spacing_mut().slider_width = (ui.available_width() - 6.0).max(40.0);
    let r = ui.add(egui::Slider::new(value, range).show_value(false));
    r.changed()
}

/// Section heading with optional trailing widget space.
pub fn section(ui: &mut Ui, th: &Theme, title: &str) {
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

pub fn divider(ui: &mut Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.left(), rect.center().y),
            Pos2::new(rect.right(), rect.center().y),
        ],
        st(1.0, th.border),
    );
}

/// Colour swatch with selection ring.
pub fn swatch(ui: &mut Ui, th: &Theme, c: Color32, size: f32, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    let inner = rect.shrink(1.5);
    p.rect_filled(inner, rr(4.0), c);
    p.rect_stroke(inner, rr(4.0), st(1.0, Color32::from_black_alpha(40)));
    if selected {
        p.rect_stroke(rect.shrink(0.5), rr(5.0), st(2.0, th.accent));
    } else if resp.hovered() {
        p.rect_stroke(rect.shrink(0.5), rr(5.0), st(1.0, th.text_dim));
    }
    resp
}

/// Draws a checkerboard, used behind transparent canvas pixels.
pub fn checkerboard(p: &eframe::egui::Painter, rect: Rect, cell: f32, a: Color32, b: Color32) {
    p.rect_filled(rect, Rounding::ZERO, a);
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 0.0 } else { cell };
        while x < rect.right() {
            let r = Rect::from_min_max(
                Pos2::new(x, y),
                Pos2::new((x + cell).min(rect.right()), (y + cell).min(rect.bottom())),
            );
            p.rect_filled(r, Rounding::ZERO, b);
            x += cell * 2.0;
        }
        y += cell;
        row += 1;
    }
}

pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

use eframe::egui;

/// A menu row: label on the left, shortcut hint on the right.
pub fn menu_item(ui: &mut Ui, th: &Theme, label: &str, shortcut: &str, enabled: bool) -> bool {
    menu_item_icon(ui, th, None, label, shortcut, enabled)
}

pub fn menu_item_icon(
    ui: &mut Ui,
    th: &Theme,
    icon: Option<Icon>,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> bool {
    let width = ui.available_width().clamp(180.0, 280.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let text_color = if enabled { th.text } else { th.text_dim };
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_filled(rect, rr(6.0), th.hover);
    }
    let mut left = rect.left() + 8.0;
    if let Some(ic) = icon {
        let ir = Rect::from_center_size(Pos2::new(left + 7.0, rect.center().y), Vec2::splat(14.0));
        icons::draw(p, ic, ir, text_color, 1.4);
        left += 22.0;
    }
    p.text(
        Pos2::new(left, rect.center().y),
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

pub fn menu_sep(ui: &mut Ui, th: &Theme) {
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

/// Modal shell: dims the app, centres a card, and returns the closure result.
pub fn modal<R>(
    ctx: &egui::Context,
    th: &Theme,
    id: &str,
    size: Vec2,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new(("dim", id)))
        .order(egui::Order::Middle)
        .fixed_pos(Pos2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let resp = ui.allocate_rect(screen, Sense::click_and_drag());
            let alpha = if th.is_dark() { 160 } else { 90 };
            ui.painter()
                .rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(alpha));
            // swallow clicks so the canvas below is not modified
            let _ = resp;
        });
    let pos = Pos2::new(
        screen.center().x - size.x * 0.5,
        screen.center().y - size.y * 0.5,
    );
    egui::Area::new(egui::Id::new(("modal", id)))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(th.panel)
                .rounding(Rounding::same(16.0))
                .stroke(st(1.0, th.border))
                .shadow(egui::epaint::Shadow {
                    offset: Vec2::new(0.0, 14.0),
                    blur: 36.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(if th.is_dark() { 140 } else { 60 }),
                })
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_width(size.x - 36.0);
                    add(ui)
                })
                .inner
        })
        .inner
}

/// Title row with a close button, used by the modal dialogs.
pub fn modal_header(ui: &mut Ui, th: &Theme, title: &str) -> bool {
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

/// Compact HSV colour picker: saturation/value square plus a hue strip.
/// egui's stock picker needs ~340 px, which does not fit our 258 px side panel.
pub fn color_picker(ui: &mut Ui, th: &Theme, color: &mut Color32) -> bool {
    use eframe::egui::ecolor::Hsva;
    use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};
    use eframe::egui::TextureId;

    let mut hsv = Hsva::from_srgb([color.r(), color.g(), color.b()]);
    let w = ui.available_width();
    let mut changed = false;

    // --- saturation / value square -------------------------------------------
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 118.0), Sense::click_and_drag());
    let hue = Hsva {
        h: hsv.h,
        s: 1.0,
        v: 1.0,
        a: 1.0,
    };
    let hue_c: Color32 = hue.into();
    let mut mesh = Mesh {
        texture_id: TextureId::default(),
        ..Default::default()
    };
    let pts = [
        (rect.left_top(), Color32::WHITE),
        (rect.right_top(), hue_c),
        (rect.left_bottom(), Color32::BLACK),
        (rect.right_bottom(), Color32::BLACK),
    ];
    for (p, c) in pts {
        mesh.vertices.push(Vertex {
            pos: p,
            uv: WHITE_UV,
            color: c,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    ui.painter().add(eframe::egui::Shape::mesh(mesh));
    if resp.is_pointer_button_down_on() || resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            hsv.s = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            hsv.v = 1.0 - ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            changed = true;
        }
    }
    let knob = Pos2::new(
        rect.left() + hsv.s * rect.width(),
        rect.top() + (1.0 - hsv.v) * rect.height(),
    );
    ui.painter()
        .circle_stroke(knob, 6.0, st(2.0, Color32::WHITE));
    ui.painter()
        .circle_stroke(knob, 7.5, st(1.0, Color32::from_black_alpha(90)));

    ui.add_space(8.0);

    // --- hue strip -----------------------------------------------------------
    let (hrect, hresp) = ui.allocate_exact_size(Vec2::new(w, 12.0), Sense::click_and_drag());
    let segs = 48;
    for i in 0..segs {
        let t0 = i as f32 / segs as f32;
        let x0 = hrect.left() + t0 * hrect.width();
        let x1 = hrect.left() + (i + 1) as f32 / segs as f32 * hrect.width();
        let c: Color32 = Hsva {
            h: t0,
            s: 1.0,
            v: 1.0,
            a: 1.0,
        }
        .into();
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(x0, hrect.top()),
                Pos2::new(x1 + 1.0, hrect.bottom()),
            ),
            Rounding::ZERO,
            c,
        );
    }
    ui.painter().rect_stroke(hrect, rr(2.0), st(1.0, th.border));
    if hresp.is_pointer_button_down_on() || hresp.dragged() {
        if let Some(p) = hresp.interact_pointer_pos() {
            hsv.h = ((p.x - hrect.left()) / hrect.width()).clamp(0.0, 1.0);
            changed = true;
        }
    }
    let hx = hrect.left() + hsv.h * hrect.width();
    let mark = Rect::from_min_max(
        Pos2::new(hx - 3.0, hrect.top() - 2.0),
        Pos2::new(hx + 3.0, hrect.bottom() + 2.0),
    );
    ui.painter()
        .rect_stroke(mark, rr(2.0), st(2.0, Color32::WHITE));
    ui.painter()
        .rect_stroke(mark, rr(2.0), st(1.0, Color32::from_black_alpha(90)));

    if changed {
        *color = hsv.into();
    }
    changed
}

/// Text field look-alike that opens a popup when clicked (font picker).
pub fn drop_field(ui: &mut Ui, th: &Theme, label: &str) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    p.rect_filled(rect, rr(7.0), blend(th.panel_alt, th.hover, t));
    p.rect_stroke(
        rect,
        rr(7.0),
        st(1.0, if resp.hovered() { th.accent } else { th.border }),
    );
    p.text(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        th.text,
    );
    // caret
    let c = Pos2::new(rect.right() - 12.0, rect.center().y);
    let mut mesh = eframe::egui::epaint::Mesh::default();
    let col = th.text_dim;
    for (dx, dy) in [(-4.0, -2.0), (4.0, -2.0), (0.0, 3.0)] {
        mesh.vertices.push(eframe::egui::epaint::Vertex {
            pos: Pos2::new(c.x + dx, c.y + dy),
            uv: eframe::egui::epaint::WHITE_UV,
            color: col,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2]);
    p.add(eframe::egui::Shape::mesh(mesh));
    resp
}
