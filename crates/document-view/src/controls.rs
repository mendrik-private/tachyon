//! Accessibility metadata for the pinned component button's existing node.
use gpui::{InteractiveElement, Interactivity, SharedString, StatefulInteractiveElement};
use gpui_component::Disableable as _;
use gpui_component::button::Button;

/// The pinned `Button` delegates interactivity but does not implement GPUI's
/// stateful builder trait. It already owns a stable element ID. This temporary
/// adapter adds metadata to that same node, without another wrapper in either
/// the rendered or accessible tree.
struct StatefulButton(Button);

impl InteractiveElement for StatefulButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.0.interactivity()
    }
}

impl StatefulInteractiveElement for StatefulButton {}

/// Nonvisual metadata for icon controls. A visible `Button::label` takes
/// precedence in the pinned toolkit; use child text for a distinct action name.
pub trait ButtonAccessibilityExt: Sized {
    /// Disable toolkit input and publish the same native accessibility state.
    fn accessible_disabled(self, disabled: bool) -> Self;
    fn accessible_name(self, label: impl Into<SharedString>) -> Self;
    fn accessible_description(self, description: impl Into<SharedString>) -> Self;
    fn accessible_shortcut(self, shortcut: impl Into<SharedString>) -> Self;
}

impl ButtonAccessibilityExt for Button {
    fn accessible_disabled(self, disabled: bool) -> Self {
        StatefulButton(self.disabled(disabled))
            .aria_disabled(disabled)
            .0
    }
    fn accessible_name(self, label: impl Into<SharedString>) -> Self {
        StatefulButton(self).aria_label(label).0
    }

    fn accessible_description(self, description: impl Into<SharedString>) -> Self {
        StatefulButton(self).aria_description(description).0
    }

    fn accessible_shortcut(self, shortcut: impl Into<SharedString>) -> Self {
        StatefulButton(self).aria_keyshortcuts(shortcut).0
    }
}
