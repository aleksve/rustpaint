//! Tool definitions and per-tool options.

use eframe::egui::{Color32, Key};

use crate::icons::Icon;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tool {
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
            Tool::Pencil => "Hard edged freehand drawing",
            Tool::Brush => "Soft round brush with adjustable hardness",
            Tool::Eraser => "Erase to transparency",
            Tool::Airbrush => "Spray paint, hold still to build up",
            Tool::Fill => "Fill a contiguous area of similar colour",
            Tool::Picker => "Pick a colour from the canvas",
            Tool::Text => "Click to place text, then type",
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

    pub fn uses_softness(self) -> bool {
        matches!(self, Tool::Brush | Tool::Airbrush)
    }

    pub fn uses_fill_toggle(self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse | Tool::Polygon)
    }
}

#[derive(Clone, Debug)]
pub struct ToolOptions {
    pub size: f32,
    pub opacity: f32,
    /// 0 = very soft edge, 1 = hard edge (brush / airbrush only).
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

pub struct Colors {
    pub primary: Color32,
    pub secondary: Color32,
    pub recent: Vec<Color32>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            primary: Color32::from_rgb(0x1a, 0x1c, 0x22),
            secondary: Color32::WHITE,
            recent: palette().iter().take(8 * 2).cloned().collect(),
        }
    }
}

impl Colors {
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.primary, &mut self.secondary);
    }

    pub fn push_recent(&mut self, c: Color32) {
        self.recent.retain(|e| *e != c);
        self.recent.insert(0, c);
        self.recent.truncate(16);
    }
}

/// The classic MS Paint palette plus a few modern extras, 2 rows of 14.
pub fn palette() -> Vec<Color32> {
    let hex = [
        0x000000, 0x7f7f7f, 0x880015, 0xed1c24, 0xff7f27, 0xfff200, 0x22b14c, 0x00a2e8, 0x3f48cc,
        0xa349a4, 0xffffff, 0xc3c3c3, 0xb97a57, 0xffaec9, 0xffc90e, 0xefe4b0, 0xb5e61d, 0x99d9ea,
        0x7092be, 0xc8bfe7, 0xf5f5f7, 0x9aa3b2, 0x6b4f3a, 0xe0727f, 0xf2b33d, 0xcfd8a0, 0x7fb069,
        0x4aa3c7, 0x5b6ee1, 0x9b6bd6,
    ];
    hex.iter()
        .map(|v| {
            Color32::from_rgb(
                ((v >> 16) & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                (v & 0xff) as u8,
            )
        })
        .collect()
}

pub fn to_hex(c: Color32) -> String {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).ok()?;
    match s.len() {
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
