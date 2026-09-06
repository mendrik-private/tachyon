#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowClass {
    Compact,
    Medium,
    Wide,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsiveLayout {
    pub class: WindowClass,
    pub navigation_overlay: bool,
    pub minimap_visible: bool,
    pub navigation_width: f32,
    pub minimum_document_padding: f32,
}

impl ResponsiveLayout {
    #[must_use]
    pub fn for_width(window_width: f32, requested_navigation_width: f32) -> Self {
        let navigation_width = requested_navigation_width.clamp(180.0, 320.0);
        if window_width < 800.0 {
            Self {
                class: WindowClass::Compact,
                navigation_overlay: true,
                minimap_visible: false,
                navigation_width,
                minimum_document_padding: 32.0,
            }
        } else if window_width < 1000.0 {
            Self {
                class: WindowClass::Medium,
                navigation_overlay: false,
                minimap_visible: false,
                navigation_width,
                minimum_document_padding: 32.0,
            }
        } else {
            Self {
                class: WindowClass::Wide,
                navigation_overlay: false,
                minimap_visible: true,
                navigation_width,
                minimum_document_padding: 32.0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_and_sidebar_constraints_match_contract() {
        assert_eq!(
            ResponsiveLayout::for_width(799.0, 100.0).class,
            WindowClass::Compact
        );
        assert_eq!(
            ResponsiveLayout::for_width(800.0, 500.0).class,
            WindowClass::Medium
        );
        let wide = ResponsiveLayout::for_width(1000.0, 224.0);
        assert_eq!(wide.class, WindowClass::Wide);
        assert!(wide.minimap_visible);
        assert_eq!(wide.navigation_width, 224.0);
    }
}
