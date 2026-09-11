use std::borrow::Cow;

#[cfg(feature = "layout-validation")]
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::App;

pub fn register(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = [
        include_bytes!("../../../assets/fonts/LiberationSerif-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/LiberationSerif-Italic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/LiberationSerif-Bold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/LiberationSerif-BoldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H1-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H1-SemiboldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H2-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H2-SemiboldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H3-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Mineral-H3-SemiboldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Oblique.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-SemiboldOblique.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Mineral-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Mineral-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Mineral-Italic.ttf").as_slice(),
    ]
    .into_iter()
    .map(Cow::Borrowed)
    .collect();

    #[cfg(not(feature = "layout-validation"))]
    let fonts = fonts.into_iter().chain(alternate_body_fonts()).collect();

    cx.text_system()
        .add_fonts(fonts)
        .expect("bundled fonts must be valid");
}

fn alternate_body_fonts() -> Vec<Cow<'static, [u8]>> {
    [
        include_bytes!("../../../assets/fonts/NotoSans-Mineral-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/NotoSans-Mineral-Italic.ttf").as_slice(),
    ]
    .into_iter()
    .map(Cow::Borrowed)
    .collect()
}

/// Register a body family after the validation window is already displaying a
/// document. Production registers the same family during startup; keeping it
/// out of diagnostic startup makes the native acceptance path exercise a real
/// font-completion invalidation instead of only switching between warm faces.
#[cfg(feature = "layout-validation")]
pub fn register_delayed_alternate_for_validation(cx: &mut App) -> bool {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.swap(true, Ordering::AcqRel) {
        return false;
    }
    cx.text_system()
        .add_fonts(alternate_body_fonts())
        .expect("delayed validation fonts must be valid");
    true
}

#[cfg(test)]
mod tests {
    const BODY_FONTS: [&[u8]; 4] = [
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Regular.ttf"),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Semibold.ttf"),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-Oblique.ttf"),
        include_bytes!("../../../assets/fonts/SplineSans-Mineral-SemiboldOblique.ttf"),
    ];

    #[test]
    fn bundled_body_fonts_keep_readable_space_advance() {
        for data in BODY_FONTS {
            let face = ttf_parser::Face::parse(data, 0).expect("valid bundled font");
            let glyph = face.glyph_index(' ').expect("U+0020 glyph");
            let advance = face.glyph_hor_advance(glyph).expect("horizontal advance") as u32;
            let units_per_em = u32::from(face.units_per_em());
            assert_eq!(advance * 10, units_per_em * 3);
        }
    }
}
