//! Flattened glyph outlines from embedded font programs.

/// Collects a glyph outline as polygons, transformed by a font matrix.
pub(crate) struct Flatten {
    m: [f64; 6],
    contours: Vec<Vec<[f64; 2]>>,
    current: Vec<[f64; 2]>,
    last: [f64; 2],
}

/// Segments per curve. Outlines are only compared after normalising to a
/// small bitmap, so a fixed subdivision is ample.
const STEPS: usize = 8;

impl Flatten {
    pub(crate) fn new(m: [f64; 6]) -> Self {
        Self {
            m,
            contours: Vec::new(),
            current: Vec::new(),
            last: [0., 0.],
        }
    }
    fn map(&self, x: f32, y: f32) -> [f64; 2] {
        let (x, y) = (f64::from(x), f64::from(y));
        [
            self.m[0] * x + self.m[2] * y + self.m[4],
            self.m[1] * x + self.m[3] * y + self.m[5],
        ]
    }
    fn push(&mut self, p: [f64; 2]) {
        self.current.push(p);
        self.last = p;
    }
    pub(crate) fn finish(mut self) -> Vec<Vec<[f64; 2]>> {
        ttf_parser::OutlineBuilder::close(&mut self);
        self.contours
    }
}

impl ttf_parser::OutlineBuilder for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = self.map(x, y);
        self.push(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.push(p);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.last, self.map(x1, y1), self.map(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1. - t;
            self.push([
                u * u * p0[0] + 2. * u * t * p1[0] + t * t * p2[0],
                u * u * p0[1] + 2. * u * t * p1[1] + t * t * p2[1],
            ]);
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (
            self.last,
            self.map(x1, y1),
            self.map(x2, y2),
            self.map(x, y),
        );
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1. - t;
            let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
            self.push([
                a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
            ]);
        }
    }
    fn close(&mut self) {
        if self.current.len() > 2 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
}
