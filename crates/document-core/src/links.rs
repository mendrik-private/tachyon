//! Read-only link resolution. No filesystem access, resource loading, or shell
//! dispatch: the application owns opening documents and external URLs.
use std::path::{Path, PathBuf};

use comrak::Anchorizer;
use percent_encoding::percent_decode_str;
use url::Url;

use crate::{BlockNode, BlockSequence, NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkDestination {
    Heading(String),
    Document {
        path: PathBuf,
        fragment: Option<String>,
    },
    External(String),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("This link is not a valid address.")]
    Invalid,
    #[error("Save this document before following a relative file link.")]
    NoDirectory,
    #[error("Only web, email, heading, and local Markdown links can be opened.")]
    Unsupported,
}

/// Resolve authored links without probing their targets. Local paths are URL
/// references (percent escapes and dot segments), not shell commands. Only
/// Markdown files may enter the native document loader.
pub fn resolve_link(target: &str, directory: Option<&Path>) -> Result<LinkDestination, LinkError> {
    if target.is_empty() || target.chars().any(char::is_control) {
        return Err(LinkError::Invalid);
    }
    if let Some(fragment) = target.strip_prefix('#') {
        return Ok(LinkDestination::Heading(decode_fragment(fragment)?));
    }
    // Do not let URL normalization turn network-path references, backslashes,
    // or leading whitespace into a different class of destination.
    if target.starts_with("//") || target.contains('\\') || target.trim() != target {
        return Err(LinkError::Invalid);
    }
    let url = match Url::parse(target) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let base = Url::from_directory_path(directory.ok_or(LinkError::NoDirectory)?)
                .map_err(|_| LinkError::Invalid)?;
            base.join(target).map_err(|_| LinkError::Invalid)?
        }
        Err(_) => return Err(LinkError::Invalid),
    };
    match url.scheme() {
        "http" | "https" | "mailto" => {
            if target.chars().any(char::is_whitespace)
                || url.path().is_empty() && url.host().is_none()
            {
                return Err(LinkError::Invalid);
            }
            Ok(LinkDestination::External(url.into()))
        }
        "file" => {
            if url.host_str().is_some() || url.query().is_some() {
                return Err(LinkError::Unsupported);
            }
            let path = url.to_file_path().map_err(|_| LinkError::Invalid)?;
            if !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
            {
                return Err(LinkError::Unsupported);
            }
            if path.as_os_str().as_encoded_bytes().contains(&0) {
                return Err(LinkError::Invalid);
            }
            let fragment = url.fragment().map(decode_fragment).transpose()?;
            Ok(LinkDestination::Document { path, fragment })
        }
        _ => Err(LinkError::Unsupported),
    }
}

fn decode_fragment(fragment: &str) -> Result<String, LinkError> {
    let decoded = percent_decode_str(fragment)
        .decode_utf8()
        .map_err(|_| LinkError::Invalid)?;
    if decoded.chars().any(char::is_control) {
        return Err(LinkError::Invalid);
    }
    Ok(decoded.into_owned())
}

/// Resolve the exact decoded GFM anchor, including duplicate suffixes, against
/// canonical headings in source order. HTML IDs remain opaque, not invented.
#[must_use]
pub fn heading_node(blocks: &BlockSequence, fragment: &str) -> Option<NodeId> {
    fn visit(blocks: &BlockSequence, fragment: &str, anchors: &mut Anchorizer) -> Option<NodeId> {
        for block in blocks {
            let found = match block.as_ref() {
                BlockNode::Heading(heading) => (anchors.anchorize(&heading.content.as_string())
                    == fragment)
                    .then_some(heading.id),
                BlockNode::List(list) => list
                    .items
                    .iter()
                    .find_map(|item| visit(&item.blocks, fragment, anchors)),
                BlockNode::BlockQuote { blocks, .. }
                | BlockNode::Alert { blocks, .. }
                | BlockNode::Definition { blocks, .. }
                | BlockNode::FootnoteDefinition { blocks, .. } => visit(blocks, fragment, anchors),
                BlockNode::Table(table) => table.rows.iter().find_map(|row| {
                    row.cells
                        .iter()
                        .find_map(|cell| visit(&cell.blocks, fragment, anchors))
                }),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    visit(blocks, fragment, &mut Anchorizer::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn relative_files_and_fragments_are_resolved_without_io() {
        let base = Some(Path::new("/tmp/docs/a folder"));
        assert_eq!(
            resolve_link("../other%20file.MD#caf%C3%A9-1", base),
            Ok(LinkDestination::Document {
                path: PathBuf::from("/tmp/docs/other file.MD"),
                fragment: Some("café-1".into()),
            })
        );
        assert_eq!(
            resolve_link("#caf%C3%A9-1", None),
            Ok(LinkDestination::Heading("café-1".into()))
        );
        assert_eq!(
            resolve_link("#", None),
            Ok(LinkDestination::Heading(String::new()))
        );
        assert_eq!(resolve_link("other.md", None), Err(LinkError::NoDirectory));
        assert_eq!(
            resolve_link("file:///tmp/a%23b.markdown", None),
            Ok(LinkDestination::Document {
                path: PathBuf::from("/tmp/a#b.markdown"),
                fragment: None,
            })
        );
        assert!(matches!(
            resolve_link("two words.md", base),
            Ok(LinkDestination::Document { .. })
        ));
        for target in ["https://example.test/a?b=2#c", "mailto:person@example.test"] {
            assert_eq!(
                resolve_link(target, base),
                Ok(LinkDestination::External(target.into()))
            );
        }
    }

    #[test]
    fn unsupported_protocols_and_ambiguous_paths_never_become_local_documents() {
        for target in [
            "javascript:alert(1)",
            "data:text/html,x",
            "custom:thing",
            "file://server/x.md",
            "//server/x.md",
            "\\\\server\\x.md",
            "../run.sh",
            "file:///tmp/x.png",
            "other.md?command=run",
            "https:\n//example.test",
            " https://example.test",
            "#%00",
            "#%FF",
            "file:///tmp/%00.md",
            "",
            "mailto:",
        ] {
            assert!(
                resolve_link(target, Some(Path::new("/tmp/docs"))).is_err(),
                "{target}"
            );
        }
    }

    #[test]
    fn heading_anchors_keep_duplicates_distinct_and_follow_canonical_structure() {
        let doc = Document::from_markdown("# Café **guide**\n\n## Café guide\n\nCafé guide-1\n----\n\n> ### Café guide\n\n# Last\n").unwrap();
        let snap = doc.snapshot();
        let ids: Vec<_> = [
            "café-guide",
            "café-guide-1",
            "café-guide-1-1",
            "café-guide-2",
            "last",
        ]
        .into_iter()
        .map(|anchor| heading_node(snap.blocks(), anchor).unwrap())
        .collect();
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            5
        );
        assert!(heading_node(snap.blocks(), "Café-guide").is_none());
        assert!(heading_node(snap.blocks(), "missing").is_none());
    }
}
