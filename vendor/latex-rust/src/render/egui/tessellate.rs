//! TrueType outline → triangle mesh in font units (y-up).
//!
//! `ttf-parser` delivers `f32` outline points; those bits become [`Dim`]
//! immediately, then integer millifont-units for flatten / earcut (render path,
//! not layout). Vertices are stored as [`Dim`] for emission.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ttf_parser::{GlyphId, OutlineBuilder};

use crate::dim::Dim;
use crate::error::{Error, FontError};
use crate::font::MathFont;

/// Sub-font-unit scale for integer flatten / earcut.
const SCALE: i64 = 64;
const FLAT_STEPS: i64 = 8;

type Ipt = (i64, i64);

/// Cached tessellation: font-unit vertices (y-up) and triangle indices.
#[derive(Clone, Debug)]
pub(super) struct GlyphTris {
    pub vertices: Vec<(Dim, Dim)>,
    pub indices: Vec<u32>,
}

fn glyph_cache() -> &'static Mutex<HashMap<u16, GlyphTris>> {
    static CACHE: OnceLock<Mutex<HashMap<u16, GlyphTris>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Tessellate `glyph_id`, reusing a process-wide triangle cache on later calls.
pub(super) fn tessellate(font: &MathFont, glyph_id: u16) -> Result<GlyphTris, Error> {
    if let Ok(guard) = glyph_cache().lock() {
        if let Some(t) = guard.get(&glyph_id) {
            return Ok(t.clone());
        }
    }
    let tris = tessellate_uncached(font, glyph_id)?;
    if let Ok(mut guard) = glyph_cache().lock() {
        guard.insert(glyph_id, tris.clone());
    }
    Ok(tris)
}

fn tessellate_uncached(font: &MathFont, glyph_id: u16) -> Result<GlyphTris, Error> {
    let face = font.face();
    let mut b = ContourBuilder::new();
    if face.outline_glyph(GlyphId(glyph_id), &mut b).is_none() {
        return Err(FontError::MissingGlyph { ch: '\u{FFFD}' }.into());
    }
    b.close_current();
    let contours = b.contours;
    if contours.is_empty() {
        return Err(FontError::MissingGlyph { ch: '\u{FFFD}' }.into());
    }
    let polys = combine_contours(contours)?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for poly in polys {
        let tris = earcut(&poly)?;
        let base = vertices.len() as u32;
        vertices.extend(poly.into_iter().map(from_fix));
        for [a, b, c] in tris {
            indices.push(base + a);
            indices.push(base + b);
            indices.push(base + c);
        }
    }
    if indices.is_empty() {
        return Err(Error::Unsupported {
            what: "glyph tessellation produced no triangles".into(),
        });
    }
    Ok(GlyphTris { vertices, indices })
}

fn from_fix(p: Ipt) -> (Dim, Dim) {
    let s = Dim::from_i64(SCALE);
    (Dim::from_i64(p.0) / &s, Dim::from_i64(p.1) / s)
}

fn floor_i64(d: &Dim) -> i64 {
    if d.is_nan() {
        return 0;
    }
    let neg = matches!(d.cmp(&Dim::zero()), Some(core::cmp::Ordering::Less));
    let abs = d.abs();
    let n = i64::from(abs.floor_to_u32().unwrap_or(0));
    if !neg {
        n
    } else if abs.eq_dim(&Dim::from_i64(n)) {
        -n
    } else {
        -n - 1
    }
}

fn to_fix(x: f32) -> i64 {
    let d = Dim::from_ieee32_bits(x.to_bits()) * Dim::from_i64(SCALE);
    floor_i64(&d)
}

struct ContourBuilder {
    contours: Vec<Vec<Ipt>>,
    current: Vec<Ipt>,
    last: Option<Ipt>,
}

impl ContourBuilder {
    fn new() -> Self {
        Self {
            contours: Vec::new(),
            current: Vec::new(),
            last: None,
        }
    }

    fn pt(x: f32, y: f32) -> Ipt {
        (to_fix(x), to_fix(y))
    }

    fn push(&mut self, p: Ipt) {
        if let Some(last) = self.current.last() {
            if *last == p {
                return;
            }
        }
        self.current.push(p);
        self.last = Some(p);
    }

    fn close_current(&mut self) {
        if self.current.len() >= 3 {
            if let (Some(&first), Some(&last)) = (self.current.first(), self.current.last()) {
                if first == last {
                    self.current.pop();
                }
            }
            if self.current.len() >= 3 {
                self.contours.push(std::mem::take(&mut self.current));
            } else {
                self.current.clear();
            }
        } else {
            self.current.clear();
        }
        self.last = None;
    }
}

impl OutlineBuilder for ContourBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_current();
        let p = Self::pt(x, y);
        self.push(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(Self::pt(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(p0) = self.last else {
            return;
        };
        let p1 = Self::pt(x1, y1);
        let p2 = Self::pt(x, y);
        flatten_quad(p0, p1, p2, self);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(p0) = self.last else {
            return;
        };
        let p1 = Self::pt(x1, y1);
        let p2 = Self::pt(x2, y2);
        let p3 = Self::pt(x, y);
        flatten_cubic(p0, p1, p2, p3, self);
    }

    fn close(&mut self) {
        self.close_current();
    }
}

fn lerp(a: Ipt, b: Ipt, t_num: i64, t_den: i64) -> Ipt {
    (
        a.0 + (b.0 - a.0) * t_num / t_den,
        a.1 + (b.1 - a.1) * t_num / t_den,
    )
}

fn eval_quad(p0: Ipt, p1: Ipt, p2: Ipt, t_num: i64, t_den: i64) -> Ipt {
    let a = lerp(p0, p1, t_num, t_den);
    let b = lerp(p1, p2, t_num, t_den);
    lerp(a, b, t_num, t_den)
}

fn eval_cubic(p0: Ipt, p1: Ipt, p2: Ipt, p3: Ipt, t_num: i64, t_den: i64) -> Ipt {
    let a = lerp(p0, p1, t_num, t_den);
    let b = lerp(p1, p2, t_num, t_den);
    let c = lerp(p2, p3, t_num, t_den);
    let d = lerp(a, b, t_num, t_den);
    let e = lerp(b, c, t_num, t_den);
    lerp(d, e, t_num, t_den)
}

fn flatten_quad(p0: Ipt, p1: Ipt, p2: Ipt, b: &mut ContourBuilder) {
    for i in 1..=FLAT_STEPS {
        b.push(eval_quad(p0, p1, p2, i, FLAT_STEPS));
    }
}

fn flatten_cubic(p0: Ipt, p1: Ipt, p2: Ipt, p3: Ipt, b: &mut ContourBuilder) {
    for i in 1..=FLAT_STEPS {
        b.push(eval_cubic(p0, p1, p2, p3, i, FLAT_STEPS));
    }
}

fn signed_area(poly: &[Ipt]) -> i64 {
    let n = poly.len();
    let mut a = 0i64;
    for i in 0..n {
        let j = (i + 1) % n;
        a += poly[i].0 * poly[j].1 - poly[j].0 * poly[i].1;
    }
    a
}

fn dist2(a: Ipt, b: Ipt) -> i64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

fn point_in_poly(poly: &[Ipt], p: Ipt) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let a_below = a.1 <= p.1;
        let b_above = b.1 > p.1;
        let b_below = b.1 <= p.1;
        let a_above = a.1 > p.1;
        if (a_below && b_above) || (b_below && a_above) {
            let dy = b.1 - a.1;
            if dy == 0 {
                continue;
            }
            let xint = a.0 + (p.1 - a.1) * (b.0 - a.0) / dy;
            if xint > p.0 {
                inside = !inside;
            }
        }
    }
    inside
}

fn combine_contours(contours: Vec<Vec<Ipt>>) -> Result<Vec<Vec<Ipt>>, Error> {
    if contours.is_empty() {
        return Ok(Vec::new());
    }
    let areas: Vec<i64> = contours.iter().map(|c| signed_area(c)).collect();
    let mut outer_idx = Vec::new();
    let mut hole_idx = Vec::new();
    for (i, a) in areas.iter().enumerate() {
        if *a == 0 {
            continue;
        }
        if *a < 0 {
            hole_idx.push(i);
        } else {
            outer_idx.push(i);
        }
    }
    if outer_idx.is_empty() && !hole_idx.is_empty() {
        outer_idx = hole_idx;
        hole_idx = Vec::new();
    }
    if outer_idx.is_empty() {
        return Ok(contours);
    }
    let outer_sign_pos = areas[outer_idx[0]] > 0;
    outer_idx.clear();
    hole_idx.clear();
    for (i, a) in areas.iter().enumerate() {
        if *a == 0 {
            continue;
        }
        if (*a > 0) == outer_sign_pos {
            outer_idx.push(i);
        } else {
            hole_idx.push(i);
        }
    }
    let mut outers: Vec<Vec<Ipt>> = outer_idx.into_iter().map(|i| contours[i].clone()).collect();
    for h in hole_idx {
        let hole = &contours[h];
        let Some(&pt) = hole.first() else {
            continue;
        };
        let mut host = 0usize;
        let mut found = false;
        for (oi, outer) in outers.iter().enumerate() {
            if point_in_poly(outer, pt) {
                host = oi;
                found = true;
                break;
            }
        }
        if !found {
            let mut best = 0i64;
            for (oi, outer) in outers.iter().enumerate() {
                let aa = signed_area(outer).abs();
                if aa > best {
                    best = aa;
                    host = oi;
                }
            }
        }
        insert_hole(&mut outers[host], hole);
    }
    Ok(outers)
}

fn insert_hole(outer: &mut Vec<Ipt>, hole: &[Ipt]) {
    if hole.len() < 3 || outer.len() < 3 {
        return;
    }
    let mut best_i = 0usize;
    let mut best_j = 0usize;
    let mut best_d = dist2(outer[0], hole[0]);
    for (i, &op) in outer.iter().enumerate() {
        for (j, &hp) in hole.iter().enumerate() {
            let d = dist2(op, hp);
            if d < best_d {
                best_d = d;
                best_i = i;
                best_j = j;
            }
        }
    }
    let mut spliced = Vec::with_capacity(outer.len() + hole.len() + 2);
    spliced.extend_from_slice(&outer[..=best_i]);
    let hn = hole.len();
    for k in 0..hn {
        spliced.push(hole[(best_j + k) % hn]);
    }
    spliced.push(hole[best_j]);
    spliced.push(outer[best_i]);
    spliced.extend_from_slice(&outer[best_i + 1..]);
    *outer = spliced;
}

fn cross(a: Ipt, b: Ipt, c: Ipt) -> i64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn triangle_contains(a: Ipt, b: Ipt, c: Ipt, p: Ipt) -> bool {
    let c1 = cross(a, b, p);
    let c2 = cross(b, c, p);
    let c3 = cross(c, a, p);
    (c1 > 0 && c2 > 0 && c3 > 0) || (c1 < 0 && c2 < 0 && c3 < 0)
}

fn earcut(poly: &[Ipt]) -> Result<Vec<[u32; 3]>, Error> {
    let n0 = poly.len();
    if n0 < 3 {
        return Ok(Vec::new());
    }
    let area = signed_area(poly);
    let ccw = area > 0;
    let mut idx: Vec<usize> = (0..n0).collect();
    if !ccw {
        idx.reverse();
    }
    let mut tris = Vec::new();
    let mut guard = 0usize;
    let max_guard = n0 * n0 + 8;
    while idx.len() > 3 {
        guard += 1;
        if guard > max_guard {
            return Err(Error::Unsupported {
                what: "glyph tessellation".into(),
            });
        }
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let prev = idx[(i + m - 1) % m];
            let cur = idx[i];
            let next = idx[(i + 1) % m];
            let a = poly[prev];
            let b = poly[cur];
            let c = poly[next];
            if cross(a, b, c) <= 0 {
                continue;
            }
            let mut empty = true;
            for (k, &vi) in idx.iter().enumerate() {
                if k == (i + m - 1) % m || k == i || k == (i + 1) % m {
                    continue;
                }
                if triangle_contains(a, b, c, poly[vi]) {
                    empty = false;
                    break;
                }
            }
            if !empty {
                continue;
            }
            tris.push([prev as u32, cur as u32, next as u32]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            return Err(Error::Unsupported {
                what: "glyph tessellation".into(),
            });
        }
    }
    if idx.len() == 3 {
        tris.push([idx[0] as u32, idx[1] as u32, idx[2] as u32]);
    }
    Ok(tris)
}
