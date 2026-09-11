//! Authored figure text stays on its canonical paragraph nodes. These roles
//! describe adjacency, not a second editable tree or generated Markdown.
use document_core::{BlockNode, NodeId};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FigureTextRole {
    Caption,
    Credit,
    GalleryCaption { first: NodeId },
    GalleryCredit { first: NodeId },
}

impl FigureTextRole {
    pub(crate) fn gap(self) -> f32 {
        match self {
            Self::Caption | Self::GalleryCaption { .. } => 8.,
            Self::Credit | Self::GalleryCredit { .. } => 4.,
        }
    }

    pub(crate) fn gallery_start(self) -> Option<NodeId> {
        match self {
            Self::GalleryCaption { first } | Self::GalleryCredit { first } => Some(first),
            _ => None,
        }
    }

    pub(crate) fn is_caption(self) -> bool {
        matches!(self, Self::Caption | Self::GalleryCaption { .. })
    }
}

pub(crate) fn classify(block: &BlockNode) -> Option<FigureTextRole> {
    let BlockNode::Paragraph(p) = block else {
        return None;
    };
    // Only an authored label establishes this convention. Italics, brevity,
    // image alt text and image titles alone are not captions.
    classify_text(&p.content.as_cow())
}

fn classify_text(text: &str) -> Option<FigureTextRole> {
    let text = text.trim_start();
    for prefix in ["Credit:", "Credits:", "Photo credit:", "Image credit:"] {
        if text
            .strip_prefix(prefix)
            .is_some_and(|s| !s.trim().is_empty())
        {
            return Some(FigureTextRole::Credit);
        }
    }
    for prefix in ["Caption:", "Figure:", "Photo:", "Illustration:"] {
        if text
            .strip_prefix(prefix)
            .is_some_and(|s| !s.trim().is_empty())
        {
            return Some(FigureTextRole::Caption);
        }
    }
    for prefix in ["Figure ", "Fig. ", "Photo ", "Illustration "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let (label, body) = rest.split_once(' ')?;
            let number = label.strip_suffix(['.', ':'])?;
            if !body.trim().is_empty()
                && number.starts_with(|c: char| c.is_ascii_digit())
                && number
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.')
            {
                return Some(FigureTextRole::Caption);
            }
        }
    }
    None
}

/// One image, optional explicit caption, optional credit; never cross another
/// source block. Galleries use each complete range as their indivisible unit.
pub(crate) fn end(roots: &[&BlockNode], image: usize) -> usize {
    let mut end = image + 1;
    if !matches!(roots.get(image), Some(BlockNode::Image(_))) {
        return end;
    }
    if roots.get(end).and_then(|b| classify(b)) == Some(FigureTextRole::Caption) {
        end += 1;
    }
    if roots.get(end).and_then(|b| classify(b)) == Some(FigureTextRole::Credit) {
        end += 1;
    }
    end
}

pub(crate) fn associations(roots: &[&BlockNode]) -> FxHashMap<NodeId, (NodeId, FigureTextRole)> {
    let mut result = FxHashMap::default();
    let mut first = None;
    let mut count = 0;
    let mut previous_end = 0;
    for (i, root) in roots.iter().enumerate() {
        if !matches!(root, BlockNode::Image(_)) {
            continue;
        }
        if i != previous_end {
            first = None;
            count = 0;
        }
        first.get_or_insert(root.id());
        count += 1;
        previous_end = end(roots, i);
        for label in &roots[i + 1..previous_end] {
            result.insert(label.id(), (root.id(), classify(label).unwrap()));
        }
        // Bound one source-adjacent gallery, just as row planning does. Never
        // infer shared text from an image's alt/title or an ordinary paragraph.
        if (2..=9).contains(&count)
            && let Some(label) = roots.get(previous_end)
            && has_label(label, "Gallery:")
        {
            let first = first.unwrap();
            result.insert(
                label.id(),
                (root.id(), FigureTextRole::GalleryCaption { first }),
            );
            if let Some(credit) = roots.get(previous_end + 1)
                && has_label(credit, "Gallery credit:")
            {
                result.insert(
                    credit.id(),
                    (root.id(), FigureTextRole::GalleryCredit { first }),
                );
            }
        }
    }
    result
}

fn has_label(block: &BlockNode, label: &str) -> bool {
    matches!(block, BlockNode::Paragraph(p) if p.content.as_cow().trim_start()
        .strip_prefix(label).is_some_and(|body| !body.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_gallery_labels_attach_to_the_complete_adjacent_set() {
        let source = "![A](a.png)\n\nCaption: Individual A\n\n![B](b.png)\n\nGallery: Two views of one specimen\n\nGallery credit: Field notebook\n\nOrdinary prose\n";
        let document = document_core::Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let roots = snapshot
            .blocks()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        let roles = associations(&roots);
        assert_eq!(
            roles.len(),
            3,
            "individual caption plus shared caption and credit"
        );
        assert_eq!(roles[&roots[3].id()].0, roots[2].id());
        assert_eq!(roles[&roots[4].id()].0, roots[2].id());
        assert_eq!(roles[&roots[3].id()].1.gallery_start(), Some(roots[0].id()));
        assert_eq!(snapshot.serialize().unwrap(), source);
    }

    #[test]
    fn shared_gallery_labels_require_a_bounded_uninterrupted_set() {
        for source in [
            "![A](a.png)\n\nGallery: Just one\n".to_owned(),
            "![A](a.png)\n\nOrdinary prose\n\n![B](b.png)\n\nGallery: Unrelated\n".to_owned(),
            "![A](a.png)\n\n## New section\n\n![B](b.png)\n\nGallery: Unrelated\n".to_owned(),
            "![A](a.png)\n\n![B](b.png)\n\nGallery:\n".to_owned(),
            format!(
                "{}Gallery: Too many for one bounded group\n",
                "![A](a.png)\n\n".repeat(10)
            ),
        ] {
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let roots = snapshot
                .blocks()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>();
            assert!(associations(&roots).is_empty(), "{source}");
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
    }

    #[test]
    fn requires_explicit_labels_not_italic_prose_or_alt_text() {
        for text in [
            "Figure 1. A branch",
            "Fig. 2a: A leaf",
            "Caption: A quiet morning",
        ] {
            assert_eq!(classify_text(text), Some(FigureTextRole::Caption));
        }
        for text in [
            "Figure out the answer.",
            "Photo opportunities are limited.",
            "Figure 1",
            "Caption:",
            "A quiet morning",
            "Figure x. A branch",
        ] {
            assert_eq!(classify_text(text), None, "{text}");
        }
        let source = "![An alt description](a.png)\n\n*Figure 1. A branch.*\n\nCredit: Field notebook\n\nAn ordinary paragraph.\n\nCredit: This is not attached.\n";
        let document = document_core::Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let roots = snapshot
            .blocks()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        let roles = associations(&roots);
        assert_eq!(roles.len(), 2);
        assert_eq!(
            roles[&roots[1].id()],
            (roots[0].id(), FigureTextRole::Caption)
        );
        assert_eq!(
            roles[&roots[2].id()],
            (roots[0].id(), FigureTextRole::Credit)
        );
        assert_eq!(snapshot.serialize().unwrap(), source);
    }
}
