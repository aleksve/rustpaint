//! Integer rectangle helpers used for canvas <-> pixel-space bookkeeping.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IRect {
    /// Inclusive-exclusive: x0 <= x < x1
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    pub fn px(x: i32, y: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + 1,
            y1: y + 1,
        }
    }

    pub fn from_min_size(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        }
    }

    pub fn w(&self) -> i32 {
        (self.x1 - self.x0).max(0)
    }

    pub fn h(&self) -> i32 {
        (self.y1 - self.y0).max(0)
    }

    pub fn is_empty(&self) -> bool {
        self.w() <= 0 || self.h() <= 0
    }

    pub fn union(&self, o: IRect) -> IRect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return *self;
        }
        IRect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    pub fn union_px(&self, x: i32, y: i32) -> IRect {
        if self.is_empty() {
            return IRect::px(x, y);
        }
        IRect {
            x0: self.x0.min(x),
            y0: self.y0.min(y),
            x1: self.x1.max(x + 1),
            y1: self.y1.max(y + 1),
        }
    }

    pub fn intersect(&self, o: IRect) -> IRect {
        IRect {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }

    pub fn clamp(&self, w: i32, h: i32) -> IRect {
        self.intersect(IRect {
            x0: 0,
            y0: 0,
            x1: w,
            y1: h,
        })
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
    }
}
