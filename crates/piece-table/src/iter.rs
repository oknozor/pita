use crate::{Location, PtBuffer};

pub struct Iter<'a, T: 'a> {
    table: &'a PtBuffer<'a, T>,
    piece_idx: usize,
    it: std::slice::Iter<'a, T>,
}

impl<'a, T: 'a> PtBuffer<'a, T> {
    pub fn iter(&'a self) -> Iter<'a, T> {
        let (piece_idx, norm_idx) = match self.index_to_piece_loc(0) {
            Location::Head(piece_idx) => (piece_idx, 0),
            Location::Middle(piece_idx, norm_idx) | Location::Tail(piece_idx, norm_idx) => {
                (piece_idx, norm_idx)
            }
            Location::EOF => {
                let it = self.adds[0..0].iter();
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
            piece_idx: piece_idx,
            it: it,
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
}
