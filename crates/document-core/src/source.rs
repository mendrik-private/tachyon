use std::{ops::Range, path::PathBuf, sync::Arc, time::SystemTime};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{NodeId, Revision};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceUnit {
    pub(crate) prefix: Range<usize>,
    pub(crate) source: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct SourceSpine {
    original: Arc<str>,
    units: Arc<FxHashMap<NodeId, SourceUnit>>,
    order: Arc<[NodeId]>,
    tail_start: usize,
    line_ending: LineEnding,
}

impl SourceSpine {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            original: Arc::from(""),
            units: Arc::new(FxHashMap::default()),
            order: Arc::from([]),
            tail_start: 0,
            line_ending: LineEnding::Lf,
        }
    }

    #[must_use]
    pub(crate) fn new(
        original: Arc<str>,
        units: FxHashMap<NodeId, SourceUnit>,
        order: Vec<NodeId>,
        tail_start: usize,
    ) -> Self {
        let line_ending = if original.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        Self {
            original,
            units: Arc::new(units),
            order: order.into(),
            tail_start,
            line_ending,
        }
    }

    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    #[must_use]
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    #[must_use]
    pub(crate) fn unit(&self, node: NodeId) -> Option<&SourceUnit> {
        self.units.get(&node)
    }

    #[must_use]
    pub(crate) fn order(&self) -> &[NodeId] {
        &self.order
    }

    #[must_use]
    pub(crate) fn tail(&self) -> &str {
        &self.original[self.tail_start..]
    }

    #[must_use]
    pub(crate) fn slice(&self, range: Range<usize>) -> &str {
        &self.original[range]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceIdentity {
    pub path: PathBuf,
    pub length: u64,
    pub modified: Option<SystemTime>,
    pub content_hash: [u8; 32],
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

#[derive(Clone, Debug)]
pub struct SaveSnapshot {
    pub revision: Revision,
    pub bytes: Arc<[u8]>,
    pub expected_identity: Option<SourceIdentity>,
}
