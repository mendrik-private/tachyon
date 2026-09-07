//! In-memory image handoff. This module never resolves paths, fetches or decodes.
//! The application image owner must authorize and load resources before layout.
use std::collections::BTreeMap;

use blitz_dom::{
    BaseDocument,
    node::{ImageData, RasterImageData, SpecialElementData},
};
use document_core::InertHtmlFragment;

use super::HtmlError;

pub(crate) type ImageResources = BTreeMap<String, RasterImageData>;
pub(crate) type ImageKey = Vec<(String, u32, u32, u64)>;
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoundImages {
    pub source: std::sync::Arc<str>,
    pub resources: ImageResources,
    pub key: ImageKey,
}
const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;

/// Validate complete source-bound coverage before any DOM work. A missing or
/// invalid image means full source fallback, never a partial/broken picture.
/// Keys retain blob identity, not pixel buffers, in the cross-document cache.
pub(super) fn key(
    fragment: &InertHtmlFragment,
    resources: &ImageResources,
) -> Result<ImageKey, HtmlError> {
    let mut bytes = 0usize;
    let mut key = Vec::with_capacity(fragment.images().len());
    for reference in fragment.images() {
        let image = resources
            .get(&reference.source)
            .ok_or(HtmlError::Unsupported)?;
        let size = (image.width as usize)
            .checked_mul(image.height as usize)
            .and_then(|size| size.checked_mul(4))
            .ok_or(HtmlError::TooLarge)?;
        bytes = bytes.checked_add(size).ok_or(HtmlError::TooLarge)?;
        if image.width == 0
            || image.height == 0
            || image.width > 16384
            || image.height > 16384
            || size != image.data.data().len()
            || bytes > MAX_RESOURCE_BYTES
        {
            return Err(HtmlError::TooLarge);
        }
        key.push((
            reference.source.clone(),
            image.width,
            image.height,
            image.data.id(),
        ));
    }
    Ok(key)
}

pub(super) fn install(
    doc: &mut BaseDocument,
    fragment: &InertHtmlFragment,
    resources: &ImageResources,
) -> Result<(), HtmlError> {
    key(fragment, resources)?;
    let mut stack = vec![doc.root_node().id];
    let mut seen = vec![false; fragment.images().len()];
    while let Some(id) = stack.pop() {
        let node = doc.get_node_mut(id).ok_or(HtmlError::Unsupported)?;
        stack.extend(node.children.iter().rev().copied());
        let Some(element) = node.element_data_mut() else {
            continue;
        };
        if element.name.local.as_ref() != "img" {
            continue;
        }
        let ordinal = element
            .attr(blitz_dom::LocalName::from("data-mineral-image"))
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or(HtmlError::Unsupported)?;
        let found = seen.get_mut(ordinal).ok_or(HtmlError::Unsupported)?;
        if std::mem::replace(found, true) {
            return Err(HtmlError::Unsupported);
        }
        let reference = &fragment.images()[ordinal];
        let image = resources
            .get(&reference.source)
            .ok_or(HtmlError::Unsupported)?;
        element.special_data =
            SpecialElementData::Image(Box::new(ImageData::Raster(image.clone())));
    }
    if seen.iter().any(|found| !found) {
        return Err(HtmlError::Unsupported);
    }
    Ok(())
}
