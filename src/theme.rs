//! Modern light / dark theming built on top of egui's `Visuals`.

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Rounding, Stroke, TextStyle, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub mode: Mode,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub canvas_bg: Color32,
    pub danger: Color32,
    pub shadow: Color32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            panel: Color32::from_rgb(0x17, 0x1a, 0x21),
            panel_alt: Color32::from_rgb(0x1c, 0x20, 0x28),
            raised: Color32::from_rgb(0x23, 0x28, 0x32),
            hover: Color32::from_rgb(0x2c, 0x33, 0x40),
            border: Color32::from_rgb(0x2e, 0x35, 0x42),
            text: Color32::from_rgb(0xe9, 0xec, 0xf2),
            text_dim: Color32::from_rgb(0x98, 0xa2, 0xb3),
            accent: Color32::from_rgb(0x4c, 0x8d, 0xff),
            accent_soft: Color32::from_rgb(0x25, 0x39, 0x5c),
            on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
            canvas_bg: Color32::from_rgb(0x0a, 0x0c, 0x10),
            danger: Color32::from_rgb(0xff, 0x6b, 0x62),
            shadow: Color32::from_black_alpha(120),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: Mode::Light,
            panel: Color32::from_rgb(0xff, 0xff, 0xff),
            panel_alt: Color32::from_rgb(0xf6, 0xf7, 0xfa),
            raised: Color32::from_rgb(0xe9, 0xec, 0xf2),
            hover: Color32::from_rgb(0xdc, 0xe2, 0xec),
            border: Color32::from_rgb(0xd3, 0xd9, 0xe3),
            text: Color32::from_rgb(0x1a, 0x1d, 0x24),
            text_dim: Color32::from_rgb(0x67, 0x70, 0x80),
            accent: Color32::from_rgb(0x24, 0x63, 0xeb),
            accent_soft: Color32::from_rgb(0xdb, 0xe6, 0xfd),
            on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
            canvas_bg: Color32::from_rgb(0xd9, 0xdd, 0xe5),
            danger: Color32::from_rgb(0xd9, 0x2d, 0x20),
            shadow: Color32::from_black_alpha(40),
        }
    }

    pub fn is_dark(&self) -> bool {
        matches!(self.mode, Mode::Dark)
    }

    pub fn visuals(&self) -> egui::Visuals {
        let mut v = if self.is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = self.panel;
        v.window_fill = self.panel;
        v.extreme_bg_color = if self.is_dark() {
            Color32::from_rgb(0x10, 0x12, 0x18)
        } else {
            Color32::from_rgb(0xf2, 0xf4, 0xf8)
        };
        v.faint_bg_color = self.panel_alt;
        v.override_text_color = Some(self.text);
        v.window_rounding = Rounding::same(14.0);
        v.menu_rounding = Rounding::same(10.0);
        v.window_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 10.0),
            blur: 30.0,
            spread: 0.0,
            color: self.shadow,
        };
        v.popup_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 6.0),
            blur: 18.0,
            spread: 0.0,
            color: self.shadow,
        };
        v.selection.bg_fill = self.accent_soft;
        v.selection.stroke = Stroke::new(1.0f32, self.accent);
        v.hyperlink_color = self.accent;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.panel;
        w.noninteractive.weak_bg_fill = self.panel_alt;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text_dim);
        w.noninteractive.rounding = Rounding::same(8.0);

        w.inactive.bg_fill = self.raised;
        w.inactive.weak_bg_fill = self.panel_alt;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, self.border);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, self.text);
        w.inactive.rounding = Rounding::same(8.0);
        w.inactive.expansion = 0.0;

        w.hovered.bg_fill = self.hover;
        w.hovered.weak_bg_fill = self.hover;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, self.border);
        w.hovered.fg_stroke = Stroke::new(1.0_f32, self.text);
        w.hovered.rounding = Rounding::same(8.0);
        w.hovered.expansion = 0.0;

        w.active.bg_fill = self.accent;
        w.active.weak_bg_fill = self.accent;
        w.active.bg_stroke = Stroke::new(1.0_f32, self.accent);
        w.active.fg_stroke = Stroke::new(1.0_f32, self.on_accent);
        w.active.rounding = Rounding::same(8.0);

        w.open.bg_fill = self.raised;
        w.open.weak_bg_fill = self.raised;
        w.open.bg_stroke = Stroke::new(1.0_f32, self.border);
        w.open.fg_stroke = Stroke::new(1.0_f32, self.text);
        w.open.rounding = Rounding::same(8.0);
        v
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ui".to_owned(),
        egui::FontData::from_static(crate::assets::UI_FONT),
    );
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "ui".to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, t: &Theme) {
    let mut style = (*ctx.style()).clone();
    style.visuals = t.visuals();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.interact_size = Vec2::new(32.0, 26.0);
    style.spacing.indent = 16.0;
    let prop = FontFamily::Proportional;
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, prop.clone())),
        (TextStyle::Body, FontId::new(13.0, prop.clone())),
        (TextStyle::Button, FontId::new(13.0, prop.clone())),
        (TextStyle::Heading, FontId::new(17.0, prop.clone())),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into_iter()
    .collect();
    ctx.set_style(style);
}
