use std::iter::Rev;
use std::ops::{Bound, Index, RangeBounds};

use crate::{Location, PtBuffer, WithBuffer};

pub struct Iter<'a, T: 'a> {
    table: &'a PtBuffer<'a, T>,
    piece_idx: usize,
    it: std::slice::Iter<'a, T>,
}

pub struct RevIter<'a, T: 'a> {
    table: &'a PtBuffer<'a, T>,
    piece_idx: usize,
    it: Rev<std::slice::Iter<'a, T>>,
}

pub struct RevRange<'a, T: 'a> {
    iter: RevIter<'a, T>,
    idx: usize,
    to: usize,
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

    pub fn rev_iter(&'a self) -> RevIter<'a, T> {
        self.make_rev_iter(0..self.length - 1)
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

        Range {
            iter: self.make_iter(from),
            idx: from,
            to,
        }
    }

    pub fn rev_range(&'a self, range: impl RangeBounds<usize>) -> RevRange<'a, T> {
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

        RevRange {
            iter: self.make_rev_iter(from..to),
            idx: from,
            to,
        }
    }

    fn make_rev_iter(&'a self, range: std::ops::Range<usize>) -> RevIter<'a, T> {
        let (piece_idx, piece, range) = match self.index_to_piece_loc(range.end) {
            Location::Head(piece_idx) => {
                let piece = self.pieces[piece_idx];
                (piece_idx, piece, piece.length..piece.start + piece.length)
            }
            Location::Middle(piece_idx, norm_idx) | Location::Tail(piece_idx, norm_idx) => {
                let piece = self.pieces[piece_idx];
                (
                    piece_idx,
                    piece,
                    piece.length - norm_idx..piece.start + piece.length,
                )
            }
            Location::Eof => {
                let idx = self.pieces.len() - 1;
                let piece = self.pieces[idx];
                (idx, piece, 0..range.end - range.start)
            }
        };

        let buf = self.get_buffer(&piece);
        let it = buf[range].iter().rev();

        RevIter {
            table: self,
            piece_idx,
            it,
        }
    }

    fn make_iter(&'a self, idx: usize) -> Iter<'a, T> {
        let (piece_idx, norm_idx) = match self.index_to_piece_loc(idx) {
            Location::Head(piece_idx) => (piece_idx, 0),
            Location::Middle(piece_idx, norm_idx) | Location::Tail(piece_idx, norm_idx) => {
                (piece_idx, norm_idx)
            }
            Location::Eof => {
                let it = self.add_buffer[0..0].iter();
                return Iter {
                    table: self,
                    piece_idx: self.pieces.len(),
                    it,
                };
            }
        };

        let piece = self.pieces[piece_idx];
        let buf = self.get_buffer(&piece);
        let it = buf[piece.start + norm_idx..piece.start + piece.length].iter();

        Iter {
            table: self,
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

impl<'a, T> Iterator for RevRange<'a, T> {
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

impl<'a, T> Iterator for RevIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.it.next() {
            Some(next) => Some(next),
            None => {
                if self.piece_idx == 0 {
                    None
                } else {
                    self.piece_idx = self.piece_idx.saturating_sub(1);
                    let piece = self.table.pieces[self.piece_idx];
                    let buf = self.table.get_buffer(&piece);
                    let range = piece.start..piece.start + piece.length;
                    self.it = buf[range].iter().rev();
                    self.next()
                }
            }
        }
    }
}

impl<'a, T> Index<usize> for PtBuffer<'a, T> {
    type Output = T;

    /// Note: Reading an index takes `O(p)` time, use iterators for fast sequential access.
    fn index(&self, idx: usize) -> &T {
        let (piece_idx, norm_idx) = match self.index_to_piece_loc(idx) {
            Location::Head(piece_idx) => (piece_idx, 0),
            Location::Middle(piece_idx, norm_idx) | Location::Tail(piece_idx, norm_idx) => {
                (piece_idx, norm_idx)
            }
            Location::Eof => panic!("PieceTable out of bounds: {}", idx),
        };

        let piece = &self.pieces[piece_idx];
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
    fn should_get_pt_range2() {
        let buf = PtBuffer::new(b"Hello boys and girls");
        let bytes: Vec<u8> = buf.range(..6).copied().collect();
        let cow = String::from_utf8_lossy(&bytes);
        assert_eq!("Hello ", cow);
    }

    #[test]
    fn should_get_pt_range_2() {
        let buf = PtBuffer::new(b"Helo");
        let r1 = buf.range(0..2);
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        let r2 = buf.range(2..4);
        let b2: Vec<u8> = r2.copied().collect();
        let c2 = String::from_utf8_lossy(&b2);
        println!("{}", c1);
        println!("{}", c2);

        assert_eq!(c1, "He");
        assert_eq!(c2, "lo");
    }

    #[test]
    fn should_iter_pt() {
        let mut buf = PtBuffer::new(b"Helo");
        buf.insert(0, b'b');
        buf.insert(0, b'a');
        buf.insert(0, b'c');
        buf.insert(4, b' ');

        println!("{:?}", buf.pieces);
        let r1 = buf.iter();
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "cabH elo");
    }

    #[test]
    fn should_rev_iter_pt_rev() {
        let mut buf = PtBuffer::new(b"Helo");
        buf.insert(0, b'b');
        buf.insert(0, b'a');
        buf.insert(0, b'c');
        buf.insert(4, b' ');

        let b0: Vec<u8> = buf.iter().copied().collect();
        let c0 = String::from_utf8_lossy(&b0);
        assert_eq!(c0, "cabH elo");

        let b1: Vec<u8> = buf.rev_iter().copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "ole Hbac");
    }

    #[test]
    fn should_rev_iter_range_pt_rev() {
        let buf = PtBuffer::new(b"abcd");

        let r1 = buf.rev_range(0..3);
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "dcb");

        let buf = PtBuffer::new(b"abcd");

        let r1 = buf.rev_range(0..2);
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "dc");

        let r1 = buf.rev_range(2..4);
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "ba");

        let r1 = buf.rev_range(1..4);
        let b1: Vec<u8> = r1.copied().collect();
        let c1 = String::from_utf8_lossy(&b1);

        assert_eq!(c1, "cba");
    }
}
