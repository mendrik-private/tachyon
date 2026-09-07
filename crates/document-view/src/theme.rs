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
        syntax_keyword: 0xe5a84a,
        syntax_string: 0x71d5a0,
        syntax_comment: 0x7f98a9,
        syntax_number: 0xc09bea,
    };

    pub const LIGHT: Self = Self {
        page: 0xfcfbf8,
        panel: 0xf3f2ed,
        surface: 0xffffff,
        surface_quiet: 0xf0f1eb,
        floating: 0xffffff,
        hover: 0xe7ecdf,
        border: 0xdedfd7,
        text: 0x1b2430,
        heading: 0x152820,
        secondary: 0x59636f,
        accent: 0x256f50,
        accent_muted: 0xdcebe1,
        selection: 0xdcebe1,
        image: 0x256f50,
        info: 0x246fbd,
        success: 0x257a50,
        warning: 0x9a6819,
        error: 0x9e4b3f,
        minimap_text: 0x7b858f,
        minimap_placeholder: 0xb1b6b8,
        syntax_keyword: 0x8f5b13,
        syntax_string: 0x327549,
        syntax_comment: 0x6f7780,
        syntax_number: 0x735aa8,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_keep_the_contract_tokens_and_distinct_appearances() {
        assert_eq!(MineralPalette::LIGHT.page, 0xfcfbf8);
        assert_eq!(MineralPalette::LIGHT.text, 0x1b2430);
        assert_eq!(MineralPalette::LIGHT.accent, 0x256f50);
        assert_eq!(MineralPalette::DARK.page, 0x0f202d);
        assert_eq!(MineralPalette::DARK.text, 0xd7e0e7);
        assert_eq!(MineralPalette::DARK.heading, 0xf2eadc);
        assert_eq!(MineralPalette::DARK.accent, 0xdda43d);
        assert_ne!(MineralPalette::LIGHT, MineralPalette::DARK);
        assert_eq!(MineralPalette::with_alpha(0xdda43d, 0x24), 0xdda43d24);
    }
}
