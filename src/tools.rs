//! Tool definitions, tool options, and colour palette helpers.

use eframe::egui::{Color32, Key};

use crate::icons::Icon;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Tool {
    Select,
    Pencil,
    Brush,
    Eraser,
    Airbrush,
    Fill,
    Picker,
    Text,
    Line,
    Rect,
    Ellipse,
    Polygon,
    Pan,
}

impl Tool {
    pub const ALL: [Tool; 13] = [
        Tool::Select,
        Tool::Pencil,
        Tool::Brush,
        Tool::Eraser,
        Tool::Airbrush,
        Tool::Fill,
        Tool::Picker,
        Tool::Text,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Polygon,
        Tool::Pan,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Pencil => "Pencil",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Airbrush => "Airbrush",
            Tool::Fill => "Fill",
            Tool::Picker => "Colour picker",
            Tool::Text => "Text",
            Tool::Line => "Line",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Pan => "Pan",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Pencil => "P",
            Tool::Brush => "B",
            Tool::Eraser => "E",
            Tool::Airbrush => "A",
            Tool::Fill => "G",
            Tool::Picker => "I",
            Tool::Text => "T",
            Tool::Line => "L",
            Tool::Rect => "R",
            Tool::Ellipse => "O",
            Tool::Polygon => "K",
            Tool::Pan => "H",
        }
    }

    pub fn shortcut_key(self) -> Key {
        match self {
            Tool::Select => Key::V,
            Tool::Pencil => Key::P,
            Tool::Brush => Key::B,
            Tool::Eraser => Key::E,
            Tool::Airbrush => Key::A,
            Tool::Fill => Key::G,
            Tool::Picker => Key::I,
            Tool::Text => Key::T,
            Tool::Line => Key::L,
            Tool::Rect => Key::R,
            Tool::Ellipse => Key::O,
            Tool::Polygon => Key::K,
            Tool::Pan => Key::H,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Pencil => Icon::Pencil,
            Tool::Brush => Icon::Brush,
            Tool::Eraser => Icon::Eraser,
            Tool::Airbrush => Icon::Spray,
            Tool::Fill => Icon::Bucket,
            Tool::Picker => Icon::Dropper,
            Tool::Text => Icon::Text,
            Tool::Line => Icon::Line,
            Tool::Rect => Icon::Rect,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Polygon => Icon::Polygon,
            Tool::Pan => Icon::Hand,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => {
                "Drag to select · drag inside to move · Ctrl+C / Ctrl+V to copy & paste"
            }
            Tool::Pencil => "Hard-edged pixel drawing",
            Tool::Brush => "Round brush with adjustable hardness and anti-aliasing",
            Tool::Eraser => "Erase to transparency",
            Tool::Airbrush => "Spray paint, hold still to build up",
            Tool::Fill => "Fill a contiguous area of similar colour",
            Tool::Picker => "Pick a colour from the canvas",
            Tool::Text => "Click to place text, type, then click Apply or press Ctrl+Enter",
            Tool::Line => "Drag for a straight line (Shift = 45° steps)",
            Tool::Rect => "Drag a rectangle (Shift = square)",
            Tool::Ellipse => "Drag an ellipse (Shift = circle)",
            Tool::Polygon => "Click to add points, Enter or double-click to finish",
            Tool::Pan => "Drag to pan the view",
        }
    }

    pub fn is_freehand(self) -> bool {
        matches!(
            self,
            Tool::Pencil | Tool::Brush | Tool::Eraser | Tool::Airbrush
        )
    }

    pub fn is_shape(self) -> bool {
        matches!(self, Tool::Line | Tool::Rect | Tool::Ellipse)
    }

    pub fn uses_size(self) -> bool {
        !matches!(
            self,
            Tool::Select | Tool::Fill | Tool::Picker | Tool::Pan | Tool::Text
        )
    }

    pub fn max_size(self) -> f32 {
        if self == Tool::Pencil { 64.0 } else { 300.0 }
    }

    pub fn default_size(self) -> f32 {
        match self {
            Tool::Pencil => 2.0,
            Tool::Brush => 12.0,
            Tool::Eraser => 24.0,
            Tool::Airbrush => 28.0,
            Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Polygon => 4.0,
            _ => 12.0,
        }
    }

    pub fn uses_softness(self) -> bool {
        matches!(self, Tool::Brush | Tool::Eraser | Tool::Airbrush)
    }

    pub fn uses_antialias(self) -> bool {
        matches!(self, Tool::Brush | Tool::Line)
    }

    pub fn uses_opacity(self) -> bool {
        !matches!(self, Tool::Select | Tool::Fill | Tool::Picker | Tool::Pan)
    }

    pub fn uses_fill_toggle(self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse | Tool::Polygon)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ToolOptions {
    pub size: f32,
    pub opacity: f32,
    /// 0 = very soft edge, 1 = hard edge (brush / eraser / airbrush).
    pub hardness: f32,
    pub antialias: bool,
    /// Shape tools: fill instead of outline.
    pub filled: bool,
    /// Bucket fill tolerance, 0..=255.
    pub tolerance: u8,
    pub font_size: f32,
    pub bold: bool,
    /// Family name of the canvas text face (see `fonts::FontCatalog`).
    pub font_family: String,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            size: 12.0,
            opacity: 1.0,
            hardness: 0.85,
            antialias: true,
            filled: false,
            tolerance: 0,
            font_size: 32.0,
            bold: false,
            font_family: crate::fonts::BUILTIN_INTER.to_string(),
        }
    }
}

pub(crate) const DEFAULT_PRIMARY: Color32 = Color32::from_rgb(0x1a, 0x1c, 0x22);
pub(crate) const DEFAULT_SECONDARY: Color32 = Color32::WHITE;

pub(crate) struct Colors {
    pub primary: Color32,
    pub secondary: Color32,
    pub recent: Vec<Color32>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            primary: DEFAULT_PRIMARY,
            secondary: DEFAULT_SECONDARY,
            recent: PALETTE[..16].to_vec(),
        }
    }
}

impl Colors {
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.primary, &mut self.secondary);
    }

    pub fn reset(&mut self) {
        self.primary = DEFAULT_PRIMARY;
        self.secondary = DEFAULT_SECONDARY;
    }

    pub fn push_recent(&mut self, c: Color32) {
        if self.recent.first() == Some(&c) {
            return;
        }
        self.recent.retain(|e| *e != c);
        self.recent.insert(0, c);
        self.recent.truncate(16);
    }
}

const fn hex_rgb(v: u32) -> Color32 {
    Color32::from_rgb(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
}

/// The classic MS Paint palette plus modern muted tones (3 rows of 10).
pub(crate) const PALETTE: [Color32; 30] = [
    hex_rgb(0x000000),
    hex_rgb(0x7f7f7f),
    hex_rgb(0x880015),
    hex_rgb(0xed1c24),
    hex_rgb(0xff7f27),
    hex_rgb(0xfff200),
    hex_rgb(0x22b14c),
    hex_rgb(0x00a2e8),
    hex_rgb(0x3f48cc),
    hex_rgb(0xa349a4),
    hex_rgb(0xffffff),
    hex_rgb(0xc3c3c3),
    hex_rgb(0xb97a57),
    hex_rgb(0xffaec9),
    hex_rgb(0xffc90e),
    hex_rgb(0xefe4b0),
    hex_rgb(0xb5e61d),
    hex_rgb(0x99d9ea),
    hex_rgb(0x7092be),
    hex_rgb(0xc8bfe7),
    hex_rgb(0xf5f5f7),
    hex_rgb(0x9aa3b2),
    hex_rgb(0x6b4f3a),
    hex_rgb(0xe0727f),
    hex_rgb(0xf2b33d),
    hex_rgb(0xcfd8a0),
    hex_rgb(0x7fb069),
    hex_rgb(0x4aa3c7),
    hex_rgb(0x5b6ee1),
    hex_rgb(0x9b6bd6),
];

pub(crate) fn to_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

pub(crate) fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if !matches!(s.len(), 3 | 6 | 8) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    match s.len() {
        3 => {
            let r = ((v >> 8) & 0xf) as u8;
            let g = ((v >> 4) & 0xf) as u8;
            let b = (v & 0xf) as u8;
            Some(Color32::from_rgb(r * 17, g * 17, b * 17))
        }
        6 => Some(Color32::from_rgb(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            ((v >> 24) & 0xff) as u8,
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_handles_valid_forms_and_rejects_sign_prefix() {
        assert_eq!(parse_hex("#F0A"), Some(Color32::from_rgb(0xff, 0x00, 0xaa)));
        assert_eq!(
            parse_hex("#123456"),
            Some(Color32::from_rgb(0x12, 0x34, 0x56))
        );
        assert_eq!(
            parse_hex("#12345680"),
            Some(Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0x80))
        );
        assert_eq!(parse_hex("+12345"), None);
        assert_eq!(parse_hex("#12345"), None);
    }
}
