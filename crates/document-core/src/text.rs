use std::{borrow::Cow, ops::Range, sync::Arc};

use ropey::Rope;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::{DocumentError, NodeId, PositionError};

pub type TextRange = Range<usize>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InlineStyle {
    Bold,
    Italic,
    Strikethrough,
    Code,
    /// TeX source, without its dollar delimiters. Display spans can also occur
    /// inside a paragraph; standalone display spans become literal blocks.
    Math {
        display: bool,
    },
    Link(crate::LinkTarget),
    Image {
        source: String,
        alt: String,
        title: Option<String>,
    },
    FootnoteReference(String),
    PreservedHtml(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineRun {
    pub range: TextRange,
    pub styles: SmallVec<[InlineStyle; 3]>,
}

#[derive(Clone, Debug)]
pub struct RichText {
    text: TextStorage,
    runs: Vec<InlineRun>,
}

/// Imported prose is overwhelmingly made of short immutable leaves. Keeping
/// those leaves contiguous avoids constructing a Rope tree for every heading,
/// paragraph, list item, and table cell during open. Large leaves start as a
/// Rope so editing a giant paragraph stays bounded; any textual mutation of a
/// small leaf promotes it to a Rope before applying the edit.
#[derive(Clone, Debug)]
enum TextStorage {
    Contiguous(Arc<String>),
    Rope(Rope),
}

const EAGER_ROPE_BYTES: usize = 8 * 1024;

impl TextStorage {
    fn new(text: String) -> Self {
        if text.len() >= EAGER_ROPE_BYTES {
            Self::Rope(Rope::from(text.as_str()))
        } else {
            Self::Contiguous(Arc::new(text))
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Contiguous(text) => text.len(),
            Self::Rope(text) => text.len(),
        }
    }

    fn as_cow(&self) -> Cow<'_, str> {
        match self {
            Self::Contiguous(text) => Cow::Borrowed(text.as_str()),
            Self::Rope(text) => Cow::Owned(text.to_string()),
        }
    }

    fn append_to(&self, output: &mut String) {
        match self {
            Self::Contiguous(text) => output.push_str(text),
            Self::Rope(text) => {
                for chunk in text.chunks() {
                    output.push_str(chunk);
                }
            }
        }
    }

    fn promote(&mut self) -> &mut Rope {
        if let Self::Contiguous(text) = self {
            *self = Self::Rope(Rope::from(text.as_str()));
        }
        let Self::Rope(text) = self else {
            unreachable!("contiguous text was promoted")
        };
        text
    }
}

impl Default for RichText {
    fn default() -> Self {
        Self::new("")
    }
}

impl RichText {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineRun {
                range: 0..text.len(),
                styles: SmallVec::new(),
            }]
        };
        Self {
            text: TextStorage::new(text),
            runs,
        }
    }

    #[must_use]
    pub fn from_runs(text: impl Into<String>, mut runs: Vec<InlineRun>) -> Self {
        let text = text.into();
        normalize_runs(&mut runs, text.len());
        Self {
            text: TextStorage::new(text),
            runs,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }

    #[must_use]
    pub fn as_string(&self) -> String {
        self.as_cow().into_owned()
    }

    #[must_use]
    pub fn as_cow(&self) -> Cow<'_, str> {
        self.text.as_cow()
    }

    pub fn append_to(&self, output: &mut String) {
        self.text.append_to(output);
    }

    #[must_use]
    pub fn runs(&self) -> &[InlineRun] {
        &self.runs
    }

    pub fn validate_range(&self, node: NodeId, range: &TextRange) -> Result<(), PositionError> {
        if range.start > range.end || range.end > self.len() {
            return Err(PositionError::OffsetOutOfBounds {
                node,
                offset: range.end.max(range.start),
                len: self.len(),
            });
        }
        let text = self.as_string();
        if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            return Err(PositionError::InvalidTextRange {
                node,
                range: range.clone(),
            });
        }
        Ok(())
    }

    pub(crate) fn replace(
        &mut self,
        node: NodeId,
        range: TextRange,
        replacement: &str,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &range)?;
        let old_len = range.end - range.start;
        let replacement_len = replacement.len();
        let inherited = self.styles_at_insertion(range.start);

        split_run_at(&mut self.runs, range.start);
        split_run_at(&mut self.runs, range.end);

        let mut next = Vec::with_capacity(self.runs.len() + usize::from(!replacement.is_empty()));
        for run in self.runs.drain(..) {
            if run.range.end <= range.start {
                next.push(run);
            } else if run.range.start >= range.end {
                let start = run.range.start - old_len + replacement_len;
                let end = run.range.end - old_len + replacement_len;
                next.push(InlineRun {
                    range: start..end,
                    styles: run.styles,
                });
            }
        }
        if !replacement.is_empty() {
            next.push(InlineRun {
                range: range.start..range.start + replacement_len,
                styles: inherited,
            });
        }
        next.sort_by_key(|run| run.range.start);
        let insertion = range.start;
        let text = self.text.promote();
        text.remove(range);
        text.insert(insertion, replacement);
        normalize_runs(&mut next, self.text.len());
        self.runs = next;
        Ok(())
    }

    pub(crate) fn replace_rich(
        &mut self,
        node: NodeId,
        range: TextRange,
        replacement: &Self,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &range)?;
        *self = Self::concatenate([
            self.slice(0..range.start),
            replacement.clone(),
            self.slice(range.end..self.len()),
        ]);
        Ok(())
    }

    pub(crate) fn replace_tail_with_text_and_suffix(
        &mut self,
        node: NodeId,
        start: usize,
        replacement: &str,
        suffix: &RichText,
        suffix_start: usize,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &(start..self.len()))?;
        suffix.validate_range(node, &(suffix_start..suffix.len()))?;

        let own = self.as_string();
        let suffix_text = suffix.as_string();
        let mut combined = String::with_capacity(
            start + replacement.len() + suffix_text.len().saturating_sub(suffix_start),
        );
        combined.push_str(&own[..start]);
        combined.push_str(replacement);
        combined.push_str(&suffix_text[suffix_start..]);

        let mut runs = Vec::new();
        for run in &self.runs {
            let end = run.range.end.min(start);
            if run.range.start < end {
                runs.push(InlineRun {
                    range: run.range.start..end,
                    styles: run.styles.clone(),
                });
            }
        }
        if !replacement.is_empty() {
            runs.push(InlineRun {
                range: start..start + replacement.len(),
                styles: self.styles_at_insertion(start),
            });
        }
        let suffix_destination = start + replacement.len();
        for run in &suffix.runs {
            let clipped_start = run.range.start.max(suffix_start);
            if clipped_start < run.range.end {
                runs.push(InlineRun {
                    range: suffix_destination + clipped_start - suffix_start
                        ..suffix_destination + run.range.end - suffix_start,
                    styles: run.styles.clone(),
                });
            }
        }
        *self = Self::from_runs(combined, runs);
        Ok(())
    }

    pub(crate) fn replace_tail_with_rich_and_suffix(
        &mut self,
        node: NodeId,
        start: usize,
        replacement: &Self,
        suffix: &Self,
        suffix_start: usize,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &(start..self.len()))?;
        suffix.validate_range(node, &(suffix_start..suffix.len()))?;
        *self = Self::concatenate([
            self.slice(0..start),
            replacement.clone(),
            suffix.slice(suffix_start..suffix.len()),
        ]);
        Ok(())
    }

    pub(crate) fn split_at(
        &self,
        node: NodeId,
        offset: usize,
    ) -> Result<(Self, Self), DocumentError> {
        self.validate_range(node, &(offset..offset))?;
        Ok((self.slice(0..offset), self.slice(offset..self.len())))
    }

    pub(crate) fn set_style(
        &mut self,
        node: NodeId,
        range: TextRange,
        style: InlineStyle,
        enabled: bool,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &range)?;
        if range.is_empty() {
            return Ok(());
        }
        split_run_at(&mut self.runs, range.start);
        split_run_at(&mut self.runs, range.end);
        for run in self
            .runs
            .iter_mut()
            .filter(|run| run.range.start >= range.start && run.range.end <= range.end)
        {
            let existing = run.styles.iter().position(|candidate| candidate == &style);
            match (enabled, existing) {
                (true, None) => run.styles.push(style.clone()),
                (false, Some(index)) => {
                    run.styles.remove(index);
                }
                _ => {}
            }
            run.styles.sort_by(style_order);
        }
        normalize_runs(&mut self.runs, self.text.len());
        Ok(())
    }

    pub(crate) fn set_link(
        &mut self,
        node: NodeId,
        range: TextRange,
        target: Option<&str>,
    ) -> Result<(), DocumentError> {
        self.validate_range(node, &range)?;
        if range.is_empty() {
            return Ok(());
        }
        split_run_at(&mut self.runs, range.start);
        split_run_at(&mut self.runs, range.end);
        for run in self
            .runs
            .iter_mut()
            .filter(|run| run.range.start >= range.start && run.range.end <= range.end)
        {
            run.styles
                .retain(|style| !matches!(style, InlineStyle::Link(_)));
            if let Some(target) = target.filter(|target| !target.is_empty()) {
                run.styles
                    .push(InlineStyle::Link(crate::LinkTarget(target.to_owned())));
            }
            run.styles.sort_by(style_order);
        }
        normalize_runs(&mut self.runs, self.text.len());
        Ok(())
    }

    #[must_use]
    pub fn utf16_offset_for_byte(&self, byte_offset: usize) -> Option<usize> {
        let text = self.as_string();
        text.is_char_boundary(byte_offset)
            .then(|| text[..byte_offset].encode_utf16().count())
    }

    #[must_use]
    pub fn byte_offset_for_utf16(&self, utf16_offset: usize) -> usize {
        let text = self.as_string();
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            if units >= utf16_offset {
                return byte;
            }
            let next = units + character.len_utf16();
            if next > utf16_offset {
                return byte;
            }
            units = next;
        }
        text.len()
    }

    #[must_use]
    pub fn previous_grapheme_boundary(&self, byte_offset: usize) -> usize {
        let text = self.as_string();
        text.grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < byte_offset).then_some(index))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn next_grapheme_boundary(&self, byte_offset: usize) -> usize {
        let text = self.as_string();
        text.grapheme_indices(true)
            .find_map(|(index, _)| (index > byte_offset).then_some(index))
            .unwrap_or(text.len())
    }

    #[must_use]
    pub(crate) fn style_coverage(&self, range: &TextRange, style: &InlineStyle) -> (usize, usize) {
        let mut covered = 0;
        for run in &self.runs {
            let start = run.range.start.max(range.start);
            let end = run.range.end.min(range.end);
            if start < end && run.styles.contains(style) {
                covered += end - start;
            }
        }
        (covered, range.end.saturating_sub(range.start))
    }

    fn styles_at_insertion(&self, offset: usize) -> SmallVec<[InlineStyle; 3]> {
        self.runs
            .iter()
            .find(|run| {
                run.range.contains(&offset)
                    || (offset == self.len() && run.range.end == self.len())
                    || (offset > 0 && run.range.end == offset)
            })
            .map_or_else(SmallVec::new, |run| {
                let mut styles = run.styles.clone();
                // A note reference is an inline object, not a text format.
                // Typing beside it must not extend a run whose serializer emits
                // only the reference label (which would discard the new text).
                if offset == run.range.start || offset == run.range.end {
                    styles.retain(|style| !matches!(style, InlineStyle::FootnoteReference(_)));
                }
                styles
            })
    }

    pub(crate) fn slice(&self, range: Range<usize>) -> Self {
        let source = self.as_string();
        let runs = self
            .runs
            .iter()
            .filter_map(|run| {
                let overlap = run.range.start.max(range.start)..run.range.end.min(range.end);
                (overlap.start < overlap.end).then(|| InlineRun {
                    range: overlap.start - range.start..overlap.end - range.start,
                    styles: run.styles.clone(),
                })
            })
            .collect();
        Self::from_runs(&source[range], runs)
    }

    fn concatenate<const N: usize>(parts: [Self; N]) -> Self {
        let capacity = parts.iter().map(Self::len).sum();
        let mut text = String::with_capacity(capacity);
        let mut runs = Vec::new();
        for part in parts {
            let offset = text.len();
            part.append_to(&mut text);
            runs.extend(part.runs.into_iter().map(|run| InlineRun {
                range: run.range.start + offset..run.range.end + offset,
                styles: run.styles,
            }));
        }
        Self::from_runs(text, runs)
    }
}

fn split_run_at(runs: &mut Vec<InlineRun>, offset: usize) {
    let Some(index) = runs
        .iter()
        .position(|run| run.range.start < offset && offset < run.range.end)
    else {
        return;
    };
    let run = runs.remove(index);
    let right_styles = run.styles.clone();
    runs.insert(
        index,
        InlineRun {
            range: run.range.start..offset,
            styles: run.styles,
        },
    );
    runs.insert(
        index + 1,
        InlineRun {
            range: offset..run.range.end,
            styles: right_styles,
        },
    );
}

fn normalize_runs(runs: &mut Vec<InlineRun>, text_len: usize) {
    runs.retain(|run| run.range.start < run.range.end && run.range.end <= text_len);
    runs.sort_by_key(|run| run.range.start);
    let mut normalized = Vec::<InlineRun>::new();
    let mut cursor = 0;
    for run in runs.drain(..) {
        if cursor < run.range.start {
            normalized.push(InlineRun {
                range: cursor..run.range.start,
                styles: SmallVec::new(),
            });
        }
        if let Some(previous) = normalized.last_mut()
            && previous.range.end == run.range.start
            && previous.styles == run.styles
            && !run
                .styles
                .iter()
                .any(|style| matches!(style, InlineStyle::FootnoteReference(_)))
        {
            previous.range.end = run.range.end;
        } else {
            normalized.push(run);
        }
        cursor = normalized.last().map_or(cursor, |run| run.range.end);
    }
    if cursor < text_len {
        normalized.push(InlineRun {
            range: cursor..text_len,
            styles: smallvec![],
        });
    }
    if text_len == 0 {
        normalized.clear();
    }
    *runs = normalized;
}

fn style_order(left: &InlineStyle, right: &InlineStyle) -> std::cmp::Ordering {
    fn rank(style: &InlineStyle) -> u8 {
        match style {
            InlineStyle::Bold => 0,
            InlineStyle::Italic => 1,
            InlineStyle::Strikethrough => 2,
            InlineStyle::Code => 3,
            InlineStyle::Link(_) => 4,
            InlineStyle::Image { .. } => 5,
            InlineStyle::FootnoteReference(_) => 6,
            InlineStyle::PreservedHtml(_) => 7,
            InlineStyle::Math { .. } => 8,
        }
    }
    rank(left).cmp(&rank(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE: NodeId = NodeId::new_unchecked(1);

    #[test]
    fn adjacent_footnotes_remain_distinct_and_edge_typing_is_not_reference_formatting() {
        let mut text = RichText::from_runs(
            "[^n][^n]",
            vec![
                InlineRun {
                    range: 0..4,
                    styles: smallvec![InlineStyle::FootnoteReference("n".into())],
                },
                InlineRun {
                    range: 4..8,
                    styles: smallvec![InlineStyle::FootnoteReference("n".into())],
                },
            ],
        );
        assert_eq!(text.runs().len(), 2);
        text.replace(NODE, 8..8, " next").unwrap();
        assert!(text.runs().last().unwrap().styles.is_empty());
        assert_eq!(crate::markdown::serialize_inline(&text), "[^n][^n] next");
        text.replace(NODE, 0..0, "Before ").unwrap();
        assert_eq!(
            crate::markdown::serialize_inline(&text),
            "Before [^n][^n] next"
        );
    }

    #[test]
    fn short_imported_text_is_shared_then_promoted_before_mutation() {
        let mut text = RichText::new("shared leaf");
        let clone = text.clone();
        let (TextStorage::Contiguous(before), TextStorage::Contiguous(shared)) =
            (&text.text, &clone.text)
        else {
            panic!("short text should stay contiguous before editing");
        };
        assert!(Arc::ptr_eq(before, shared));

        text.replace(NODE, 6..6, " edited").expect("valid edit");
        assert!(matches!(text.text, TextStorage::Rope(_)));
        assert_eq!(text.as_string(), "shared edited leaf");
        assert_eq!(clone.as_string(), "shared leaf");
    }

    #[test]
    fn giant_text_is_rope_backed_from_construction() {
        let text = RichText::new("x".repeat(EAGER_ROPE_BYTES));
        assert!(matches!(text.text, TextStorage::Rope(_)));
    }

    #[test]
    fn replacement_preserves_style_on_both_sides() {
        let mut text = RichText::new("hello world");
        text.set_style(NODE, 0..5, InlineStyle::Bold, true)
            .expect("valid style range");
        text.replace(NODE, 1..4, "i").expect("valid edit");
        assert_eq!(text.as_string(), "hio world");
        assert_eq!(text.runs()[0].range, 0..3);
        assert!(text.runs()[0].styles.contains(&InlineStyle::Bold));
    }

    #[test]
    fn utf16_conversion_clamps_inside_surrogate_pairs() {
        let text = RichText::new("a🎉中");
        assert_eq!(text.utf16_offset_for_byte("a🎉".len()), Some(3));
        assert_eq!(text.byte_offset_for_utf16(2), 1);
        assert_eq!(text.byte_offset_for_utf16(3), "a🎉".len());
    }

    #[test]
    fn movement_uses_grapheme_boundaries() {
        let text = RichText::new("a\u{301}👨‍👩‍👧‍👦z");
        let first = "a\u{301}".len();
        let family = "a\u{301}👨‍👩‍👧‍👦".len();
        assert_eq!(text.next_grapheme_boundary(0), first);
        assert_eq!(text.next_grapheme_boundary(first), family);
        assert_eq!(text.previous_grapheme_boundary(family), first);
    }

    #[test]
    fn cross_block_join_preserves_prefix_and_suffix_styles() {
        let mut first = RichText::new("bold tail");
        first
            .set_style(NODE, 0..4, InlineStyle::Bold, true)
            .expect("style");
        let mut second = RichText::new("drop italic");
        second
            .set_style(NODE, 5..11, InlineStyle::Italic, true)
            .expect("style");
        first
            .replace_tail_with_text_and_suffix(NODE, 4, " + ", &second, 5)
            .expect("join");
        assert_eq!(first.as_string(), "bold + italic");
        assert!(first.runs()[0].styles.contains(&InlineStyle::Bold));
        assert!(
            first
                .runs()
                .last()
                .expect("suffix")
                .styles
                .contains(&InlineStyle::Italic)
        );
    }
}
