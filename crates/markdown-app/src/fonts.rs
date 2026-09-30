use std::borrow::Cow;

#[cfg(feature = "layout-validation")]
use std::sync::atomic::{AtomicBool, Ordering};

use document_view::fonts;
use gpui::App;

pub fn register(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = [
        fonts::FRAUNCES_EXTRA_BOLD,
        fonts::FRAUNCES_EXTRA_BOLD_ITALIC,
        fonts::PUBLIC_SANS_EXTRA_LIGHT,
        fonts::PUBLIC_SANS_REGULAR,
        fonts::PUBLIC_SANS_SEMIBOLD,
        fonts::PUBLIC_SANS_BOLD,
        fonts::PUBLIC_SANS_EXTRA_LIGHT_ITALIC,
        fonts::PUBLIC_SANS_ITALIC,
        fonts::PUBLIC_SANS_SEMIBOLD_ITALIC,
        fonts::PUBLIC_SANS_BOLD_ITALIC,
        fonts::SPLINE_SANS_MONO_REGULAR,
        fonts::SPLINE_SANS_MONO_SEMIBOLD,
        fonts::SPLINE_SANS_MONO_ITALIC,
        fonts::FIRA_CODE_REGULAR,
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
    [fonts::NOTO_SANS_REGULAR, fonts::NOTO_SANS_ITALIC]
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
    use super::fonts;

    static BODY_FONTS: [&[u8]; 8] = [
        fonts::PUBLIC_SANS_EXTRA_LIGHT,
        fonts::PUBLIC_SANS_REGULAR,
        fonts::PUBLIC_SANS_SEMIBOLD,
        fonts::PUBLIC_SANS_BOLD,
        fonts::PUBLIC_SANS_EXTRA_LIGHT_ITALIC,
        fonts::PUBLIC_SANS_ITALIC,
        fonts::PUBLIC_SANS_SEMIBOLD_ITALIC,
        fonts::PUBLIC_SANS_BOLD_ITALIC,
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
            fonts::FRAUNCES_EXTRA_BOLD,
            fonts::FRAUNCES_EXTRA_BOLD_ITALIC,
        ] {
            let face = ttf_parser::Face::parse(data, 0).expect("valid bundled headline font");
            assert_eq!(face.weight().to_number(), 800);
        }
    }

    #[test]
    fn bundled_fira_code_retains_programming_ligature_features() {
        let face = ttf_parser::Face::parse(fonts::FIRA_CODE_REGULAR, 0)
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
