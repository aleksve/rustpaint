//! A tiny hand-drawn vector icon set, so the app ships no image assets and
//! stays crisp at any DPI on every platform.

use eframe::egui::{Color32, IconData, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // small icon library: keep spare glyphs available for menus/dialogs
pub(crate) enum Icon {
    Pencil,
    Brush,
    Eraser,
    Spray,
    Bucket,
    Dropper,
    Text,
    Line,
    Rect,
    Ellipse,
    Polygon,
    Select,
    Hand,
    Undo,
    Redo,
    New,
    Open,
    Save,
    Plus,
    Minus,
    Grid,
    Fit,
    Sun,
    Moon,
    Close,
    ChevronDown,
    Swap,
    Trash,
    Copy,
    Cut,
    Paste,
    FlipH,
    FlipV,
    RotateCw,
    Invert,
    Crop,
    Resize,
    Info,
    Check,
    ZoomIn,
    ZoomOut,
    Palette,
    Sparkle,
    File,
    Folder,
    Upload,
}

/// Generate the application icon without shipping a platform-specific image.
pub(crate) fn app_icon() -> IconData {
    let n = 128usize;
    let mut rgba = vec![0u8; n * n * 4];
    let (from, to) = ([47u8, 123, 245], [122u8, 168, 255]);
    let side = n as f32;
    for y in 0..n {
        let fy = y as f32 + 0.5;
        for x in 0..n {
            let fx = x as f32 + 0.5;
            let cov = round_rect_coverage(fx, fy, 4.0, 4.0, side - 4.0, side - 4.0, 26.0);
            if cov <= 0.001 {
                continue;
            }
            let t = ((fx + fy) / (2.0 * side)).clamp(0.0, 1.0);
            let mut c = [
                lerp_u8(from[0], to[0], t),
                lerp_u8(from[1], to[1], t),
                lerp_u8(from[2], to[2], t),
            ];
            let d = crate::geom::distance_to_segment(fx, fy, 34.0, 94.0, 92.0, 34.0);
            let stroke_cov = (1.0 - (d - 9.0).max(0.0) / 1.5).clamp(0.0, 1.0);
            if stroke_cov > 0.0 {
                for channel in &mut c {
                    *channel = lerp_u8(*channel, 255, stroke_cov);
                }
            }
            let dn = ((fx - 92.0).powi(2) + (fy - 34.0).powi(2)).sqrt();
            let nib = (1.0 - (dn - 13.0).max(0.0) / 1.5).clamp(0.0, 1.0);
            if nib > 0.0 {
                for channel in &mut c {
                    *channel = lerp_u8(*channel, 255, nib);
                }
            }
            let i = (y * n + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], (cov * 255.0).round() as u8]);
        }
    }
    IconData {
        rgba,
        width: n as u32,
        height: n as u32,
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8
}

fn round_rect_coverage(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = px.clamp(x0 + r, x1 - r);
    let cy = py.clamp(y0 + r, y1 - r);
    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    (r + 0.5 - d).clamp(0.0, 1.0)
}

fn stroke(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

struct Ic<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    w: f32,
    c: Color32,
}

impl Ic<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.o.x + x * self.s, self.o.y + y * self.s)
    }
    fn line(&self, a: (f32, f32), b: (f32, f32)) {
        self.p.line_segment(
            [self.pt(a.0, a.1), self.pt(b.0, b.1)],
            stroke(self.w, self.c),
        );
    }
    fn poly(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|p| self.pt(p.0, p.1)).collect();
        self.p.add(Shape::line(v, stroke(self.w, self.c)));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|p| self.pt(p.0, p.1)).collect();
        self.p.add(Shape::closed_line(v, stroke(self.w, self.c)));
    }
    fn fill(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|p| self.pt(p.0, p.1)).collect();
        self.p.add(Shape::convex_polygon(v, self.c, Stroke::NONE));
    }
    fn circle(&self, c: (f32, f32), r: f32) {
        self.p
            .circle_stroke(self.pt(c.0, c.1), r * self.s, stroke(self.w, self.c));
    }
    fn dot(&self, c: (f32, f32), r: f32) {
        self.p.circle_filled(self.pt(c.0, c.1), r * self.s, self.c);
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.closed(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    }
    fn rrect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) {
        let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5);
        let arcs = [
            (x1 - r, y0 + r, -90.0f32, 0.0f32),
            (x1 - r, y1 - r, 0.0, 90.0),
            (x0 + r, y1 - r, 90.0, 180.0),
            (x0 + r, y0 + r, 180.0, 270.0),
        ];
        let mut pts: Vec<Pos2> = Vec::with_capacity(20);
        for (cx, cy, a0, a1) in arcs {
            for i in 0..=4 {
                let t = i as f32 / 4.0;
                let a = (a0 + (a1 - a0) * t).to_radians();
                pts.push(self.pt(cx + r * a.cos(), cy + r * a.sin()));
            }
        }
        self.p.add(Shape::closed_line(pts, stroke(self.w, self.c)));
    }
    fn ellipse(&self, c: (f32, f32), rx: f32, ry: f32) {
        let mut pts = Vec::with_capacity(33);
        for i in 0..=32 {
            let a = (i as f32 / 32.0) * std::f32::consts::TAU;
            pts.push(self.pt(c.0 + rx * a.cos(), c.1 + ry * a.sin()));
        }
        self.p.add(Shape::closed_line(pts, stroke(self.w, self.c)));
    }
    fn arc(&self, c: (f32, f32), r: f32, a0: f32, a1: f32) {
        let n = 20;
        let mut pts = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let a = (a0 + (a1 - a0) * t).to_radians();
            pts.push(self.pt(c.0 + r * a.cos(), c.1 + r * a.sin()));
        }
        self.p.add(Shape::line(pts, stroke(self.w, self.c)));
    }
    /// Small arrow head at `tip`, pointing along `dir` (radians).
    fn head(&self, tip: (f32, f32), dir: f32, size: f32) {
        for da in [2.5f32, -2.5f32] {
            let a = dir + da;
            self.line(tip, (tip.0 + size * a.cos(), tip.1 + size * a.sin()));
        }
    }
    fn dashed_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dash = 0.12;
        let gap = 0.09;
        let mut x = x0;
        while x < x1 {
            self.line((x, y0), ((x + dash).min(x1), y0));
            self.line((x, y1), ((x + dash).min(x1), y1));
            x += dash + gap;
        }
        let mut y = y0;
        while y < y1 {
            self.line((x0, y), (x0, (y + dash).min(y1)));
            self.line((x1, y), (x1, (y + dash).min(y1)));
            y += dash + gap;
        }
    }
}

pub(crate) fn draw(p: &Painter, icon: Icon, rect: Rect, color: Color32, width: f32) {
    let side = rect.width().min(rect.height());
    let s = side * 0.82;
    let o = Pos2::new(rect.center().x - s * 0.5, rect.center().y - s * 0.5);
    let ic = Ic {
        p,
        o,
        s,
        w: width,
        c: color,
    };
    use Icon::*;
    match icon {
        Pencil => {
            ic.closed(&[(0.26, 0.68), (0.66, 0.14), (0.86, 0.32), (0.34, 0.78)]);
            ic.fill(&[(0.26, 0.68), (0.34, 0.78), (0.12, 0.90)]);
            ic.line((0.26, 0.68), (0.34, 0.78));
        }
        Brush => {
            ic.line((0.84, 0.16), (0.50, 0.50));
            ic.closed(&[(0.46, 0.44), (0.58, 0.56), (0.30, 0.74)]);
            ic.fill(&[(0.30, 0.74), (0.22, 0.86), (0.34, 0.82)]);
            ic.line((0.44, 0.52), (0.52, 0.60));
        }
        Eraser => {
            ic.rrect(0.14, 0.44, 0.86, 0.76, 0.08);
            ic.line((0.38, 0.76), (0.60, 0.44));
            ic.line((0.10, 0.90), (0.90, 0.90));
        }
        Spray => {
            ic.rrect(0.28, 0.40, 0.56, 0.88, 0.07);
            ic.line((0.36, 0.40), (0.36, 0.28));
            ic.line((0.36, 0.28), (0.50, 0.28));
            for d in [
                (0.68f32, 0.26f32),
                (0.82, 0.36),
                (0.70, 0.50),
                (0.86, 0.58),
                (0.72, 0.68),
            ] {
                ic.dot(d, 0.045);
            }
        }
        Bucket => {
            ic.closed(&[(0.20, 0.54), (0.48, 0.22), (0.78, 0.52), (0.50, 0.84)]);
            ic.arc((0.49, 0.50), 0.20, 195.0, 345.0);
            ic.dot((0.86, 0.66), 0.08);
        }
        Dropper => {
            ic.line((0.24, 0.82), (0.58, 0.46));
            ic.circle((0.68, 0.34), 0.15);
            ic.fill(&[(0.62, 0.28), (0.76, 0.42), (0.58, 0.46), (0.56, 0.34)]);
            ic.dot((0.20, 0.86), 0.06);
        }
        Text => {
            ic.line((0.22, 0.24), (0.78, 0.24));
            ic.line((0.50, 0.24), (0.50, 0.80));
            ic.line((0.32, 0.80), (0.68, 0.80));
        }
        Line => {
            ic.line((0.20, 0.80), (0.80, 0.20));
            ic.dot((0.20, 0.80), 0.07);
            ic.dot((0.80, 0.20), 0.07);
        }
        Rect => ic.rrect(0.16, 0.26, 0.84, 0.74, 0.05),
        Ellipse => ic.ellipse((0.5, 0.5), 0.36, 0.26),
        Polygon => ic.closed(&[
            (0.50, 0.14),
            (0.88, 0.40),
            (0.74, 0.86),
            (0.26, 0.86),
            (0.12, 0.40),
        ]),
        Select => ic.dashed_rect(0.12, 0.20, 0.88, 0.80),
        Hand => {
            ic.rrect(0.30, 0.36, 0.74, 0.86, 0.14);
            ic.line((0.40, 0.36), (0.40, 0.18));
            ic.line((0.50, 0.34), (0.50, 0.12));
            ic.line((0.60, 0.36), (0.60, 0.20));
            ic.line((0.30, 0.56), (0.16, 0.48));
        }
        Undo => {
            ic.arc((0.50, 0.44), 0.30, 200.0, 350.0);
            ic.line((0.50, 0.74), (0.78, 0.74));
            ic.head((0.20, 0.44), 200.0f32.to_radians(), 0.22);
            ic.line((0.50 - 0.30 * 0.94, 0.44 - 0.30 * 0.34), (0.20, 0.44));
        }
        Redo => {
            ic.arc((0.50, 0.44), 0.30, 340.0, 190.0);
            ic.line((0.22, 0.74), (0.50, 0.74));
            ic.head((0.80, 0.44), 340.0f32.to_radians(), 0.22);
            ic.line((0.80, 0.44), (0.50 + 0.30 * 0.94, 0.44 - 0.30 * 0.34));
        }
        New => {
            ic.rrect(0.22, 0.12, 0.64, 0.88, 0.07);
            ic.line((0.33, 0.36), (0.53, 0.36));
            ic.line((0.33, 0.52), (0.53, 0.52));
            ic.line((0.74, 0.58), (0.74, 0.86));
            ic.line((0.60, 0.72), (0.88, 0.72));
        }
        Open => {
            ic.closed(&[
                (0.10, 0.78),
                (0.10, 0.26),
                (0.40, 0.26),
                (0.48, 0.38),
                (0.90, 0.38),
                (0.90, 0.78),
            ]);
            ic.line((0.10, 0.46), (0.90, 0.46));
        }
        Save => {
            ic.poly(&[(0.18, 0.60), (0.18, 0.84), (0.82, 0.84), (0.82, 0.60)]);
            ic.line((0.50, 0.16), (0.50, 0.62));
            ic.poly(&[(0.36, 0.48), (0.50, 0.62), (0.64, 0.48)]);
        }
        Upload => {
            ic.poly(&[(0.18, 0.60), (0.18, 0.84), (0.82, 0.84), (0.82, 0.60)]);
            ic.line((0.50, 0.62), (0.50, 0.16));
            ic.poly(&[(0.36, 0.30), (0.50, 0.16), (0.64, 0.30)]);
        }
        Plus => {
            ic.line((0.50, 0.20), (0.50, 0.80));
            ic.line((0.20, 0.50), (0.80, 0.50));
        }
        Minus => ic.line((0.20, 0.50), (0.80, 0.50)),
        Grid => {
            ic.rect(0.14, 0.14, 0.86, 0.86);
            ic.line((0.38, 0.14), (0.38, 0.86));
            ic.line((0.62, 0.14), (0.62, 0.86));
            ic.line((0.14, 0.38), (0.86, 0.38));
            ic.line((0.14, 0.62), (0.86, 0.62));
        }
        Fit => {
            for (x, y, dx, dy) in [
                (0.14f32, 0.14f32, 1.0f32, 1.0f32),
                (0.86, 0.14, -1.0, 1.0),
                (0.14, 0.86, 1.0, -1.0),
                (0.86, 0.86, -1.0, -1.0),
            ] {
                ic.line((x, y), (x + dx * 0.24, y));
                ic.line((x, y), (x, y + dy * 0.24));
            }
        }
        Sun => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                ic.line(
                    (0.5 + 0.24 * a.cos(), 0.5 + 0.24 * a.sin()),
                    (0.5 + 0.36 * a.cos(), 0.5 + 0.36 * a.sin()),
                );
            }
            ic.circle((0.5, 0.5), 0.15);
        }
        Moon => {
            ic.arc((0.48, 0.5), 0.34, -60.0, 240.0);
            ic.arc((0.70, 0.5), 0.30, 240.0, -60.0);
        }
        Close => {
            ic.line((0.24, 0.24), (0.76, 0.76));
            ic.line((0.76, 0.24), (0.24, 0.76));
        }
        ChevronDown => ic.poly(&[(0.28, 0.42), (0.50, 0.62), (0.72, 0.42)]),
        Swap => {
            ic.line((0.20, 0.36), (0.80, 0.36));
            ic.head((0.80, 0.36), 0.0, 0.18);
            ic.line((0.80, 0.68), (0.20, 0.68));
            ic.head((0.20, 0.68), std::f32::consts::PI, 0.18);
        }
        Trash => {
            ic.line((0.16, 0.28), (0.84, 0.28));
            ic.poly(&[(0.38, 0.28), (0.38, 0.18), (0.62, 0.18), (0.62, 0.28)]);
            ic.poly(&[(0.26, 0.28), (0.32, 0.86), (0.68, 0.86), (0.74, 0.28)]);
            ic.line((0.44, 0.40), (0.44, 0.74));
            ic.line((0.56, 0.40), (0.56, 0.74));
        }
        Copy => {
            ic.rrect(0.14, 0.14, 0.60, 0.60, 0.07);
            ic.rrect(0.40, 0.40, 0.86, 0.86, 0.07);
        }
        Cut => {
            ic.line((0.31, 0.20), (0.66, 0.68));
            ic.line((0.69, 0.20), (0.34, 0.68));
            ic.circle((0.26, 0.80), 0.11);
            ic.circle((0.74, 0.80), 0.11);
        }
        Paste => {
            ic.rrect(0.22, 0.22, 0.78, 0.88, 0.07);
            ic.rect(0.38, 0.12, 0.62, 0.28);
            ic.line((0.34, 0.50), (0.66, 0.50));
            ic.line((0.34, 0.66), (0.66, 0.66));
        }
        FlipH => {
            ic.line((0.50, 0.10), (0.50, 0.90));
            ic.fill(&[(0.10, 0.50), (0.40, 0.28), (0.40, 0.72)]);
            ic.fill(&[(0.90, 0.50), (0.60, 0.28), (0.60, 0.72)]);
        }
        FlipV => {
            ic.line((0.10, 0.50), (0.90, 0.50));
            ic.fill(&[(0.50, 0.10), (0.28, 0.40), (0.72, 0.40)]);
            ic.fill(&[(0.50, 0.90), (0.28, 0.60), (0.72, 0.60)]);
        }
        RotateCw => {
            ic.arc((0.5, 0.5), 0.30, 40.0, 330.0);
            ic.head(
                (
                    0.5 + 0.30 * 40.0f32.to_radians().cos(),
                    0.5 + 0.30 * 40.0f32.to_radians().sin(),
                ),
                -0.7,
                0.20,
            );
        }
        Invert => {
            ic.circle((0.5, 0.5), 0.34);
            let mut pts = Vec::with_capacity(26);
            pts.push((0.5f32, 0.5f32));
            for i in 0..=24 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 24.0;
                pts.push((0.5 + 0.34 * a.cos(), 0.5 + 0.34 * a.sin()));
            }
            ic.fill(&pts);
        }
        Crop => {
            ic.poly(&[(0.28, 0.06), (0.28, 0.72), (0.94, 0.72)]);
            ic.poly(&[(0.06, 0.28), (0.72, 0.28), (0.72, 0.94)]);
        }
        Resize => {
            ic.line((0.22, 0.78), (0.78, 0.22));
            ic.head((0.78, 0.22), -0.79, 0.20);
            ic.head((0.22, 0.78), 2.36, 0.20);
        }
        Info => {
            ic.circle((0.5, 0.5), 0.36);
            ic.dot((0.5, 0.30), 0.055);
            ic.line((0.5, 0.44), (0.5, 0.70));
        }
        Check => ic.poly(&[(0.20, 0.52), (0.42, 0.74), (0.80, 0.26)]),
        ZoomIn => {
            ic.circle((0.44, 0.44), 0.28);
            ic.line((0.64, 0.64), (0.88, 0.88));
            ic.line((0.44, 0.31), (0.44, 0.57));
            ic.line((0.31, 0.44), (0.57, 0.44));
        }
        ZoomOut => {
            ic.circle((0.44, 0.44), 0.28);
            ic.line((0.64, 0.64), (0.88, 0.88));
            ic.line((0.31, 0.44), (0.57, 0.44));
        }
        Palette => {
            ic.circle((0.5, 0.5), 0.36);
            ic.dot((0.36, 0.42), 0.065);
            ic.dot((0.54, 0.32), 0.065);
            ic.dot((0.66, 0.50), 0.065);
            ic.dot((0.44, 0.62), 0.065);
        }
        Sparkle => {
            ic.fill(&[
                (0.50, 0.06),
                (0.62, 0.38),
                (0.94, 0.50),
                (0.62, 0.62),
                (0.50, 0.94),
                (0.38, 0.62),
                (0.06, 0.50),
                (0.38, 0.38),
            ]);
        }
        File => {
            ic.rrect(0.24, 0.10, 0.76, 0.90, 0.07);
            ic.line((0.36, 0.36), (0.64, 0.36));
            ic.line((0.36, 0.52), (0.64, 0.52));
            ic.line((0.36, 0.68), (0.56, 0.68));
        }
        Folder => {
            ic.closed(&[
                (0.10, 0.78),
                (0.10, 0.26),
                (0.40, 0.26),
                (0.48, 0.38),
                (0.90, 0.38),
                (0.90, 0.78),
            ]);
        }
    }
}

/// Marching-ants rectangle used for selections, clipped to the painter's
/// viewport so high zoom factors only emit visible dashes.
pub(crate) fn marching_ants(p: &Painter, rect: Rect, offset: f32, dark: bool) {
    let c1 = if dark {
        Color32::from_rgb(0x18, 0x18, 0x18)
    } else {
        Color32::BLACK
    };
    let c2 = Color32::WHITE;
    let dash = 4.0f32;
    let period = dash * 2.0;
    let phase = offset.rem_euclid(period);
    let clip = p.clip_rect().expand(2.0);
    if !clip.intersects(rect.expand(2.0)) {
        return;
    }

    // Horizontal edges: top (left->right) and bottom (right->left).
    for (y, x_start, x_end, dir_x) in [
        (rect.top(), rect.left(), rect.right(), 1.0f32),
        (rect.bottom(), rect.right(), rect.left(), -1.0f32),
    ] {
        if y < clip.top() || y > clip.bottom() {
            continue;
        }
        let len = (x_end - x_start).abs();
        if len <= 0.01 {
            continue;
        }
        let (vis0, vis1) = if dir_x > 0.0 {
            (
                (clip.left() - x_start).clamp(0.0, len),
                (clip.right() - x_start).clamp(0.0, len),
            )
        } else {
            (
                (x_start - clip.right()).clamp(0.0, len),
                (x_start - clip.left()).clamp(0.0, len),
            )
        };
        draw_dashed_span(p, vis0, vis1, dash, phase, c1, c2, |t| {
            Pos2::new(x_start + dir_x * t, y)
        });
    }

    // Vertical edges: right (top->bottom) and left (bottom->top).
    for (x, y_start, y_end, dir_y) in [
        (rect.right(), rect.top(), rect.bottom(), 1.0f32),
        (rect.left(), rect.bottom(), rect.top(), -1.0f32),
    ] {
        if x < clip.left() || x > clip.right() {
            continue;
        }
        let len = (y_end - y_start).abs();
        if len <= 0.01 {
            continue;
        }
        let (vis0, vis1) = if dir_y > 0.0 {
            (
                (clip.top() - y_start).clamp(0.0, len),
                (clip.bottom() - y_start).clamp(0.0, len),
            )
        } else {
            (
                (y_start - clip.bottom()).clamp(0.0, len),
                (y_start - clip.top()).clamp(0.0, len),
            )
        };
        draw_dashed_span(p, vis0, vis1, dash, phase, c1, c2, |t| {
            Pos2::new(x, y_start + dir_y * t)
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_dashed_span(
    p: &Painter,
    vis0: f32,
    vis1: f32,
    dash: f32,
    phase: f32,
    c1: Color32,
    c2: Color32,
    point_at: impl Fn(f32) -> Pos2,
) {
    if vis1 <= vis0 {
        return;
    }
    let first_k = ((vis0 + phase) / dash).floor().max(0.0) as i64;
    let mut k = first_k;
    let mut t = k as f32 * dash - phase;
    while t < vis1 {
        let s = t.max(vis0);
        let e = (t + dash).min(vis1);
        if e > s {
            let c = if k % 2 == 0 { c1 } else { c2 };
            p.line_segment([point_at(s), point_at(e)], Stroke::new(1.0_f32, c));
        }
        t += dash;
        k += 1;
    }
}
