use std::borrow::Cow;

#[cfg(feature = "layout-validation")]
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::App;

pub fn register(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = [
        include_bytes!("../../../assets/fonts/Fraunces-Tachyon-ExtraBold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/Fraunces-Tachyon-ExtraBoldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-ExtraLight.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Bold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-ExtraLightItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Italic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-SemiboldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-BoldItalic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Tachyon-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Tachyon-Semibold.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/SplineSansMono-Tachyon-Italic.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/FiraCode-Tachyon-Regular.ttf").as_slice(),
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
        include_bytes!("../../../assets/fonts/NotoSans-Tachyon-Regular.ttf").as_slice(),
        include_bytes!("../../../assets/fonts/NotoSans-Tachyon-Italic.ttf").as_slice(),
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
    const BODY_FONTS: [&[u8]; 8] = [
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-ExtraLight.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Regular.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Semibold.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Bold.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-ExtraLightItalic.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-Italic.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-SemiboldItalic.ttf"),
        include_bytes!("../../../assets/fonts/PublicSans-Tachyon-BoldItalic.ttf"),
    ];

    #[test]
    fn bundled_body_fonts_keep_readable_space_advance() {
        for (data, expected_weight) in BODY_FONTS
            .into_iter()
            .zip([200, 400, 600, 700, 200, 400, 600, 700])
        {
            let face = ttf_parser::Face::parse(data, 0).expect("valid bundled font");
            assert_eq!(face.weight().to_number(), expected_weight);
            let glyph = face.glyph_index(' ').expect("U+0020 glyph");
            let advance = face.glyph_hor_advance(glyph).expect("horizontal advance") as u32;
            let units_per_em = u32::from(face.units_per_em());
            assert!(advance * 5 >= units_per_em);
            assert!(advance * 2 <= units_per_em);
        }
    }

    #[test]
    fn bundled_headline_faces_are_extra_bold() {
        for data in [
            include_bytes!("../../../assets/fonts/Fraunces-Tachyon-ExtraBold.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/Fraunces-Tachyon-ExtraBoldItalic.ttf").as_slice(),
        ] {
            let face = ttf_parser::Face::parse(data, 0).expect("valid bundled headline font");
            assert_eq!(face.weight().to_number(), 800);
        }
    }

    #[test]
    fn bundled_fira_code_retains_programming_ligature_features() {
        let face = ttf_parser::Face::parse(
            include_bytes!("../../../assets/fonts/FiraCode-Tachyon-Regular.ttf"),
            0,
        )
        .expect("valid bundled Fira Code font");
        let features = face
            .tables()
            .gsub
            .expect("Fira Code must retain its substitution table")
            .features;
        assert!(
            features
                .into_iter()
                .any(|feature| feature.tag == ttf_parser::Tag::from_bytes(b"calt")),
            "Fira Code must include its contextual-ligature feature"
        );
    }
}
