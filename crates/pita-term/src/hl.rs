use std::ops::{Add, Index};
use std::slice::Iter;

#[derive(Debug)]
pub struct HlQueue {
    inner: Vec<(usize, usize, usize)>,
}

impl HlQueue {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Vec::with_capacity(cap),
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn push(&mut self, item: (usize, usize, usize)) {
        self.inner.push(item);
    }
}

impl HlQueue {
    pub fn get(&self, index: usize) -> Option<usize> {
        let idx = index + 1;
        self.inner.iter().find(|(start, end, hl)| {
            idx >= *start && idx < *end
        }).map(|h|h.2)
    }


}

#[cfg(test)]
mod test {
    use crate::hl::HlQueue;

    #[test]
    fn index_hls() {
        let hls = HlQueue {
            inner: vec![(0, 3, 11), (4, 6, 12)],
        };

        assert_eq!(hls.get(2), Some(11));
        assert_eq!(hls.get(6), Some(12));
        assert_eq!(hls.get(7), None);
    }
}