//! Trusted native fonts, independent of the fragment's denied resource URLs.
//! Keep the bundled defaults, but let Fontique select installed fonts for
//! scripts/emoji that those faces do not cover. Nothing is downloaded.

use std::sync::{Mutex, OnceLock};

use blitz_dom::{FontContext, build_single_font_ctx};

pub(super) fn context() -> FontContext {
    static FONTS: OnceLock<Mutex<FontContext>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = build_single_font_ctx(include_bytes!(
                "../../../../assets/fonts/PublicSans-Tachyon-ExtraLight.ttf"
            ));
            for bytes in [
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-Regular.ttf")
                    .as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-Semibold.ttf")
                    .as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-Bold.ttf").as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-ExtraLightItalic.ttf")
                    .as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-Italic.ttf").as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-SemiboldItalic.ttf")
                    .as_slice(),
                include_bytes!("../../../../assets/fonts/PublicSans-Tachyon-BoldItalic.ttf")
                    .as_slice(),
                include_bytes!("../../../../assets/fonts/SplineSansMono-Tachyon-Regular.ttf")
                    .as_slice(),
                blitz_dom::BULLET_FONT,
            ] {
                fonts.collection.register_fonts(bytes.to_vec().into(), None);
            }
            fonts.collection.load_system_fonts();
            fonts.collection.make_shared();
            Mutex::new(fonts)
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_contexts_reuse_registered_bundled_font_sources() {
        let mut first = context();
        let mut second = context();
        for name in ["Public Sans Tachyon", "Spline Sans Mono Tachyon"] {
            let a = first.collection.family_by_name(name).unwrap();
            let b = second.collection.family_by_name(name).unwrap();
            assert_eq!(
                a.id(),
                b.id(),
                "fragment contexts must not re-register fonts"
            );
            assert_eq!(a.fonts().len(), b.fonts().len());
            for (a, b) in a.fonts().iter().zip(b.fonts()) {
                let a = first.source_cache.get(a.source()).unwrap();
                let b = second.source_cache.get(b.source()).unwrap();
                assert_eq!(a.id(), b.id(), "bundled font data is shared, not recopied");
            }
        }
    }
}
