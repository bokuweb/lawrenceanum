//! ページのコンテンツストリームを解釈し、文字（位置・大きさ・Unicode）と描かれたパスを集める
//! （ISO 32000-1 8・9）。
//!
//! 座標はすべて「ページ座標」: 表示の向き（/Rotate を適用した後）で左上が原点、y は下向き、
//! 単位は pt。CropBox を基準にする。

use std::collections::HashMap;
use std::rc::Rc;

use serde::Serialize;

use crate::document::{Document, PageRef};
use crate::font::Font;
use crate::lexer::Parser;
use crate::object::{Dict, ObjRef, Object};

/// 1 文字。
#[derive(Debug, Clone, Serialize)]
pub struct Glyph {
    /// Unicode（対応が分からなければ空）。
    pub text: String,
    /// 文字コードと CID（単純フォントでは CID = 文字コード）。
    pub code: u32,
    pub cid: u32,
    /// [`Page::fonts`] の添字。
    pub font: usize,
    /// 文字の原点（横書きは基線の左端、縦書きは字の上端の中央）。
    pub x: f64,
    pub y: f64,
    /// 字そのものの送りの終点（字間 Tc・語間 Tw を含まない。次の文字が詰めて置かれたときの原点）。
    pub end_x: f64,
    pub end_y: f64,
    /// 字の大きさ（pt）。
    pub size: f64,
    /// 縦書き（WMode 1）のフォントで組まれた文字か。
    pub vertical: bool,
    /// 字面の枠（em の正方形、横書きは Ascent〜Descent）`[x0, y0, x1, y1]`。
    pub bbox: [f64; 4],
    /// 描画されない文字（文字描画モード 3・7）。
    pub invisible: bool,
    /// この字の後の字間・語間（Tc・Tw）による送り（pt、字送りの向きに正）。
    pub spacing: f64,
    /// この字の前の TJ の数値による送り（pt、字送りの向きに正）。
    pub kern: f64,
    /// 文字列を描く演算子（Tj・TJ など）の通し番号。同じ演算子で描かれた字は同じ番号。
    pub show: u32,
}

/// 描かれた（塗り・線を引いた）パス 1 つ。
#[derive(Debug, Clone, Serialize)]
pub struct PathItem {
    pub subpaths: Vec<SubPath>,
    pub stroke: bool,
    pub fill: bool,
    /// 線幅（ページ座標の pt、CTM の拡大率を掛けた値）。
    pub line_width: f64,
    /// 描いたときのクリップの外接矩形（ページ座標の `[x0, y0, x1, y1]`）。クリップが無ければ `None`。
    /// クリップのパスの外接矩形を重ねた範囲なので、実際に見える範囲より狭くはならない。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<[f64; 4]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubPath {
    /// 頂点（曲線は終点だけ）。
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    /// 曲線を含むか。
    pub curved: bool,
    /// `re` で描いた矩形か。
    pub rect: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FontInfo {
    pub name: String,
    pub vertical: bool,
    /// CIDSystemInfo の Ordering（`Japan1`・`Identity` など）。
    pub ordering: Option<String>,
    pub has_to_unicode: bool,
}

/// 1 ページの中身。
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    /// 表示の向きでの幅・高さ（pt）。
    pub width: f64,
    pub height: f64,
    /// /Rotate（0・90・180・270）。
    pub rotate: i32,
    pub glyphs: Vec<Glyph>,
    pub paths: Vec<PathItem>,
    pub fonts: Vec<FontInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn from_operands(o: &[Object]) -> Option<Matrix> {
        if o.len() < 6 {
            return None;
        }
        let v: Vec<f64> = o[o.len() - 6..].iter().filter_map(Object::as_f64).collect();
        (v.len() == 6).then(|| Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }

    /// `self` を先に、`other` を後に掛ける。
    fn then(&self, other: &Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = other.0;
        Matrix([
            a * a2 + b * c2,
            a * b2 + b * d2,
            c * a2 + d * c2,
            c * b2 + d * d2,
            e * a2 + f * c2 + e2,
            e * b2 + f * d2 + f2,
        ])
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    fn translate(tx: f64, ty: f64) -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, tx, ty])
    }

    /// 線幅などの長さの拡大率（縦横の平均）。
    fn scale(&self) -> f64 {
        let [a, b, c, d, _, _] = self.0;
        ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0
    }
}

#[derive(Clone)]
struct GState {
    ctm: Matrix,
    font: Option<usize>,
    font_size: f64,
    char_spacing: f64,
    word_spacing: f64,
    h_scale: f64,
    leading: f64,
    rise: f64,
    render: i64,
    line_width: f64,
    clip: Option<[f64; 4]>,
    source_clips:Option<Vec<crate::source_images::SourceClip>>,
    source_geometry_complete:bool,
    ink_geometry_complete:bool,
    ink_fill_components:Option<usize>,
    source_graphics_states:Option<Vec<Vec<u8>>>,
}

const MAX_FORM_DEPTH: usize = 12;

struct Interp<'a> {
    doc: &'a Document,
    device: Matrix,
    gs: GState,
    stack: Vec<GState>,
    tm: Matrix,
    tlm: Matrix,
    fonts: Vec<Rc<Font>>,
    font_ids: HashMap<ObjRef, usize>,
    glyphs: Vec<Glyph>,
    /// Source candidates stay adjacent to the original emitted CharCode.
    /// Legacy extraction does not allocate or decode source evidence.
    source_resolved: Option<Vec<Option<char>>>,
    source_rects:Option<Vec<Option<[f64;4]>>>,
    ink_viewport:[f64;4],
    image_draws: Option<Vec<crate::source_images::ImageDraw>>,
    image_gaps:Option<Vec<crate::source_images::CaptureGap>>,
    source_pending_clip:Option<crate::source_images::ClipRule>,
    paths: Vec<PathItem>,
    path: Vec<SubPath>,
    current: Option<SubPath>,
    forms: Vec<ObjRef>,
    /// 次の字の前の TJ の数値による送り（text space の字の大きさ倍）。
    pending_kern: f64,
    show_count: u32,
    /// `W`・`W*` の後、次にパスを描く（`n` を含む）ときにクリップを狭める。
    pending_clip: bool,
}

/// ページを解釈する。
pub fn extract_page(doc: &Document, page: &PageRef) -> Page {
    interpret_page(doc, page, false, false).0
}
pub(crate) fn extract_page_source_aj1(doc: &Document, page: &PageRef) -> crate::Aj1PageSources {
    let (page, resolved, ink_rects, _, _) = interpret_page(doc, page, true, false);
    crate::source_aj1::Aj1PageSources { original: page, resolved, ink_rects }
}

pub(crate) fn extract_page_source_images(doc:&Document,page:&PageRef)->crate::source_images::XObjectPageSources{
 let(original,resolved,ink_rects,draws,gaps)=interpret_page(doc,page,true,true);
 crate::source_images::XObjectPageSources{native:crate::source_aj1::Aj1PageSources{original,resolved,ink_rects},crop:page.crop_box,draws,gaps,document_digest:[0;32],media:page.media_box}
}
fn interpret_page(doc: &Document, page: &PageRef, source: bool, image_sources:bool) -> (Page, Vec<Option<char>>,Vec<Option<[f64;4]>>,Vec<crate::source_images::ImageDraw>,Vec<crate::source_images::CaptureGap>) {
    let [x0, y0, x1, y1] = page.crop_box;
    let device = match page.rotate {
        90 => Matrix([0.0, 1.0, 1.0, 0.0, -y0, -x0]),
        180 => Matrix([-1.0, 0.0, 0.0, 1.0, x1, -y0]),
        270 => Matrix([0.0, -1.0, -1.0, 0.0, y1, x1]),
        _ => Matrix([1.0, 0.0, 0.0, -1.0, -x0, y1]),
    };
    let (w, h) = (x1 - x0, y1 - y0);
    let (width, height) = if page.rotate % 180 == 90 { (h, w) } else { (w, h) };
    let mut it = Interp {
        doc,
        device,
        gs: GState {
            ctm: Matrix::IDENTITY,
            font: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            render: 0,
            line_width: 1.0,
            clip: None,
            source_clips:source.then(Vec::new),
            source_geometry_complete:true,
            ink_geometry_complete:true,
            ink_fill_components:Some(1),
            source_graphics_states:image_sources.then(Vec::new),
        },
        stack: Vec::new(),
        tm: Matrix::IDENTITY,
        tlm: Matrix::IDENTITY,
        fonts: Vec::new(),
        font_ids: HashMap::new(),
        glyphs: Vec::new(),
        source_resolved: source.then(Vec::new),
        source_rects:source.then(Vec::new),
        ink_viewport:[0.,0.,width,height],
        image_draws: image_sources.then(Vec::new),
        image_gaps:image_sources.then(Vec::new),
        source_pending_clip:None,
        paths: Vec::new(),
        path: Vec::new(),
        current: None,
        forms: Vec::new(),
        pending_kern: 0.0,
        show_count: 0,
        pending_clip: false,
    };
    let mut ink_gaps=Vec::new();
    let content = page_content(doc, &page.dict,if image_sources{it.image_gaps.as_mut()}else if source{Some(&mut ink_gaps)}else{None});
    if source&&(!ink_gaps.is_empty()||it.image_gaps.as_ref().is_some_and(|g|!g.is_empty())||!width.is_finite()||!height.is_finite()||width<=0.||height<=0.){it.gs.ink_geometry_complete=false;}

    if image_sources && (!(width.is_finite()&&height.is_finite()&&width>0.&&height>0.)||!device.0.iter().all(|v|v.is_finite())){it.gs.source_geometry_complete=false;it.image_gap(crate::source_images::GapKind::InvalidPageGeometry,b"");}
    it.run(&content, &page.resources);
    let resolved = it.source_resolved.take().unwrap_or_default();
    let ink_rects=it.source_rects.take().unwrap_or_default();
    let image_draws=it.image_draws.take().unwrap_or_default();
    let image_gaps=it.image_gaps.take().unwrap_or_default();
    (
        Page {
        width,
        height,
        rotate: page.rotate,
        glyphs: it.glyphs,
        paths: it.paths,
        fonts: it
            .fonts
            .iter()
            .map(|f| FontInfo {
                name: f.base_font.clone(),
                vertical: f.vertical,
                ordering: f.ordering.clone(),
                has_to_unicode: f.has_to_unicode,
            })
            .collect(),
    },
        resolved,
        ink_rects,
        image_draws,
        image_gaps,
    )
}

fn page_content(doc:&Document,page:&Dict,mut gaps:Option<&mut Vec<crate::source_images::CaptureGap>>)->Vec<u8>{
 let mut out=Vec::new();
 let raw=page.get(&b"Contents"[..]);
 let streams=match raw.map(|c|doc.resolve(c)){
  Some(Object::Array(a))=>a.iter().map(|x|doc.resolve(x)).collect(),
  Some(Object::Null)if matches!(raw,Some(Object::Null))=>Vec::new(),
  Some(o)=>vec![o],None=>Vec::new(),
 };
 for object in streams{
  let data=object.as_stream().map(|stream|doc.decode_stream(stream));
  if let Some(Ok(bytes))=data{out.extend_from_slice(&bytes);out.push(b'\n');}
  else if let Some(list)=gaps.as_deref_mut(){list.push(crate::source_images::CaptureGap{kind:crate::source_images::GapKind::PageContentFailure,resource_name:vec![],form_path:vec![]});}
 }
 out
}

impl Interp<'_> {
    fn run(&mut self, content: &[u8], resources: &Dict) {
        let mut parser = Parser::content(content);
        let mut operands: Vec<Object> = Vec::new();
        while let Some(item) = parser.next() {
            match item {
                Ok(o) => operands.push(o),
                Err(op) => {
                    if op == b"BI" {
                        self.image_gap(crate::source_images::GapKind::InlineImage,b"");
                        if !operands.is_empty()
                            || !crate::inline_image::skip_raw(&mut parser, self.doc, resources)
                        {
                            self.gs.ink_geometry_complete=false;
                            skip_inline_image(&mut parser);
                        }
                    } else {
                        self.op(&op, &operands, resources);
                    }
                    operands.clear();
                }
            }
        }
    }

    fn num(o: &[Object], i: usize) -> f64 {
        o.get(i).and_then(Object::as_f64).unwrap_or(0.0)
    }

    fn op(&mut self, op: &[u8], o: &[Object], resources: &Dict) {
        self.observe_ink_op(op,o,resources);
        let n = |i: usize| Self::num(o, i);
        match op {
            b"q" => self.stack.push(self.gs.clone()),
            b"Q" => {
                if let Some(g) = self.stack.pop() {
                    self.gs = g;
                }else if self.image_gaps.is_some(){self.image_gap(crate::source_images::GapKind::StateStackUnderflow,b"");self.gs.source_geometry_complete=false;}
            }
            b"cm" => {
                if self.image_gaps.is_some()&&!(o.len()==6&&o.iter().all(|v|v.as_f64().is_some_and(f64::is_finite))){self.image_gap(crate::source_images::GapKind::InvalidContentMatrix,b"");self.gs.source_geometry_complete=false;}
                if let Some(m) = Matrix::from_operands(o) {
                    self.gs.ctm = m.then(&self.gs.ctm);
                }
            }
            b"gs"=>{if let Some(names)=self.gs.source_graphics_states.as_mut(){if let Some(name)=o.first().and_then(Object::as_name){names.push(name.to_vec());}}},
            b"w" => self.gs.line_width = n(0),
            // ---- text ----
            b"BT" => {
                self.tm = Matrix::IDENTITY;
                self.tlm = Matrix::IDENTITY;
            }
            b"Tf" => {
                self.gs.font = o
                    .first()
                    .and_then(Object::as_name)
                    .and_then(|name| self.font(resources, name));
                self.gs.font_size = n(1);
            }
            b"Tc" => self.gs.char_spacing = n(0),
            b"Tw" => self.gs.word_spacing = n(0),
            b"Tz" => self.gs.h_scale = n(0) / 100.0,
            b"TL" => self.gs.leading = n(0),
            b"Ts" => self.gs.rise = n(0),
            b"Tr" => self.gs.render = o.first().and_then(Object::as_i64).unwrap_or(0),
            b"Td" => self.next_line(n(0), n(1)),
            b"TD" => {
                self.gs.leading = -n(1);
                self.next_line(n(0), n(1));
            }
            b"Tm" => {
                if let Some(m) = Matrix::from_operands(o) {
                    self.tm = m;
                    self.tlm = m;
                }
            }
            b"T*" => self.next_line(0.0, -self.gs.leading),
            b"Tj" => {
                if let Some(s) = o.first().and_then(Object::as_str) {
                    self.show_op(s);
                }
            }
            b"'" => {
                self.next_line(0.0, -self.gs.leading);
                if let Some(s) = o.first().and_then(Object::as_str) {
                    self.show_op(s);
                }
            }
            b"\"" => {
                self.gs.word_spacing = n(0);
                self.gs.char_spacing = n(1);
                self.next_line(0.0, -self.gs.leading);
                if let Some(s) = o.get(2).and_then(Object::as_str) {
                    self.show_op(s);
                }
            }
            b"TJ" => {
                self.show_count += 1;
                self.pending_kern = 0.0;
                for item in o.first().and_then(Object::as_array).unwrap_or(&[]) {
                    match item {
                        Object::String(s) => self.show(s),
                        other => {
                            let adj = other.as_f64().unwrap_or(0.0) / 1000.0 * self.gs.font_size;
                            self.pending_kern -= adj;
                            let vertical = self.cur_font().is_some_and(|f| f.vertical);
                            let t = if vertical {
                                Matrix::translate(0.0, -adj)
                            } else {
                                Matrix::translate(-adj * self.gs.h_scale, 0.0)
                            };
                            self.tm = t.then(&self.tm);
                        }
                    }
                }
            }
            // ---- path ----
            b"m" => {
                self.flush_subpath();
                let p = self.to_page(n(0), n(1));
                self.current = Some(SubPath {
                    points: vec![p],
                    closed: false,
                    curved: false,
                    rect: false,
                });
            }
            b"l" => {
                let p = self.to_page(n(0), n(1));
                self.current.get_or_insert_with(empty_subpath).points.push(p);
            }
            b"c" | b"v" | b"y" => {
                let (x, y) = if op == b"c" { (n(4), n(5)) } else { (n(2), n(3)) };
                let p = self.to_page(x, y);
                let sp = self.current.get_or_insert_with(empty_subpath);
                sp.points.push(p);
                sp.curved = true;
            }
            b"h" => {
                if let Some(sp) = self.current.as_mut() {
                    sp.closed = true;
                }
            }
            b"re" => {
                self.flush_subpath();
                let (x, y, w, h) = (n(0), n(1), n(2), n(3));
                let points = vec![
                    self.to_page(x, y),
                    self.to_page(x + w, y),
                    self.to_page(x + w, y + h),
                    self.to_page(x, y + h),
                ];
                self.path.push(SubPath {
                    points,
                    closed: true,
                    curved: false,
                    rect: true,
                });
            }
            b"S" => self.paint(true, false, false),
            b"s" => self.paint(true, false, true),
            b"f" | b"F" | b"f*" => self.paint(false, true, false),
            b"B" | b"B*" => self.paint(true, true, false),
            b"b" | b"b*" => self.paint(true, true, true),
            b"W" | b"W*" => {self.pending_clip=true;if self.image_gaps.is_some()||self.source_rects.is_some(){self.source_pending_clip=Some(if op==b"W*"{crate::source_images::ClipRule::EvenOdd}else{crate::source_images::ClipRule::NonZero});}},
            b"n" => {
                self.flush_subpath();
                self.apply_clip();
                self.path.clear();
            }
            // ---- XObject ----
            b"Do" => {
                if let Some(name) = o.first().and_then(Object::as_name) {
                    self.do_xobject(resources, name);
                }else{self.image_gap(crate::source_images::GapKind::UnknownXObject,b"");}
            }
            _ => {}
        }
    }

    fn next_line(&mut self, tx: f64, ty: f64) {
        self.tlm = Matrix::translate(tx, ty).then(&self.tlm);
        self.tm = self.tlm;
    }

    fn cur_font(&self) -> Option<Rc<Font>> {
        self.gs.font.map(|i| self.fonts[i].clone())
    }

    fn font(&mut self, resources: &Dict, name: &[u8]) -> Option<usize> {
        let fonts = self.doc.dict_get(resources, b"Font")?;
        let entry = fonts.as_dict()?.get(name)?.clone();
        if let Object::Ref(r) = entry {
            if let Some(&id) = self.font_ids.get(&r) {
                return Some(id);
            }
            let dict = self.doc.get(r)?;
            let font = Font::load(self.doc, dict.as_dict()?);
            self.fonts.push(Rc::new(font));
            self.font_ids.insert(r, self.fonts.len() - 1);
            return Some(self.fonts.len() - 1);
        }
        let font = Font::load(self.doc, entry.as_dict()?);
        self.fonts.push(Rc::new(font));
        Some(self.fonts.len() - 1)
    }

    /// Tj・'・" から呼ぶ（TJ は配列の文字列ごとに [`Self::show`] を呼ぶ）。
    fn show_op(&mut self, bytes: &[u8]) {
        self.show_count += 1;
        self.pending_kern = 0.0;
        self.show(bytes);
    }

    fn show(&mut self, bytes: &[u8]) {
        let (Some(font_id), Some(font)) = (self.gs.font, self.cur_font()) else {
            return;
        };
        let gs = &self.gs;
        let fs = gs.font_size;
        let ctm_dev = gs.ctm.then(&self.device);
        for c in font.decode(bytes) {
            let trm = Matrix([fs * gs.h_scale, 0.0, 0.0, fs, 0.0, gs.rise])
                .then(&self.tm)
                .then(&ctm_dev);
            let size = {
                let [_, _, c2, d2, _, _] = trm.0;
                (c2 * c2 + d2 * d2).sqrt()
            };
            let space = c.len == 1 && c.code == 32;
            let extra = gs.char_spacing + if space { gs.word_spacing } else { 0.0 };
            // 字そのものの送り（字間・語間を含まない）と、文字列としての送り。
            let (bbox_pts, own, advance) = if font.vertical {
                let (w1, vx, _vy) = font.vertical_metrics(&c);
                let w0 = font.width(&c);
                // 縦書きの字面は原点（上端の中央）から字送りの分だけ下へ伸びる em の正方形とみなす。
                let corners = [(-vx, w1), (w0 - vx, 0.0)];
                (corners, (0.0, w1), Matrix::translate(0.0, w1 * fs + extra))
            } else {
                let w0 = font.width(&c);
                let corners = [(0.0, font.descent), (w0, font.ascent)];
                (
                    corners,
                    (w0, 0.0),
                    Matrix::translate((w0 * fs + extra) * gs.h_scale, 0.0),
                )
            };
            let (x, y) = trm.apply(0.0, 0.0);
            let (end_x, end_y) = trm.apply(own.0, own.1);
            // text space の長さをページ座標の長さに（字の大きさの比で）直す。
            let to_pt = if fs.abs() > 1e-9 { size / fs.abs() } else { 0.0 };
            let spacing = extra * to_pt * if font.vertical { -1.0 } else { gs.h_scale };
            let kern = std::mem::take(&mut self.pending_kern) * to_pt * if font.vertical { -1.0 } else { gs.h_scale };
            let bbox = bounds(&trm, bbox_pts);
            let ink_rect=if self.source_rects.is_some(){self.capture_ink_rectangle(&font,&c,&trm)}else{None};
            self.tm = advance.then(&self.tm);
            self.glyphs.push(Glyph {
                text: font.unicode(&c),
                code: c.code,
                cid: c.cid,
                font: font_id,
                x,
                y,
                end_x,
                end_y,
                size,
                vertical: font.vertical,
                bbox,
                invisible: matches!(gs.render, 3 | 7),
                spacing,
                kern,
                show: self.show_count,
            });
            if let Some(rects)=&mut self.source_rects{rects.push(ink_rect);}
            if let Some(resolved) = &mut self.source_resolved {
                resolved.push(font.source_aj1_fallback(self.doc, &c));
            }
        }
    }

    fn to_page(&self, x: f64, y: f64) -> [f64; 2] {
        let (px, py) = self.gs.ctm.then(&self.device).apply(x, y);
        [px, py]
    }

    fn flush_subpath(&mut self) {
        if let Some(sp) = self.current.take() {
            if !sp.points.is_empty() {
                self.path.push(sp);
            }
        }
    }

    fn paint(&mut self, stroke: bool, fill: bool, close: bool) {
        if close {
            if let Some(sp) = self.current.as_mut() {
                sp.closed = true;
            }
        }
        self.flush_subpath();
        // `W` で狭めたクリップは、同じパスを描いた後から効く。
        let clip = self.gs.clip;
        self.apply_clip();
        let subpaths = std::mem::take(&mut self.path);
        if subpaths.is_empty() {
            return;
        }
        let line_width = self.gs.line_width * self.gs.ctm.scale();
        self.paths.push(PathItem {
            subpaths,
            stroke,
            fill,
            line_width,
            clip,
        });
    }

    /// `W`・`W*` で指定したクリップを、今のパスの外接矩形で狭める。曲線を含むパスは
    /// 頂点（終点）だけでは外接矩形が決まらないので狭めない。
    fn apply_clip(&mut self) {
        if let Some(rule)=self.source_pending_clip.take(){
            let complete=self.gs.source_geometry_complete&&!self.path.is_empty()&&self.path.iter().all(|sp|!sp.curved&&sp.points.iter().flatten().all(|v|v.is_finite()));
            if let Some(clips)=self.gs.source_clips.as_mut(){clips.push(crate::source_images::SourceClip{origin:crate::source_images::ClipOrigin::Path,rule,subpaths:self.path.clone(),form_box:None,page_quad:None,geometry_complete:complete});}
            if !complete{self.gs.source_geometry_complete=false;self.image_gap(crate::source_images::GapKind::PathGeometryIncomplete,b"");}
        }
        if !std::mem::take(&mut self.pending_clip) || self.path.iter().any(|sp| sp.curved) {
            return;
        }
        let mut pts = self.path.iter().flat_map(|sp| sp.points.iter());
        let Some(&[x, y]) = pts.next() else { return };
        let b = pts.fold([x, y, x, y], |b, &[x, y]| {
            [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]
        });
        self.gs.clip = Some(match self.gs.clip {
            Some(c) => [c[0].max(b[0]), c[1].max(b[1]), c[2].min(b[2]), c[3].min(b[3]),
            ],
            None => b,
        });
    }

    fn image_gap(&mut self,kind:crate::source_images::GapKind,name:&[u8]){
        if let Some(gaps)=self.image_gaps.as_mut(){gaps.push(crate::source_images::CaptureGap{kind,resource_name:name.to_vec(),form_path:self.forms.clone()});}
    }
    fn do_xobject(&mut self,resources:&Dict,name:&[u8]){
        use crate::source_images::{GapKind,SourceClip,ClipOrigin,ClipRule};
        let Some(xobjects)=self.doc.dict_get(resources,b"XObject")else{self.image_gap(GapKind::MissingResource,name);return;};
        let Some(entry)=xobjects.as_dict().and_then(|d|d.get(name)).cloned()else{self.image_gap(GapKind::MissingResource,name);return;};
        let r=entry.as_ref();
        if self.forms.len()>=MAX_FORM_DEPTH{self.image_gap(GapKind::FormDepthLimit,name);return;}
        if r.is_some_and(|r|self.forms.contains(&r)){self.image_gap(GapKind::RecursiveForm,name);return;}
        let obj=self.doc.resolve(&entry);
        let Some(stream)=obj.as_stream()else{self.image_gap(GapKind::UnknownXObject,name);return;};
        let subtype=self.doc.dict_get(&stream.dict,b"Subtype");
        if subtype.as_ref().and_then(Object::as_name)!=Some(b"Form"){
            if self.image_draws.is_some(){
                if subtype.as_ref().and_then(Object::as_name)==Some(b"Image"){
                    let width=crate::source_images::dimension(self.doc.dict_get(&stream.dict,b"Width"));
                    let height=crate::source_images::dimension(self.doc.dict_get(&stream.dict,b"Height"));
                    if width.is_none()||height.is_none(){self.image_gap(GapKind::InvalidImageDimensions,name);}
                    let draw=crate::source_images::ImageDraw{
                        resource_name:name.to_vec(),object_ref:r,sequence:self.image_draws.as_ref().map_or(0,Vec::len),form_path:self.forms.clone(),
                        pixel_width:width,pixel_height:height,image_to_user:self.gs.ctm.0,page_clip_bounds:self.gs.clip,
                        clips:self.gs.source_clips.clone().unwrap_or_default(),source_geometry_complete:self.gs.source_geometry_complete&&self.gs.ctm.0.iter().all(|v|v.is_finite()),graphics_states:self.gs.source_graphics_states.clone().unwrap_or_default(),
                        payload:stream.data.clone(),declared_dict:stream.dict.clone(),
                    };
                    if let Some(draws)=self.image_draws.as_mut(){draws.push(draw);}
                }else{self.image_gap(GapKind::UnknownXObject,name);}
            }
            return;
        }
        let Ok(content)=self.doc.decode_stream(stream)else{self.image_gap(GapKind::FormDecodeFailure,name);return;};
        let resource_value=self.doc.dict_get(&stream.dict,b"Resources");
        let invalid_resources=resource_value.as_ref().is_some_and(|r|!matches!(r,Object::Dict(_)));
        let form_resources=resource_value.and_then(|o|o.as_dict().cloned());
        let matrix_value=self.doc.dict_get(&stream.dict,b"Matrix");
        let matrix=matrix_value.as_ref().and_then(|m|m.as_array().and_then(Matrix::from_operands)).unwrap_or(Matrix::IDENTITY);
        let invalid_matrix=matrix_value.as_ref().is_some_and(|m|m.as_array().is_none_or(|a|a.len()!=6||!a.iter().all(|v|v.as_f64().is_some_and(f64::is_finite))));
        let saved=(self.gs.clone(),self.stack.len(),self.tm,self.tlm,self.current.take(),std::mem::take(&mut self.path));
        let saved_pending=self.source_pending_clip.take();
        self.gs.ctm=matrix.then(&self.gs.ctm);
        if self.image_draws.is_some()||self.source_rects.is_some(){
            if saved_pending.is_some()||saved.4.is_some()||!saved.5.is_empty(){self.image_gap(GapKind::OpenPathAtForm,name);self.gs.source_geometry_complete=false;}
            if invalid_matrix{self.image_gap(GapKind::InvalidFormMatrix,name);self.gs.source_geometry_complete=false;}
            if invalid_resources{self.image_gap(GapKind::InvalidFormResources,name);self.gs.source_geometry_complete=false;}
            let bbox=self.doc.dict_get(&stream.dict,b"BBox").and_then(|o|{let a=o.as_array()?;if a.len()!=4{return None;}let v=a.iter().map(Object::as_f64).collect::<Option<Vec<_>>>()?;(v.iter().all(|x|x.is_finite())&&v[0]<=v[2]&&v[1]<=v[3]).then(||[v[0],v[1],v[2],v[3]])});
            let quad=bbox.map(|[x0,y0,x1,y1]|[[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x,y]|self.to_page(x,y)));
            let complete=self.gs.source_geometry_complete&&quad.is_some_and(|q|q.iter().flatten().all(|v|v.is_finite()));
            if bbox.is_none(){self.image_gap(GapKind::InvalidFormBox,name);}
            if quad.is_some_and(|q|!q.iter().flatten().all(|v|v.is_finite())){self.image_gap(GapKind::UnrepresentableGeometry,name);}
            if !complete{self.gs.source_geometry_complete=false;}
            if let Some(clips)=self.gs.source_clips.as_mut(){clips.push(SourceClip{origin:ClipOrigin::FormBox,rule:ClipRule::NonZero,subpaths:vec![],form_box:bbox,page_quad:quad,geometry_complete:complete});}
        }
        if let Some(r)=r{self.forms.push(r);}
        self.run(&content,form_resources.as_ref().unwrap_or(resources));
        if r.is_some(){self.forms.pop();}
        let(gs,depth,tm,tlm,current,path)=saved;
        self.gs=gs;self.stack.truncate(depth);self.tm=tm;self.tlm=tlm;self.current=current;self.path=path;self.source_pending_clip=saved_pending;
    }

}

fn empty_subpath() -> SubPath {
    SubPath {
        points: Vec::new(),
        closed: false,
        curved: false,
        rect: false,
    }
}

fn bounds(m: &Matrix, [(x0, y0), (x1, y1)]: [(f64, f64); 2]) -> [f64; 4] {
    let pts = [m.apply(x0, y0), m.apply(x1, y0), m.apply(x0, y1), m.apply(x1, y1)];
    let xs = pts.iter().map(|p| p.0);
    let ys = pts.iter().map(|p| p.1);
    [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// `BI ... ID <data> EI` のインライン画像を読み飛ばす。
fn skip_inline_image(parser: &mut Parser) {
    while let Some(item) = parser.next() {
        if let Err(k) = item {
            if k == b"ID" {
                break;
            }
        }
    }
    let data = parser.lexer.data;
    let mut i = parser.pos() + 1;
    while i + 2 <= data.len() {
        if &data[i..i + 2] == b"EI"
            && i > 0
            && crate::lexer::is_whitespace(data[i - 1])
            && data.get(i + 2).is_none_or(|b| crate::lexer::is_whitespace(*b))
        {
            parser.lexer.pos = i + 2;
            return;
        }
        i += 1;
    }
    parser.lexer.pos = data.len();
}

#[path="source_ruling_context.rs"]mod ruling_context;
