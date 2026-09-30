//! The bundled font faces. Each file is embedded once in the binary; the GPUI
//! text system, the HTML preview renderer and the diagram renderer all read
//! these statics.

macro_rules! bundled_fonts {
    ($($name:ident = $file:literal;)*) => {
        $(pub static $name: &[u8] = include_bytes!(concat!("../../../assets/fonts/", $file));)*
    };
}

bundled_fonts! {
    FRAUNCES_EXTRA_BOLD = "Fraunces-Tachyon-ExtraBold.ttf";
    FRAUNCES_EXTRA_BOLD_ITALIC = "Fraunces-Tachyon-ExtraBoldItalic.ttf";
    PUBLIC_SANS_EXTRA_LIGHT = "PublicSans-Tachyon-ExtraLight.ttf";
    PUBLIC_SANS_REGULAR = "PublicSans-Tachyon-Regular.ttf";
    PUBLIC_SANS_SEMIBOLD = "PublicSans-Tachyon-Semibold.ttf";
    PUBLIC_SANS_BOLD = "PublicSans-Tachyon-Bold.ttf";
    PUBLIC_SANS_EXTRA_LIGHT_ITALIC = "PublicSans-Tachyon-ExtraLightItalic.ttf";
    PUBLIC_SANS_ITALIC = "PublicSans-Tachyon-Italic.ttf";
    PUBLIC_SANS_SEMIBOLD_ITALIC = "PublicSans-Tachyon-SemiboldItalic.ttf";
    PUBLIC_SANS_BOLD_ITALIC = "PublicSans-Tachyon-BoldItalic.ttf";
    SPLINE_SANS_MONO_REGULAR = "SplineSansMono-Tachyon-Regular.ttf";
    SPLINE_SANS_MONO_SEMIBOLD = "SplineSansMono-Tachyon-Semibold.ttf";
    SPLINE_SANS_MONO_ITALIC = "SplineSansMono-Tachyon-Italic.ttf";
    FIRA_CODE_REGULAR = "FiraCode-Tachyon-Regular.ttf";
    NOTO_SANS_REGULAR = "NotoSans-Tachyon-Regular.ttf";
    NOTO_SANS_ITALIC = "NotoSans-Tachyon-Italic.ttf";
}
