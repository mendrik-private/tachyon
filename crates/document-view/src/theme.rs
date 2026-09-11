/// Shared Mineral design tokens used by both the document renderer and the
/// application chrome. Keeping the palette in one crate prevents the shell
/// and the rich surface from drifting when the OS appearance changes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MineralPalette {
    pub page: u32,
    pub panel: u32,
    pub surface: u32,
    pub surface_quiet: u32,
    pub floating: u32,
    pub hover: u32,
    pub border: u32,
    pub text: u32,
    pub heading: u32,
    pub secondary: u32,
    pub accent: u32,
    pub accent_muted: u32,
    pub selection: u32,
    pub image: u32,
    pub info: u32,
    pub success: u32,
    pub warning: u32,
    pub error: u32,
    pub minimap_text: u32,
    pub minimap_placeholder: u32,
    pub syntax_keyword: u32,
    pub syntax_string: u32,
    pub syntax_comment: u32,
    pub syntax_number: u32,
}

impl MineralPalette {
    pub const DARK: Self = Self {
        page: 0x0f202d,
        panel: 0x0c1b27,
        surface: 0x152a38,
        surface_quiet: 0x132633,
        floating: 0x203440,
        hover: 0x1b3544,
        border: 0x29465a,
        text: 0xd7e0e7,
        heading: 0xf2eadc,
        secondary: 0x9eb2c2,
        accent: 0xdda43d,
        accent_muted: 0x4f4025,
        selection: 0x574628,
        image: 0x43a17a,
        info: 0x4d9cff,
        success: 0x48c987,
        warning: 0xe1a33d,
        error: 0xed6b65,
        minimap_text: 0x6f8494,
        minimap_placeholder: 0x405767,
        syntax_keyword: 0xedb75f,
        syntax_string: 0x71d5a0,
        syntax_comment: 0xacbbc7,
        syntax_number: 0xceaff0,
    };

    pub const LIGHT: Self = Self {
        page: 0xfaf9f6,
        panel: 0xf2f1ec,
        surface: 0xffffff,
        surface_quiet: 0xf2f1ec,
        floating: 0xffffff,
        hover: 0xe8eee2,
        border: 0xdaddd5,
        text: 0x36423e,
        heading: 0x152421,
        secondary: 0x606b65,
        accent: 0x3f6247,
        accent_muted: 0xe8eee2,
        selection: 0xd5e3cf,
        image: 0x3f6247,
        info: 0x225c83,
        success: 0x3f6247,
        warning: 0x875500,
        error: 0x9b302d,
        minimap_text: 0x7b858f,
        minimap_placeholder: 0xb1b6b8,
        syntax_keyword: 0x82520f,
        syntax_string: 0x286a3f,
        syntax_comment: 0x565e67,
        syntax_number: 0x684d9c,
    };

    #[must_use]
    pub const fn for_dark(dark: bool) -> Self {
        if dark { Self::DARK } else { Self::LIGHT }
    }

    /// Converts a six-digit RGB token to GPUI's eight-digit RGBA layout.
    #[must_use]
    pub const fn with_alpha(color: u32, alpha: u8) -> u32 {
        (color << 8) | alpha as u32
    }

    /// Quiet punctuation blends with its actual containing surface, including
    /// tinted cards and selection, without changing the accessible label text.
    pub(crate) const fn label_delimiter(self) -> u32 {
        Self::with_alpha(self.text, 0x28)
    }

    /// Authored signals have separate surface/foreground roles. Do not derive
    /// their pale backgrounds by compositing saturated text over arbitrary UI.
    pub(crate) fn signal(self, kind: &document_core::AlertKind) -> SignalStyle {
        use document_core::AlertKind;
        let (ink, paper) = match kind {
            AlertKind::Note | AlertKind::Important => (self.info, 0xedf4f9),
            AlertKind::Tip => (self.success, 0xe8eee2),
            AlertKind::Warning => (self.warning, 0xfff4df),
            AlertKind::Caution => (self.error, 0xfbedea),
            AlertKind::Other(_) => (self.secondary, self.surface_quiet),
        };
        SignalStyle {
            ink,
            paper: if self == Self::DARK {
                self.surface
            } else {
                paper
            },
            rule: if self == Self::DARK {
                ink
            } else {
                mix(ink, paper, 0.22)
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SignalStyle {
    pub ink: u32,
    pub paper: u32,
    pub rule: u32,
}

fn mix(ink: u32, paper: u32, amount: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |color, shift| {
        let a = ((ink >> shift) & 255) as f32;
        let b = ((paper >> shift) & 255) as f32;
        color | (((b + (a - b) * amount).round() as u32) << shift)
    })
}

/// Screen tokens from designs/document-design-grammar.md. All dimensions are
/// logical document pixels; the layout scales them once with reader zoom.
pub(crate) struct DocumentStyle;

impl DocumentStyle {
    // Ordinary prose, list bodies and card bodies share the opening specimen's
    // size. Reading roles still select their existing font families.
    pub const BODY_SIZE: f32 = 21.;
    pub const BODY_LEADING: f32 = 32.;
    pub const CHECKBOX_SIZE: f32 = 18.;
    pub const CHECKBOX_TEXT_GAP: f32 = 13.;
    pub const TASK_INDENT_EXTRA: f32 = Self::CHECKBOX_SIZE + Self::CHECKBOX_TEXT_GAP + 1. - 24.;
    pub const TASK_PROGRESS_GAP: f32 = Self::BODY_LEADING * 0.7;
    pub const TASK_HEADER_HEIGHT: f32 =
        4. + Self::CAPTION_LEADING + 8. + 8. + Self::TASK_PROGRESS_GAP;
    pub const REFERENCE_SIZE: f32 = Self::BODY_SIZE;
    pub const REFERENCE_LEADING: f32 = Self::BODY_LEADING;
    pub const READING_SIZE: f32 = Self::BODY_SIZE;
    pub const READING_LEADING: f32 = Self::BODY_LEADING;
    pub const LEAD_SIZE: f32 = 21.;
    pub const LEAD_LEADING: f32 = 32.;
    pub const OPENING_SECTION_GAP: f32 = 48.;
    pub const TITLE_SECTION_HEADING_GAP: f32 = 28.;
    pub const MAJOR_SECTION_GAP: f32 = 64.;
    pub const REFERENCE_SECTION_GAP: f32 = 32.;
    pub const SUBSECTION_GAP: f32 = 40.;
    pub const CODE_SIZE: f32 = 14.;
    pub const CODE_LEADING: f32 = 21.;
    pub const CAPTION_SIZE: f32 = 13.;
    pub const CAPTION_LEADING: f32 = 18.;
    pub const METADATA_SIZE: f32 = 14.;
    pub const METADATA_LEADING: f32 = 20.;
    pub const TABLE_SIZE: f32 = 14.;
    pub const TABLE_LEADING: f32 = 21.;
    pub const FEATURE_TITLE_SIZE: f32 = 24.;
    pub const QUOTE_INSET: f32 = 24.;
    pub const QUOTE_PADDING: f32 = 16.;
    pub const PULL_QUOTE_SIZE: f32 = 24.;
    pub const PULL_QUOTE_LEADING: f32 = 32.;
    pub const BIBLIOGRAPHY_HANG: f32 = 24.;
    pub const BIBLIOGRAPHY_GAP: f32 = 12.;
    pub const EQUATION_GAP: f32 = 24.;
    pub const EQUATION_SCROLL_RAIL: f32 = 16.;
    pub const METRIC_VALUE_SIZE: f32 = 40.;
    pub const METRIC_VALUE_LEADING: f32 = 44.;
    pub const BADGE_INSET: f32 = 8.;
    pub const BADGE_VERTICAL_INSET: f32 = 4.;
    pub const SINGLE_COLUMN_WIDTH: f32 = 560.;
    pub const COMPACT_TITLE_SIZE: f32 = 44.;
    pub const COMPACT_TITLE_LEADING: f32 = 50.;
    /// Conservative fallback before native fonts are available. Loaded fonts
    /// supply the actual 72-character measure through FontMeasurement.
    pub const PROSE_WIDTH: f32 = 600.;
    pub const PROSE_CHARACTERS: f32 = 72.;
    /// Bounded expansion for standalone prose and broad reading bands.
    pub const MAX_PROSE_CHARACTERS: f32 = 84.;
    pub const GUTTER: f32 = 24.;
    pub const INSTRUCTION_GAP: f32 = 12.;
    pub const INSET: f32 = 24.;
    pub const RADIUS: f32 = 4.;
    pub const MEDIA_RADIUS: f32 = 8.;
    pub const CODE_DARK: u32 = 0x202a2b;
    pub const CODE_TEXT: u32 = 0xf0f2ed;
    /// Dark executable panels retain pale syntax while selected, independent
    /// of the surrounding page appearance.
    pub const CODE_SELECTION: u32 = 0x30473d;

    pub fn heading(level: u8) -> (f32, f32) {
        match level {
            1 => (52., 58.),
            2 => (30., 34.5),
            3 => (24., 28.8),
            4 => (18., 23.4),
            _ => (17., 22.1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_keep_the_contract_tokens_and_distinct_appearances() {
        assert_eq!(MineralPalette::LIGHT.page, 0xfaf9f6);
        assert_eq!(MineralPalette::LIGHT.text, 0x36423e);
        assert_eq!(MineralPalette::LIGHT.accent, 0x3f6247);
        assert_eq!(MineralPalette::DARK.page, 0x0f202d);
        assert_eq!(MineralPalette::DARK.text, 0xd7e0e7);
        assert_eq!(MineralPalette::DARK.heading, 0xf2eadc);
        assert_eq!(MineralPalette::DARK.accent, 0xdda43d);
        assert_ne!(MineralPalette::LIGHT, MineralPalette::DARK);
        assert_eq!(MineralPalette::with_alpha(0xdda43d, 0x24), 0xdda43d24);
    }

    #[test]
    fn grammar_signal_surfaces_are_exact_and_distinct() {
        use document_core::AlertKind::*;
        assert_eq!(
            MineralPalette::DARK.signal(&Tip).ink,
            MineralPalette::DARK.success
        );
        assert_ne!(
            MineralPalette::DARK.signal(&Tip).ink,
            MineralPalette::DARK.signal(&Warning).ink
        );
        for (kind, ink, paper) in [
            (Note, 0x225c83, 0xedf4f9),
            (Tip, 0x3f6247, 0xe8eee2),
            (Important, 0x225c83, 0xedf4f9),
            (Warning, 0x875500, 0xfff4df),
            (Caution, 0x9b302d, 0xfbedea),
        ] {
            let style = MineralPalette::LIGHT.signal(&kind);
            assert_eq!((style.ink, style.paper), (ink, paper));
            assert_ne!(style.rule, style.ink);
            assert_ne!(style.rule, style.paper);
        }
    }
}
