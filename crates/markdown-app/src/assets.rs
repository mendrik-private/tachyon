use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let icon: &'static [u8] = match path {
            "tachyon/lightbulb.svg" => include_bytes!("../assets/icons/lightbulb.svg"),
            "tachyon/flag.svg" => include_bytes!("../assets/icons/flag.svg"),
            "tachyon/bold.svg" => include_bytes!("../assets/icons/bold.svg"),
            "tachyon/italic.svg" => include_bytes!("../assets/icons/italic.svg"),
            "tachyon/link.svg" => include_bytes!("../assets/icons/link.svg"),
            "tachyon/strike.svg" => include_bytes!("../assets/icons/strike.svg"),
            "tachyon/code.svg" => include_bytes!("../assets/icons/code.svg"),
            "tachyon/row-add.svg" => include_bytes!("../assets/icons/row-add.svg"),
            "tachyon/column-add.svg" => include_bytes!("../assets/icons/column-add.svg"),
            "tachyon/row-delete.svg" => include_bytes!("../assets/icons/row-delete.svg"),
            "tachyon/column-delete.svg" => include_bytes!("../assets/icons/column-delete.svg"),
            "tachyon/row-up.svg" => include_bytes!("../assets/icons/row-up.svg"),
            "tachyon/row-down.svg" => include_bytes!("../assets/icons/row-down.svg"),
            "tachyon/column-left.svg" => include_bytes!("../assets/icons/column-left.svg"),
            "tachyon/column-right.svg" => include_bytes!("../assets/icons/column-right.svg"),
            "tachyon/align.svg" => include_bytes!("../assets/icons/align.svg"),
            "tachyon/border.svg" => include_bytes!("../assets/icons/border.svg"),
            _ => return gpui_component_assets::Assets.load(path),
        };
        Ok(Some(Cow::Borrowed(icon)))
    }
    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut result = gpui_component_assets::Assets.list(path)?;
        for icon in ["tachyon/lightbulb.svg", "tachyon/flag.svg"] {
            if icon.starts_with(path) {
                result.push(icon.into());
            }
        }
        if "tachyon/bold.svg".starts_with(path) {
            result.push("tachyon/bold.svg".into());
        }
        if "tachyon/italic.svg".starts_with(path) {
            result.push("tachyon/italic.svg".into());
        }
        if "tachyon/link.svg".starts_with(path) {
            result.push("tachyon/link.svg".into());
        }
        if "tachyon/strike.svg".starts_with(path) {
            result.push("tachyon/strike.svg".into());
        }
        if "tachyon/code.svg".starts_with(path) {
            result.push("tachyon/code.svg".into());
        }
        if "tachyon/row-add.svg".starts_with(path) {
            result.push("tachyon/row-add.svg".into());
        }
        if "tachyon/column-add.svg".starts_with(path) {
            result.push("tachyon/column-add.svg".into());
        }
        if "tachyon/row-delete.svg".starts_with(path) {
            result.push("tachyon/row-delete.svg".into());
        }
        if "tachyon/column-delete.svg".starts_with(path) {
            result.push("tachyon/column-delete.svg".into());
        }
        if "tachyon/row-up.svg".starts_with(path) {
            result.push("tachyon/row-up.svg".into());
        }
        if "tachyon/row-down.svg".starts_with(path) {
            result.push("tachyon/row-down.svg".into());
        }
        if "tachyon/column-left.svg".starts_with(path) {
            result.push("tachyon/column-left.svg".into());
        }
        if "tachyon/column-right.svg".starts_with(path) {
            result.push("tachyon/column-right.svg".into());
        }
        if "tachyon/align.svg".starts_with(path) {
            result.push("tachyon/align.svg".into());
        }
        if "tachyon/border.svg".starts_with(path) {
            result.push("tachyon/border.svg".into());
        }
        Ok(result)
    }
}
