use std::sync::Arc;

use crate::{
    BlockNode, BlockSequence, ColumnAlignment, ColumnSpec, DocumentError, NodeId, Paragraph,
    RichText, Table, TableCell, TableRow,
};

/// Lines of pasted TSV: `\r\n`, `\r` and `\n` each end one line. Interior
/// empty lines are rows (clearing their cells); one trailing terminator is
/// not a row of its own.
fn tsv_lines(text: &str) -> Vec<&str> {
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix(['\n', '\r']))
        .unwrap_or(text);
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' | b'\n' => {
                lines.push(&text[start..index]);
                index += if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = index;
            }
            _ => index += 1,
        }
    }
    lines.push(&text[start..]);
    lines
}

impl Table {
    #[must_use]
    pub fn new_default(mut allocate: impl FnMut() -> NodeId) -> Self {
        let id = allocate();
        let columns: Arc<[ColumnSpec]> = vec![ColumnSpec::default(), ColumnSpec::default()].into();
        let rows = (0..3)
            .map(|_| empty_row(2, &mut allocate))
            .collect::<Vec<_>>()
            .into();
        Self {
            id,
            columns,
            rows,
            header_rows: 1,
            border: crate::TableBorder::default(),
            preserved_metadata: Arc::from([]),
        }
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub(crate) fn insert_row(
        &mut self,
        index: usize,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        if index > self.row_count() {
            return Err(DocumentError::TableCoordinate {
                row: index,
                column: 0,
            });
        }
        let mut rows = self.rows.to_vec();
        rows.insert(index, empty_row(self.column_count(), &mut allocate));
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn delete_row(&mut self, index: usize) -> Result<(), DocumentError> {
        if self.row_count() <= 1 {
            return Err(DocumentError::EmptyTable);
        }
        let mut rows = self.rows.to_vec();
        if index >= rows.len() {
            return Err(DocumentError::TableCoordinate {
                row: index,
                column: 0,
            });
        }
        rows.remove(index);
        self.header_rows = self.header_rows.min(rows.len());
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn duplicate_row(
        &mut self,
        index: usize,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        let Some(source) = self.rows.get(index) else {
            return Err(DocumentError::TableCoordinate {
                row: index,
                column: 0,
            });
        };
        let duplicate = source.map_ids(&mut |_| allocate());
        let mut rows = self.rows.to_vec();
        rows.insert(index + 1, duplicate);
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn move_row(&mut self, from: usize, to: usize) -> Result<(), DocumentError> {
        if from >= self.row_count() || to >= self.row_count() {
            return Err(DocumentError::TableCoordinate {
                row: from.max(to),
                column: 0,
            });
        }
        let mut rows = self.rows.to_vec();
        let row = rows.remove(from);
        rows.insert(to, row);
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn insert_column(
        &mut self,
        index: usize,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        if index > self.column_count() {
            return Err(DocumentError::TableCoordinate {
                row: 0,
                column: index,
            });
        }
        let mut columns = self.columns.to_vec();
        columns.insert(index, ColumnSpec::default());
        let mut rows = self.rows.to_vec();
        for row in &mut rows {
            let mut cells = row.cells.to_vec();
            cells.insert(index, empty_cell(&mut allocate));
            row.cells = cells.into();
        }
        self.columns = columns.into();
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn delete_column(&mut self, index: usize) -> Result<(), DocumentError> {
        if self.column_count() <= 1 {
            return Err(DocumentError::EmptyTable);
        }
        if index >= self.column_count() {
            return Err(DocumentError::TableCoordinate {
                row: 0,
                column: index,
            });
        }
        let mut columns = self.columns.to_vec();
        columns.remove(index);
        let mut rows = self.rows.to_vec();
        for row in &mut rows {
            let mut cells = row.cells.to_vec();
            cells.remove(index);
            row.cells = cells.into();
        }
        self.columns = columns.into();
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn duplicate_column(
        &mut self,
        index: usize,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        let Some(column) = self.columns.get(index).cloned() else {
            return Err(DocumentError::TableCoordinate {
                row: 0,
                column: index,
            });
        };
        let mut columns = self.columns.to_vec();
        columns.insert(index + 1, column);
        let mut rows = self.rows.to_vec();
        for row in &mut rows {
            let mut cells = row.cells.to_vec();
            let duplicate = cells[index].map_ids(&mut |_| allocate());
            cells.insert(index + 1, duplicate);
            row.cells = cells.into();
        }
        self.columns = columns.into();
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn clear_cell(
        &mut self,
        row: usize,
        column: usize,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        let mut rows = self.rows.to_vec();
        let Some(target_row) = rows.get_mut(row) else {
            return Err(DocumentError::TableCoordinate { row, column });
        };
        let mut cells = target_row.cells.to_vec();
        let Some(cell) = cells.get_mut(column) else {
            return Err(DocumentError::TableCoordinate { row, column });
        };
        cell.blocks = empty_cell_blocks(&mut allocate);
        target_row.cells = cells.into();
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn move_column(&mut self, from: usize, to: usize) -> Result<(), DocumentError> {
        if from >= self.column_count() || to >= self.column_count() {
            return Err(DocumentError::TableCoordinate {
                row: 0,
                column: from.max(to),
            });
        }
        let mut columns = self.columns.to_vec();
        let column = columns.remove(from);
        columns.insert(to, column);
        let mut rows = self.rows.to_vec();
        for row in &mut rows {
            let mut cells = row.cells.to_vec();
            let cell = cells.remove(from);
            cells.insert(to, cell);
            row.cells = cells.into();
        }
        self.columns = columns.into();
        self.rows = rows.into();
        Ok(())
    }

    pub(crate) fn set_alignment(
        &mut self,
        column: usize,
        alignment: ColumnAlignment,
    ) -> Result<(), DocumentError> {
        let mut columns = self.columns.to_vec();
        let Some(spec) = columns.get_mut(column) else {
            return Err(DocumentError::TableCoordinate { row: 0, column });
        };
        spec.alignment = alignment;
        self.columns = columns.into();
        Ok(())
    }

    pub(crate) fn set_width(&mut self, column: usize, width: f32) -> Result<(), DocumentError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(DocumentError::InvalidColumnWidth);
        }
        let mut columns = self.columns.to_vec();
        let Some(spec) = columns.get_mut(column) else {
            return Err(DocumentError::TableCoordinate { row: 0, column });
        };
        spec.width = Some(width);
        self.columns = columns.into();
        Ok(())
    }

    pub(crate) fn paste_tsv(
        &mut self,
        start_row: usize,
        start_column: usize,
        text: &str,
        mut allocate: impl FnMut() -> NodeId,
    ) -> Result<(), DocumentError> {
        if start_row >= self.row_count() || start_column >= self.column_count() {
            return Err(DocumentError::TableCoordinate {
                row: start_row,
                column: start_column,
            });
        }
        let matrix: Vec<Vec<&str>> = tsv_lines(text)
            .into_iter()
            .map(|line| line.split('\t').collect())
            .collect();
        if matrix.is_empty() {
            return Ok(());
        }
        let required_columns = start_column + matrix.iter().map(Vec::len).max().unwrap_or(0);
        let mut rows = self.rows.to_vec();
        let added_columns = required_columns.saturating_sub(self.column_count());
        if added_columns > 0 {
            // Allocate column by column, as repeated column insertion would.
            let mut added = rows
                .iter()
                .map(|_| Vec::with_capacity(added_columns))
                .collect::<Vec<Vec<TableCell>>>();
            for _ in 0..added_columns {
                for cells in &mut added {
                    cells.push(empty_cell(&mut allocate));
                }
            }
            for (row, added) in rows.iter_mut().zip(added) {
                let mut cells = row.cells.to_vec();
                cells.extend(added);
                row.cells = cells.into();
            }
            let mut columns = self.columns.to_vec();
            columns.resize(required_columns, ColumnSpec::default());
            self.columns = columns.into();
        }
        let required_rows = start_row + matrix.len();
        while rows.len() < required_rows {
            rows.push(empty_row(self.column_count(), &mut allocate));
        }

        for (row_offset, values) in matrix.iter().enumerate() {
            let row = &mut rows[start_row + row_offset];
            let mut cells = row.cells.to_vec();
            for (column_offset, value) in values.iter().enumerate() {
                let cell = &mut cells[start_column + column_offset];
                let paragraph_id = allocate();
                cell.blocks = BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(Paragraph {
                    id: paragraph_id,
                    content: RichText::new(*value),
                }))]);
            }
            row.cells = cells.into();
        }
        self.rows = rows.into();
        Ok(())
    }

    #[must_use]
    pub fn requires_html_serialization(&self) -> bool {
        self.header_rows != 1
            || self.rows.iter().any(|row| {
                row.cells.iter().any(|cell| {
                    cell.blocks.len() != 1
                        || !matches!(
                            cell.blocks.get(0).map(Arc::as_ref),
                            Some(BlockNode::Paragraph(_))
                        )
                })
            })
    }
}

fn empty_row(columns: usize, allocate: &mut impl FnMut() -> NodeId) -> TableRow {
    TableRow {
        id: allocate(),
        cells: (0..columns)
            .map(|_| empty_cell(allocate))
            .collect::<Vec<_>>()
            .into(),
    }
}

fn empty_cell(allocate: &mut impl FnMut() -> NodeId) -> TableCell {
    TableCell {
        id: allocate(),
        blocks: empty_cell_blocks(allocate),
    }
}

fn empty_cell_blocks(allocate: &mut impl FnMut() -> NodeId) -> BlockSequence {
    BlockSequence::new(vec![Arc::new(BlockNode::Paragraph(Paragraph {
        id: allocate(),
        content: RichText::default(),
    }))])
}
