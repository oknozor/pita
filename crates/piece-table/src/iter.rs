use std::iter::Rev;
use std::ops::{Bound, Index, RangeBounds};
use crate::{Location, PtBuffer, WithBuffer};

pub struct Iter<'a, T: 'a> {
    table: &'a PtBuffer<'a, T>,
    piece_idx: usize,
    it: std::slice::Iter<'a, T>,
}

pub struct Range<'a, T: 'a> {
    iter: Iter<'a, T>,
    idx: usize,
    to: usize,
}

impl<'a, T: 'a> PtBuffer<'a, T> {
    pub fn iter(&'a self) -> Iter<'a, T> {
        self.make_iter(0)
    }

    pub fn range(&'a self, range: impl RangeBounds<usize>) -> Range<'a, T> {
        let from = match range.start_bound() {
            Bound::Included(x) => *x,
            Bound::Excluded(x) => *x + 1,
            Bound::Unbounded => 0,
        };

        let to = match range.end_bound() {
            Bound::Included(x) => *x + 1,
            Bound::Excluded(x) => *x,
            Bound::Unbounded => self.length,
        };


        let iter = self.make_iter(from);

        Range {
            iter: iter,
            idx: from,
            to: to,
        }
    }

    fn make_iter(&'a self, idx: usize) -> Iter<'a, T> {
        let (piece_idx, norm_idx) = match self.index_to_piece_loc(idx) {
            Location::Head(piece_idx) => (piece_idx, 0),
            Location::Middle(piece_idx, norm_idx) | Location::Tail(piece_idx, norm_idx) => {
                (piece_idx, norm_idx)
            }
            Location::EOF => {
                let it = self.add_buffer[0..0].iter();
                return Iter {
                    table: &self,
                    piece_idx: self.pieces.len(),
                    it: it,
                };
            }
        };

        let piece = self.pieces[piece_idx];
        let buf = self.get_buffer(&piece);
        let it = buf[piece.start + norm_idx..piece.start + piece.length].iter();

        Iter {
            table: &self,
            piece_idx,
            it,
        }
    }
}

impl<'a, T> Iterator for Range<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.to {
            None
        } else {
            self.idx += 1;
            self.iter.next()
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.it.next() {
            Some(next) => Some(next),
            None => {
                self.piece_idx += 1;

                if self.piece_idx >= self.table.pieces.len() {
                    None
                } else {
                    let piece = self.table.pieces[self.piece_idx];
                    let buf = self.table.get_buffer(&piece);

                    self.it = buf[piece.start..piece.start + piece.length].iter();
                    self.next()
                }
            }
        }
    }
}

impl<'a, T> DoubleEndedIterator for Range<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.idx >= self.to {
            None
        } else {
            self.idx += 1;
            self.iter.it.next_back()
        }
    }
}


impl<'a, T> Index<usize> for PtBuffer<'a, T> {
    type Output = T;

    /// Note: Reading an index takes `O(p)` time, use iterators for fast sequential access.
    fn index<'b>(&'b self, idx: usize) -> &'b T {
        let (piece_idx, norm_idx) = match self.index_to_piece_loc(idx) {
            Location::Head(piece_idx) => (piece_idx, 0),
            Location::Middle(piece_idx, norm_idx) |
            Location::Tail(piece_idx, norm_idx) => (piece_idx, norm_idx),
            Location::EOF => panic!("PieceTable out of bounds: {}", idx),
        };

        let ref piece = self.pieces[piece_idx];
        match piece.with_buffer {
            WithBuffer::Original => &self.file_buffer[piece.start + norm_idx],
            WithBuffer::Add => &self.add_buffer[piece.start + norm_idx],
        }
    }
}

#[cfg(test)]
mod test {
    use crate::PtBuffer;

    #[test]
    fn should_iter_piece_table() {
        let mut buf = PtBuffer::new(b"Hello ");
        buf.insert(buf.length, b'w');
        buf.insert(buf.length, b'o');
        buf.insert(buf.length, b'r');
        buf.insert(buf.length, b'l');
        buf.insert(buf.length, b'd');

        let bytes: Vec<u8> = buf.iter().copied().collect();
        let cow = String::from_utf8_lossy(&bytes);
        assert_eq!("Hello world", cow);
    }

    #[test]
    fn should_get_pt_range() {
        let buf = PtBuffer::new(b"Hello boys and girls");
        let bytes: Vec<u8> = buf.range(6..buf.len()).copied().collect();
        let cow = String::from_utf8_lossy(&bytes);
        assert_eq!("boys and girls", cow);
    }

    #[test]
    fn should_get_pt_range_tev() {
        let buf = PtBuffer::new(b"Hello boys and girls");
        let bytes: Vec<u8> = buf.range(6..buf.len()).rev().copied().collect();
        let cow = String::from_utf8_lossy(&bytes);
        assert_eq!("slrig dna syob", cow);
    }

    #[test]
    fn should_get_pt_range_rev2() {
        let buf = PtBuffer::new(b"Helo");
        let range = buf.range(0..2).rev();
        let bytes: Vec<u8> = range.copied().collect();
        let cow1 = String::from_utf8_lossy(&bytes);

        println!("{}", cow1);
        let rangedd = buf.range(2..4).rev();
        let bytesdd: Vec<u8> = rangedd.copied().collect();
        let cow2 = String::from_utf8_lossy(&bytesdd);
        println!("{}", cow2);

        assert_ne!(cow1, cow2)
    }
}
