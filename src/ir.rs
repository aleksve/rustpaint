//! Integer rectangle helpers used for canvas <-> pixel-space bookkeeping.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct IRect {
    /// Inclusive-exclusive: x0 <= x < x1
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    #[inline]
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    #[inline]
    pub fn px(x: i32, y: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x.saturating_add(1),
            y1: y.saturating_add(1),
        }
    }

    #[inline]
    pub fn from_min_size(x: i32, y: i32, w: i32, h: i32) -> Self {
        let w = w.max(0);
        let h = h.max(0);
        Self {
            x0: x,
            y0: y,
            x1: x.saturating_add(w),
            y1: y.saturating_add(h),
        }
    }

    #[inline]
    pub fn w(&self) -> i32 {
        if self.x1 <= self.x0 {
            0
        } else {
            self.x1.saturating_sub(self.x0)
        }
    }

    #[inline]
    pub fn h(&self) -> i32 {
        if self.y1 <= self.y0 {
            0
        } else {
            self.y1.saturating_sub(self.y0)
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    #[inline]
    pub fn contains_rect(&self, o: IRect) -> bool {
        if o.is_empty() {
            return true;
        }
        if self.is_empty() {
            return false;
        }
        self.x0 <= o.x0 && self.y0 <= o.y0 && self.x1 >= o.x1 && self.y1 >= o.y1
    }

    #[inline]
    pub fn expand(&self, pad: i32) -> IRect {
        if self.is_empty() {
            return *self;
        }
        IRect {
            x0: self.x0.saturating_sub(pad),
            y0: self.y0.saturating_sub(pad),
            x1: self.x1.saturating_add(pad),
            y1: self.y1.saturating_add(pad),
        }
    }

    #[inline]
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

    #[inline]
    pub fn union_px(&self, x: i32, y: i32) -> IRect {
        if self.is_empty() {
            return IRect::px(x, y);
        }
        IRect {
            x0: self.x0.min(x),
            y0: self.y0.min(y),
            x1: self.x1.max(x.saturating_add(1)),
            y1: self.y1.max(y.saturating_add(1)),
        }
    }

    #[inline]
    pub fn intersect(&self, o: IRect) -> IRect {
        IRect {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }

    #[inline]
    pub fn clamp(&self, w: i32, h: i32) -> IRect {
        self.intersect(IRect {
            x0: 0,
            y0: 0,
            x1: w.max(0),
            y1: h.max(0),
        })
    }

    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
    }
}

#[cfg(test)]
mod tests {
    use super::IRect;

    #[test]
    fn saturates_on_extreme_coordinates() {
        let r = IRect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX);
        assert_eq!(r.w(), i32::MAX);
        assert_eq!(r.h(), i32::MAX);
        assert!(!r.is_empty());

        let p = IRect::px(i32::MAX, i32::MAX);
        assert!(p.is_empty());

        let sized = IRect::from_min_size(i32::MAX - 2, -10, 100, 20);
        assert_eq!(sized.x1, i32::MAX);
        assert_eq!(sized.y1, 10);
    }
}
