//! Conservative drawing-context proof for source outline rectangles.
//! This observes private state and never changes emitted Glyphs or Paths.
use super::*;
#[derive(Clone,Copy)]
enum SourceOutline{InkRectangle,SceneRectangle,CompoundEnvelope}
impl Interp<'_> {
    pub(super) fn observe_ink_op(&mut self, op: &[u8], o: &[Object], resources: &Dict) {
        if self.source_rects.is_none() {
            return;
        }
        let numbers =
            |n: usize| o.len() == n && o.iter().all(|v| v.as_f64().is_some_and(f64::is_finite));
        let colors = |n: usize| {
            numbers(n)
                && o.iter()
                    .all(|v| v.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x)))
        };
        let name = || o.len() == 1 && o[0].as_name().is_some();
        match op {
            b"g" | b"rg" | b"k" => {
                let n = if op == b"g" {
                    1
                } else if op == b"rg" {
                    3
                } else {
                    4
                };
                self.gs.ink_fill_components = colors(n).then_some(n);
                return;
            }
            b"cs" => {
                self.gs.ink_fill_components = if name() {
                    match o[0].as_name() {
                        Some(b"DeviceGray") => Some(1),
                        Some(b"DeviceRGB") => Some(3),
                        Some(b"DeviceCMYK") => Some(4),
                        _ => None,
                    }
                } else {
                    None
                };
                return;
            }
            b"sc" | b"scn" => {
                if !self.gs.ink_fill_components.is_some_and(colors) {
                    self.gs.ink_fill_components = None;
                }
                return;
            }
            _ => {}
        }
        let valid = match op {
            b"q" | b"BT" | b"ET" | b"T*" | b"h" | b"S" | b"s" | b"f" | b"F" | b"f*" | b"B"
            | b"B*" | b"b" | b"b*" | b"W" | b"W*" | b"n" | b"BX" | b"EX" => o.is_empty(),
            b"Q" => o.is_empty() && !self.stack.is_empty(),
            b"cm" | b"Tm" | b"c" => numbers(6),
            b"Td" | b"TD" | b"m" | b"l" => numbers(2),
            b"v" | b"y" | b"re" => numbers(4),
            b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Ts" | b"w" | b"M" | b"i" => numbers(1),
            b"J" | b"j" => o.len() == 1 && o[0].as_i64().is_some_and(|x| (0..=2).contains(&x)),
            b"Tr" => {
                let mode = (o.len() == 1).then(|| o[0].as_i64()).flatten();
                if mode.is_some_and(|x| x >= 4) {
                    self.gs.ink_geometry_complete = false;
                }
                mode.is_some_and(|x| (0..=7).contains(&x))
            }
            b"Tf" => {
                o.len() == 2
                    && o[0].as_name().is_some()
                    && o[1].as_f64().is_some_and(f64::is_finite)
            }
            b"Tj" | b"'" => o.len() == 1 && o[0].as_str().is_some(),
            b"\"" => {
                o.len() == 3
                    && o[..2]
                        .iter()
                        .all(|v| v.as_f64().is_some_and(f64::is_finite))
                    && o[2].as_str().is_some()
            }
            b"TJ" => {
                o.len() == 1
                    && o[0].as_array().is_some_and(|a| {
                        a.iter()
                            .all(|v| v.as_str().is_some() || v.as_f64().is_some_and(f64::is_finite))
                    })
            }
            b"gs" => name() && self.ink_graphics_state(resources, o[0].as_name().unwrap()),
            b"G" => colors(1),
            b"RG" => colors(3),
            b"K" => colors(4),
            b"CS" | b"ri" | b"Do" | b"sh" | b"MP" | b"BMC" => name(),
            b"SC" | b"SCN" => o
                .iter()
                .all(|v| v.as_name().is_some() || v.as_f64().is_some_and(f64::is_finite)),
            b"d" => {
                o.len() == 2
                    && o[0]
                        .as_array()
                        .is_some_and(|a| a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)))
                    && o[1].as_f64().is_some_and(f64::is_finite)
            }
            b"DP" | b"BDC" => o.len() == 2 && o[0].as_name().is_some(),
            b"EMC" => o.is_empty(),
            _ => false,
        };
        if !valid {
            self.gs.ink_geometry_complete = false;
        }
    }
    fn ink_graphics_state(&self, resources: &Dict, name: &[u8]) -> bool {
        let Some(states) = self.doc.dict_get(resources, b"ExtGState") else {
            return false;
        };
        let Some(state) = states.as_dict().and_then(|d| self.doc.dict_get(d, name)) else {
            return false;
        };
        let Some(dict) = state.as_dict() else {
            return false;
        };
        dict.keys().all(|key| {
            let Some(v) = self.doc.dict_get(dict, key) else {
                return false;
            };
            match key.as_slice() {
                b"Type" => v.as_name() == Some(b"ExtGState"),
                // Stroke adjustment and curve smoothing do not change a filled polygon.
                b"SA" => matches!(v, Object::Bool(_)),
                b"SM" | b"CA" => v
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
                b"ca" => v.as_f64() == Some(1.),
                b"SMask" => v.as_name() == Some(b"None"),
                b"BM" => matches!(v.as_name(), Some(b"Normal" | b"Compatible")),
                b"AIS" | b"OP" | b"op" => matches!(v, Object::Bool(false)),
                b"OPM" => v.as_i64().is_some_and(|n| n == 0 || n == 1),
                // Uninterpreted masks, fonts, transfer functions, and blend states
                // cannot silently obtain ordinary opaque fill semantics.
                _ => false,
            }
        })
    }
    pub(super) fn capture_ink_rectangle(
        &self,
        font: &Font,
        c: &crate::font::CharCode,
        trm: &Matrix,
    ) -> Option<[f64; 4]> {
        self.capture_outline_geometry(font,c,trm,SourceOutline::InkRectangle)
    }
    pub(super) fn capture_scene_rectangle(&self,font:&Font,c:&crate::font::CharCode,trm:&Matrix)->Option<[f64;4]>{
        self.capture_outline_geometry(font,c,trm,SourceOutline::SceneRectangle)
    }
    pub(super) fn capture_scene_compound_envelope(&self,font:&Font,c:&crate::font::CharCode,trm:&Matrix)->Option<[f64;4]>{
        self.capture_outline_geometry(font,c,trm,SourceOutline::CompoundEnvelope)
    }
    fn capture_outline_geometry(&self,font:&Font,c:&crate::font::CharCode,trm:&Matrix,kind:SourceOutline)->Option<[f64;4]>{
        if !self.gs.ink_geometry_complete
            || !self.gs.source_geometry_complete
            || self.gs.render != 0
            || self.gs.ink_fill_components.is_none()
        {
            return None;
        }
        let [x0,y0,x1,y1]=match kind{
            SourceOutline::InkRectangle=>font.source_rectangle(self.doc,c)?,
            SourceOutline::SceneRectangle=>font.source_scene_rectangle(self.doc,c)?,
            SourceOutline::CompoundEnvelope=>font.source_scene_compound_envelope(self.doc,c)?,
        };
        // PDF origin1 = origin0 + v; embedded outlines use origin0.
        let (vx,vy)=if !matches!(kind,SourceOutline::InkRectangle)&&font.vertical{let(_,vx,vy)=font.vertical_metrics(c);(vx,vy)}else{(0.,0.)};
        if !vx.is_finite()||!vy.is_finite(){return None;}
        let points = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
            let (x, y) = trm.apply(x / 1000.-vx, y / 1000.-vy);
            [x, y]
        });
        let b = crate::cff_aj1::rectangles::filled_rectangle(&points)?;
        let contains = |a: [f64; 4]| {
            a.iter().all(|x| x.is_finite())
                && a[0] < a[2]
                && a[1] < a[3]
                && a[0] <= b[0]
                && a[1] <= b[1]
                && b[2] <= a[2]
                && b[3] <= a[3]
        };
        if !contains(self.ink_viewport) {
            return None;
        }
        for clip in self.gs.source_clips.as_ref()? {
            if !clip.geometry_complete {
                return None;
            }
            let rect = if let Some(q) = clip.page_quad {
                crate::cff_aj1::rectangles::filled_rectangle(&q)?
            } else {
                if clip.subpaths.len() != 1 || clip.subpaths[0].curved {
                    return None;
                }
                crate::cff_aj1::rectangles::filled_rectangle(&clip.subpaths[0].points)?
            };
            // A partially clipped rectangle is not a certificate for the full glyph.
            if !contains(rect) {
                return None;
            }
        }
        Some(b)
    }
}
