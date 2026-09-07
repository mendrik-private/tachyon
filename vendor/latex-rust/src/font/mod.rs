//! OpenType math font metrics. Integer font units → [`Dim`](crate::Dim).

use ttf_parser::Face;

use crate::dim::Dim;
use crate::error::{Error, FontError};

/// Embedded STIX Two Math Regular 2.13 (SIL OFL 1.1).
pub const STIX_TWO_MATH_OTF: &[u8] =
    include_bytes!("../../fonts/stix-two-math/STIXTwoMath-Regular.otf");

/// SHA-256 (hex) of [`STIX_TWO_MATH_OTF`]. Locked by gold.
pub const STIX_TWO_MATH_SHA256: &str =
    "f2076b9f1676438439dd41e23676f5ab99056e83d6b8f8c27841591ef2ccfa72";

/// Face name as shipped.
pub const STIX_TWO_MATH_NAME: &str = "STIX Two Math";

/// Horizontal glyph metrics in font units and em.
///
/// # Examples
///
/// ```
/// use latex_rust::MathFont;
///
/// let font = MathFont::stix_two_math().unwrap();
/// let g = font.glyph('x').unwrap();
/// assert_eq!(g.ch, 'x');
/// assert!(!g.advance.is_zero());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphMetrics {
    /// Character requested.
    pub ch: char,
    /// OpenType glyph id.
    pub glyph_id: u16,
    /// Horizontal advance, font units.
    pub advance_fu: u16,
    /// Advance in em.
    pub advance: Dim,
    /// Height above baseline in em (`max(y_max, 0)`).
    pub height: Dim,
    /// Depth below baseline in em (`max(-y_min, 0)`).
    pub depth: Dim,
}

/// Loaded math face.
///
/// # Examples
///
/// ```
/// use latex_rust::MathFont;
///
/// let font = MathFont::stix_two_math().unwrap();
/// assert_eq!(font.units_per_em(), 1000);
/// ```
pub struct MathFont {
    raw: &'static [u8],
    face: Face<'static>,
    units_per_em: u16,
    ascender_fu: i16,
    descender_fu: i16,
}

impl MathFont {
    /// Load the embedded STIX Two Math Regular face.
    ///
    /// # Errors
    ///
    /// [`crate::FontError::InvalidFace`] if the embedded bytes are not a usable OpenType face.
    ///
    /// # Examples
    ///
    /// ```
    /// use latex_rust::MathFont;
    /// assert!(MathFont::stix_two_math().is_ok());
    /// ```
    pub fn stix_two_math() -> Result<Self, Error> {
        Self::from_bytes(STIX_TWO_MATH_OTF)
    }

    /// Parse OpenType bytes. Lifetime is `'static` for the embedded font only;
    /// this constructor requires a static buffer so the face can be rebuilt.
    pub fn from_bytes(raw: &'static [u8]) -> Result<Self, Error> {
        let face = Face::parse(raw, 0).map_err(|_| FontError::InvalidFace)?;
        let units_per_em = face.units_per_em();
        if units_per_em == 0 {
            return Err(FontError::InvalidFace.into());
        }
        let ascender_fu = face.ascender();
        let descender_fu = face.descender();
        Ok(Self {
            raw,
            face,
            units_per_em,
            ascender_fu,
            descender_fu,
        })
    }

    pub(crate) fn face(&self) -> &Face<'static> {
        &self.face
    }

    /// OpenType bytes this face was parsed from.
    #[must_use]
    pub fn bytes(&self) -> &'static [u8] {
        self.raw
    }

    /// `unitsPerEm` from the `head` table.
    #[must_use]
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// `hhea` ascender in font units.
    #[must_use]
    pub fn ascender_fu(&self) -> i16 {
        self.ascender_fu
    }

    /// `hhea` descender in font units (typically negative).
    #[must_use]
    pub fn descender_fu(&self) -> i16 {
        self.descender_fu
    }

    /// Ascender in em.
    #[must_use]
    pub fn ascender(&self) -> Dim {
        Dim::from_font_units(i64::from(self.ascender_fu), self.units_per_em)
    }

    /// Depth below baseline from `hhea` descender, in em (non-negative).
    #[must_use]
    pub fn descender(&self) -> Dim {
        let d = i64::from(self.descender_fu);
        Dim::from_font_units(-d, self.units_per_em)
    }

    /// Metrics for `ch`, or [`FontError::MissingGlyph`].
    pub fn glyph(&self, ch: char) -> Result<GlyphMetrics, Error> {
        let face = self.face();
        let gid = face.glyph_index(ch).ok_or(FontError::MissingGlyph { ch })?;
        let advance_fu = face
            .glyph_hor_advance(gid)
            .ok_or(FontError::MissingGlyph { ch })?;
        let mut height_fu = 0i64;
        let mut depth_fu = 0i64;
        if let Some(bbox) = face.glyph_bounding_box(gid) {
            height_fu = i64::from(bbox.y_max).max(0);
            depth_fu = i64::from(-bbox.y_min).max(0);
        }
        let upem = self.units_per_em;
        Ok(GlyphMetrics {
            ch,
            glyph_id: gid.0,
            advance_fu,
            advance: Dim::from_font_units(i64::from(advance_fu), upem),
            height: Dim::from_font_units(height_fu, upem),
            depth: Dim::from_font_units(depth_fu, upem),
        })
    }

    /// Metrics for OpenType glyph id `gid`, tagged with `ch` for the box payload.
    pub fn glyph_id(&self, ch: char, gid: u16) -> Result<GlyphMetrics, Error> {
        let face = self.face();
        let gid = ttf_parser::GlyphId(gid);
        let advance_fu = face
            .glyph_hor_advance(gid)
            .ok_or(FontError::MissingGlyph { ch })?;
        let mut height_fu = 0i64;
        let mut depth_fu = 0i64;
        if let Some(bbox) = face.glyph_bounding_box(gid) {
            height_fu = i64::from(bbox.y_max).max(0);
            depth_fu = i64::from(-bbox.y_min).max(0);
        }
        let upem = self.units_per_em;
        Ok(GlyphMetrics {
            ch,
            glyph_id: gid.0,
            advance_fu,
            advance: Dim::from_font_units(i64::from(advance_fu), upem),
            height: Dim::from_font_units(height_fu, upem),
            depth: Dim::from_font_units(depth_fu, upem),
        })
    }

    /// MATH italic correction for `glyph_id`, or zero.
    pub fn italic_correction(&self, glyph_id: u16) -> Dim {
        let face = self.face();
        let Some(math) = face.tables().math else {
            return Dim::zero();
        };
        let Some(info) = math.glyph_info else {
            return Dim::zero();
        };
        let Some(table) = info.italic_corrections else {
            return Dim::zero();
        };
        match table.get(ttf_parser::GlyphId(glyph_id)) {
            Some(v) => Dim::from_font_units(i64::from(v.value), self.units_per_em),
            None => Dim::zero(),
        }
    }

    /// MATH top-accent attachment (em from glyph left), if present.
    pub fn top_accent_attachment(&self, glyph_id: u16) -> Option<Dim> {
        let face = self.face();
        let math = face.tables().math?;
        let info = math.glyph_info?;
        let table = info.top_accent_attachments?;
        let v = table.get(ttf_parser::GlyphId(glyph_id))?;
        Some(Dim::from_font_units(i64::from(v.value), self.units_per_em))
    }

    /// Horizontal glyph-assembly parts: `(gid, start_connector, end_connector, advance, extender)`.
    /// Lengths are font units.
    pub fn horizontal_assembly_parts(&self, glyph_id: u16) -> Vec<(u16, u16, u16, u16, bool)> {
        let mut out = Vec::new();
        let face = self.face();
        let Some(math) = face.tables().math else {
            return out;
        };
        let Some(variants) = math.variants else {
            return out;
        };
        let Some(cons) = variants
            .horizontal_constructions
            .get(ttf_parser::GlyphId(glyph_id))
        else {
            return out;
        };
        let Some(assembly) = cons.assembly else {
            return out;
        };
        for i in 0..assembly.parts.len() {
            if let Some(p) = assembly.parts.get(i) {
                out.push((
                    p.glyph_id.0,
                    p.start_connector_length,
                    p.end_connector_length,
                    p.full_advance,
                    p.part_flags.extender(),
                ));
            }
        }
        out
    }

    /// Horizontal MATH variants of `glyph_id`, including the base glyph first.
    pub fn horizontal_variants(&self, glyph_id: u16) -> Vec<u16> {
        let mut out = vec![glyph_id];
        let face = self.face();
        let Some(math) = face.tables().math else {
            return out;
        };
        let Some(variants) = math.variants else {
            return out;
        };
        let Some(cons) = variants
            .horizontal_constructions
            .get(ttf_parser::GlyphId(glyph_id))
        else {
            return out;
        };
        for i in 0..cons.variants.len() {
            if let Some(v) = cons.variants.get(i) {
                out.push(v.variant_glyph.0);
            }
        }
        out
    }

    /// Vertical MATH variants of `glyph_id`, including the base glyph first.
    pub fn vertical_variants(&self, glyph_id: u16) -> Vec<u16> {
        let mut out = vec![glyph_id];
        let face = self.face();
        let Some(math) = face.tables().math else {
            return out;
        };
        let Some(variants) = math.variants else {
            return out;
        };
        let Some(cons) = variants
            .vertical_constructions
            .get(ttf_parser::GlyphId(glyph_id))
        else {
            return out;
        };
        for i in 0..cons.variants.len() {
            if let Some(v) = cons.variants.get(i) {
                out.push(v.variant_glyph.0);
            }
        }
        out
    }

    /// SHA-256 hex of the raw face bytes.
    #[must_use]
    pub fn sha256_hex(bytes: &[u8]) -> String {
        let d = crate::hash::sha256(bytes);
        let mut s = String::with_capacity(64);
        for b in d {
            s.push_str(&hex_byte(b));
        }
        s
    }
}

fn hex_byte(b: u8) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let hi = H[(b >> 4) as usize];
    let lo = H[(b & 0xf) as usize];
    let mut out = String::with_capacity(2);
    out.push(hi as char);
    out.push(lo as char);
    out
}
