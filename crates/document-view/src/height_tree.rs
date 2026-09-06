#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum HeightTreeError {
    #[error("height index {index} is out of bounds for {len} fragments")]
    OutOfBounds { index: usize, len: usize },
    #[error("height must be finite and non-negative, got {0}")]
    InvalidHeight(f32),
}

/// Fenwick tree supporting O(log n) cumulative-height queries and updates.
#[derive(Clone, Debug, Default)]
pub struct HeightTree {
    heights: Vec<f32>,
    fenwick: Vec<f64>,
}

impl HeightTree {
    pub fn new(heights: impl IntoIterator<Item = f32>) -> Result<Self, HeightTreeError> {
        let heights: Vec<_> = heights.into_iter().collect();
        for height in &heights {
            validate_height(*height)?;
        }
        let mut tree = Self {
            fenwick: vec![0.0; heights.len() + 1],
            heights,
        };
        for index in 1..=tree.heights.len() {
            tree.fenwick[index] += f64::from(tree.heights[index - 1]);
            let parent = index + index.isolate_lowest_one();
            if parent < tree.fenwick.len() {
                tree.fenwick[parent] += tree.fenwick[index];
            }
        }
        Ok(tree)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    #[must_use]
    pub fn height(&self, index: usize) -> Option<f32> {
        self.heights.get(index).copied()
    }

    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.prefix_sum(self.len())
    }

    #[must_use]
    pub fn prefix_sum(&self, end: usize) -> f32 {
        let mut index = end.min(self.len());
        let mut total = 0.0;
        while index > 0 {
            total += self.fenwick[index];
            index &= index - 1;
        }
        total as f32
    }

    pub fn update(&mut self, index: usize, height: f32) -> Result<(), HeightTreeError> {
        validate_height(height)?;
        let len = self.len();
        let Some(previous) = self.heights.get_mut(index) else {
            return Err(HeightTreeError::OutOfBounds { index, len });
        };
        let delta = height - *previous;
        *previous = height;
        add(&mut self.fenwick, index, f64::from(delta));
        Ok(())
    }

    pub fn replace_all(
        &mut self,
        heights: impl IntoIterator<Item = f32>,
    ) -> Result<(), HeightTreeError> {
        *self = Self::new(heights)?;
        Ok(())
    }

    #[must_use]
    pub fn index_at_offset(&self, y: f32) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        let target = f64::from(y.max(0.0));
        let mut index = 0;
        let mut accumulated = 0.0;
        let mut bit = highest_power_of_two(self.len());
        while bit != 0 {
            let next = index + bit;
            if next <= self.len() && accumulated + self.fenwick[next] <= target {
                index = next;
                accumulated += self.fenwick[next];
            }
            bit >>= 1;
        }
        Some(index.min(self.len() - 1))
    }
}

fn validate_height(height: f32) -> Result<(), HeightTreeError> {
    if height.is_finite() && height >= 0.0 {
        Ok(())
    } else {
        Err(HeightTreeError::InvalidHeight(height))
    }
}

fn add(fenwick: &mut [f64], zero_based_index: usize, delta: f64) {
    let mut index = zero_based_index + 1;
    while index < fenwick.len() {
        fenwick[index] += delta;
        index += index.isolate_lowest_one();
    }
}

fn highest_power_of_two(value: usize) -> usize {
    if value == 0 {
        0
    } else {
        1 << (usize::BITS - 1 - value.leading_zeros())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_local_update_are_consistent() {
        let mut tree = HeightTree::new([10.0, 20.0, 30.0]).expect("valid heights");
        assert_eq!(tree.index_at_offset(0.0), Some(0));
        assert_eq!(tree.index_at_offset(9.9), Some(0));
        assert_eq!(tree.index_at_offset(10.0), Some(1));
        assert_eq!(tree.index_at_offset(59.0), Some(2));
        tree.update(1, 5.0).expect("valid update");
        assert_eq!(tree.total_height(), 45.0);
        assert_eq!(tree.index_at_offset(14.9), Some(1));
        assert_eq!(tree.index_at_offset(15.0), Some(2));
    }
}
