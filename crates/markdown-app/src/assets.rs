use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let icon: &'static [u8] = match path {
            "mineral/lightbulb.svg" => include_bytes!("../assets/icons/lightbulb.svg"),
            "mineral/flag.svg" => include_bytes!("../assets/icons/flag.svg"),
            "mineral/bold.svg" => include_bytes!("../assets/icons/bold.svg"),
            "mineral/italic.svg" => include_bytes!("../assets/icons/italic.svg"),
            "mineral/link.svg" => include_bytes!("../assets/icons/link.svg"),
            "mineral/strike.svg" => include_bytes!("../assets/icons/strike.svg"),
            "mineral/code.svg" => include_bytes!("../assets/icons/code.svg"),
            "mineral/row-add.svg" => include_bytes!("../assets/icons/row-add.svg"),
            "mineral/column-add.svg" => include_bytes!("../assets/icons/column-add.svg"),
            "mineral/row-delete.svg" => include_bytes!("../assets/icons/row-delete.svg"),
            "mineral/column-delete.svg" => include_bytes!("../assets/icons/column-delete.svg"),
            "mineral/row-up.svg" => include_bytes!("../assets/icons/row-up.svg"),
            "mineral/row-down.svg" => include_bytes!("../assets/icons/row-down.svg"),
            "mineral/column-left.svg" => include_bytes!("../assets/icons/column-left.svg"),
            "mineral/column-right.svg" => include_bytes!("../assets/icons/column-right.svg"),
            "mineral/align.svg" => include_bytes!("../assets/icons/align.svg"),
            "mineral/border.svg" => include_bytes!("../assets/icons/border.svg"),
            _ => return gpui_component_assets::Assets.load(path),
        };
        Ok(Some(Cow::Borrowed(icon)))
    }
    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut result = gpui_component_assets::Assets.list(path)?;
        for icon in ["mineral/lightbulb.svg", "mineral/flag.svg"] {
            if icon.starts_with(path) {
                result.push(icon.into());
            }
        }
        if "mineral/bold.svg".starts_with(path) {
            result.push("mineral/bold.svg".into());
        }
        if "mineral/italic.svg".starts_with(path) {
            result.push("mineral/italic.svg".into());
        }
        if "mineral/link.svg".starts_with(path) {
            result.push("mineral/link.svg".into());
        }
        if "mineral/strike.svg".starts_with(path) {
            result.push("mineral/strike.svg".into());
        }
        if "mineral/code.svg".starts_with(path) {
            result.push("mineral/code.svg".into());
        }
        if "mineral/row-add.svg".starts_with(path) {
            result.push("mineral/row-add.svg".into());
        }
        if "mineral/column-add.svg".starts_with(path) {
            result.push("mineral/column-add.svg".into());
        }
        if "mineral/row-delete.svg".starts_with(path) {
            result.push("mineral/row-delete.svg".into());
        }
        if "mineral/column-delete.svg".starts_with(path) {
            result.push("mineral/column-delete.svg".into());
        }
        if "mineral/row-up.svg".starts_with(path) {
            result.push("mineral/row-up.svg".into());
        }
        if "mineral/row-down.svg".starts_with(path) {
            result.push("mineral/row-down.svg".into());
        }
        if "mineral/column-left.svg".starts_with(path) {
            result.push("mineral/column-left.svg".into());
        }
        if "mineral/column-right.svg".starts_with(path) {
            result.push("mineral/column-right.svg".into());
        }
        if "mineral/align.svg".starts_with(path) {
            result.push("mineral/align.svg".into());
        }
        if "mineral/border.svg".starts_with(path) {
            result.push("mineral/border.svg".into());
        }
        Ok(result)
    }
}
