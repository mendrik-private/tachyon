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
    pub secondary: u32,
    pub accent: u32,
    pub accent_muted: u32,
    pub selection: u32,
    pub image: u32,
    pub error: u32,
    pub minimap_text: u32,
    pub minimap_placeholder: u32,
}

impl MineralPalette {
    pub const DARK: Self = Self {
        page: 0x18232f,
        panel: 0x141f2a,
        surface: 0x22303d,
        surface_quiet: 0x1e2b37,
        floating: 0x273643,
        hover: 0x293947,
        border: 0x354554,
        text: 0xe8e3d8,
        secondary: 0x91a3b5,
        accent: 0xd59b3b,
        accent_muted: 0x514229,
        selection: 0x514229,
        image: 0x43a17a,
        error: 0xc66b5b,
        minimap_text: 0x718396,
        minimap_placeholder: 0x4a5b6b,
    };

    pub const LIGHT: Self = Self {
        page: 0xfcfbf8,
        panel: 0xfcfbf8,
        surface: 0xffffff,
        surface_quiet: 0xf1efe9,
        floating: 0xffffff,
        hover: 0xf1efe9,
        border: 0xd8d5ce,
        text: 0x1b2430,
        secondary: 0x59636f,
        accent: 0x256f50,
        accent_muted: 0xdcebe1,
        selection: 0xdcebe1,
        image: 0x256f50,
        error: 0x9e4b3f,
        minimap_text: 0x7b858f,
        minimap_placeholder: 0xb1b6b8,
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
        assert_eq!(MineralPalette::DARK.page, 0x18232f);
        assert_eq!(MineralPalette::DARK.text, 0xe8e3d8);
        assert_eq!(MineralPalette::DARK.accent, 0xd59b3b);
        assert_ne!(MineralPalette::LIGHT, MineralPalette::DARK);
        assert_eq!(MineralPalette::with_alpha(0xd59b3b, 0x24), 0xd59b3b24);
    }
}
