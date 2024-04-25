pub mod iter;

#[derive(Debug)]
pub struct PtBuffer<'a, T: 'a> {
    file_buffer: &'a [T],
    add_buffer: Vec<T>,
    pieces: Vec<Piece>,
    length: usize,
    last_edit_idx: usize,
    reusable_edit: ReusableEdit,
}

pub type PieceIdx = usize;
pub type Delta = usize;

#[derive(Debug, Copy, Clone)]
enum Location {
    Head(PieceIdx),
    Middle(PieceIdx, Delta),
    Tail(PieceIdx, Delta),
    EOF,
}

/// Either the original immutable buffer or the add buffer.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum WithBuffer {
    Original,
    Add,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct Piece {
    with_buffer: WithBuffer,
    start: usize,
    length: usize,
}

#[derive(Debug, Copy, Clone)]
enum ReusableEdit {
    Insert(usize, bool),
    Remove(Location),
    None,
}

impl<'a> PtBuffer<'a, u8> {
    pub fn line_column_to_idx(&self, column: usize, line: usize) -> usize {
        let mut l_count = 0;
        let mut c_count = 0;

        for (idx, c) in self.iter().enumerate() {
            if column == c_count && line == l_count {
                return idx;
            }

            if l_count == line {
                c_count += 1;
            }

            if c == &b'\n' {
                l_count += 1;
            }
        }

        panic!("(x:{column}, y:{line}) out of bound");
    }
}

impl<'a, T: 'a> PtBuffer<'a, T> {
    pub fn new(src: &'a [T]) -> Self {
        let piece = Piece {
            with_buffer: WithBuffer::Original,
            start: 0,
            length: src.len(),
        };

        Self {
            file_buffer: src,
            add_buffer: vec![],
            pieces: vec![piece],
            length: src.len(),
            last_edit_idx: 0,
            reusable_edit: ReusableEdit::None,
        }
    }

    pub fn push(&mut self, value: T) {
        let reuse = self.pieces.last().map_or(false, |last| {
            last.with_buffer == WithBuffer::Add && last.start + last.length == self.add_buffer.len()
        });

        self.add_buffer.push(value);

        if reuse {
            self.pieces.last_mut().unwrap().length += 1;
            self.reusable_edit = ReusableEdit::Insert(self.pieces.len() - 1, false);
        } else {
            self.pieces.push(Piece {
                start: self.add_buffer.len() - 1,
                length: 1,
                with_buffer: WithBuffer::Add,
            });
            self.reusable_edit = ReusableEdit::Insert(self.pieces.len() - 1, true);
        }

        self.last_edit_idx = self.length;
        self.length += 1;
    }

    pub fn insert(&mut self, at: usize, item: T) {
        debug_assert!(at <= self.length);
        match self.reusable_edit {
            ReusableEdit::Insert(piece_idx, _) if at == self.last_edit_idx + 1 => {
                let piece = &mut self.pieces[piece_idx];
                self.add_buffer.push(item);
                piece.length += 1;
            }
            _ => self.raw_insert(at, item),
        }

        self.last_edit_idx = at;
        self.length += 1;
    }

    pub fn remove(&mut self, at: usize) {
        debug_assert!(at < self.length);
        let piece_to_remove: Option<usize>;

        match self.reusable_edit {
            ReusableEdit::Insert(piece_idx, head) if at + 1 == self.last_edit_idx && head => {
                let piece = &mut self.pieces[piece_idx];
                piece.length -= 1;
                piece_to_remove = (piece.length == 0).then(|| piece_idx);
            }
            ReusableEdit::Remove(loc) if at == self.last_edit_idx => {
                println!("Reusable remove");
                piece_to_remove = self.raw_remove(loc);
            }
            _ => {
                let loc = self.index_to_piece_loc(at);
                piece_to_remove = self.raw_remove(loc);
            }
        }

        if let Some(piece_idx) = piece_to_remove {
            self.pieces.remove(piece_idx);

            if piece_idx > 0 {
                let idx = piece_idx - 1;
                let len = self.pieces[idx].length;
                let loc = if len == 1 {
                    Location::Head(idx)
                } else {
                    Location::Tail(idx, len - 1)
                };

                self.reusable_edit = ReusableEdit::Remove(loc);
            }
        }

        self.last_edit_idx = at;
    }

    pub fn len(&self) -> usize {
        self.length
    }
}

impl<'a, T: 'a> PtBuffer<'a, T> {
    fn raw_insert(&mut self, at: usize, item: T) {
        let piece_start = self.add_buffer.len();
        self.add_buffer.push(item);
        match self.index_to_piece_loc(at) {
            Location::Head(piece_idx) => {
                self.pieces.insert(
                    piece_idx,
                    Piece {
                        start: piece_start,
                        length: 1,
                        with_buffer: WithBuffer::Add,
                    },
                );

                self.reusable_edit = ReusableEdit::Insert(piece_idx, true);
            }
            Location::Middle(piece_idx, delta) | Location::Tail(piece_idx, delta) => {
                let origin = self.pieces[piece_idx];
                self.pieces[piece_idx].length = delta;

                let insert = Piece {
                    start: piece_start,
                    with_buffer: WithBuffer::Add,
                    length: 1,
                };

                let split = Piece {
                    with_buffer: origin.with_buffer,
                    start: origin.start + delta,
                    length: origin.length - delta,
                };

                self.pieces.insert(piece_idx + 1, insert);
                self.pieces.insert(piece_idx + 2, split);
                self.reusable_edit = ReusableEdit::Insert(piece_idx + 1, false);
            }
            Location::EOF => {
                let piece_idx = self.pieces.len();

                self.pieces.push(Piece {
                    with_buffer: WithBuffer::Add,
                    start: piece_start,
                    length: 1,
                });

                self.reusable_edit = ReusableEdit::Insert(piece_idx, true);
            }
        }
    }

    fn raw_remove(&mut self, location: Location) -> Option<usize> {
        match location {
            Location::Head(piece_idx) => {
                let piece = &mut self.pieces[piece_idx];
                piece.start += 1;
                piece.length -= 1;

                if piece.length == 0 {
                    return Some(piece_idx);
                };
            }
            Location::Tail(piece_idx, delta) => {
                self.pieces[piece_idx].length -= 1;

                let loc = if delta - 1 == 0 {
                    Location::Head(piece_idx)
                } else {
                    Location::Tail(piece_idx, delta - 1)
                };

                self.reusable_edit = ReusableEdit::Remove(loc);
            }
            Location::Middle(piece_idx, delta) => {
                let orig = self.pieces[piece_idx];
                self.pieces[piece_idx].length = delta;

                let start = delta + 1;
                if orig.length - start > 0 {
                    self.pieces.insert(
                        piece_idx + 1,
                        Piece {
                            start: orig.start + start,
                            length: orig.length - start,
                            with_buffer: orig.with_buffer,
                        },
                    );
                }

                if piece_idx > 0 {
                    let loc = if delta - 1 == 0 {
                        Location::Head(piece_idx)
                    } else {
                        Location::Middle(piece_idx, delta - 1)
                    };

                    self.reusable_edit = ReusableEdit::Remove(loc);
                }
            }
            Location::EOF => {}
        }

        None
    }


    fn index_to_piece_loc(&self, idx: usize) -> Location {
        let mut acc = 0;
        for (piece_idx, piece) in self.pieces.iter().enumerate() {
            if idx >= acc && idx < acc + piece.length {
                return match idx - acc {
                    0 => Location::Head(piece_idx),
                    delta if delta == piece.length - 1 => Location::Tail(piece_idx, delta),
                    delta => Location::Middle(piece_idx, delta),
                };
            }

            acc += piece.length;
        }

        Location::EOF
    }

    pub(crate) fn get_buffer(&'a self, piece: &Piece) -> &'a [T] {
        match piece.with_buffer {
            WithBuffer::Add => &self.add_buffer,
            WithBuffer::Original => self.file_buffer,
        }
    }
}

#[cfg(test)]
mod test {
    use unicode_segmentation::UnicodeSegmentation;

    use crate::{Piece, PtBuffer};

    #[test]
    fn should_create_a_pt_buffer() {
        let _buf = PtBuffer::new(b"Hello world");
    }

    #[test]
    fn should_insert_at_buffer_end() {
        let mut buf = PtBuffer::new(b"Hello ");
        buf.insert(6, b'w');

        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 0,
                    length: 6,
                    with_buffer: crate::WithBuffer::Original,
                },
                Piece {
                    start: 0,
                    length: 1,
                    with_buffer: crate::WithBuffer::Add,
                }
            ]
        )
    }

    #[test]
    fn should_repeatedly_insert_at_buffer_end() {
        let mut buf = PtBuffer::new(b"Hello ");
        insert_str_at(&mut buf, 6, "world");
        assert_buf_str(&buf, "Hello world")
    }

    #[test]
    fn should_insert_at_buffer_start() {
        let mut buf = PtBuffer::new(b"Hello ");
        buf.insert(0, b'o');
        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 0,
                    length: 1,
                    with_buffer: crate::WithBuffer::Add,
                },
                Piece {
                    start: 0,
                    length: 6,
                    with_buffer: crate::WithBuffer::Original,
                },
            ]
        )
    }

    #[test]
    fn should_insert_anywhere_in_buffer() {
        let mut buf = PtBuffer::new(b"Hello ");
        buf.insert(3, b'o');
        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 0,
                    length: 3,
                    with_buffer: crate::WithBuffer::Original,
                },
                Piece {
                    start: 0,
                    length: 1,
                    with_buffer: crate::WithBuffer::Add,
                },
                Piece {
                    start: 3,
                    length: 3,
                    with_buffer: crate::WithBuffer::Original,
                }
            ]
        )
    }

    #[test]
    fn should_remove_at_buffer_start() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(0);
        buf.remove(0);
        buf.remove(0);
        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 3,
                    length: 8,
                    with_buffer: crate::WithBuffer::Original,
                },
            ]
        )
    }

    #[test]
    fn should_remove_at_buffer_end() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(10);
        buf.remove(9);
        buf.remove(8);
        assert_eq!(
            buf.pieces,
            [Piece {
                start: 0,
                length: 8,
                with_buffer: crate::WithBuffer::Original,
            }, ]
        )
    }

    #[test]
    fn should_remove_anywhere_in_buffer() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(3);

        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 0,
                    length: 3,
                    with_buffer: crate::WithBuffer::Original,
                },
                Piece {
                    start: 4,
                    length: 7,
                    with_buffer: crate::WithBuffer::Original,
                }
            ]
        )
    }

    #[test]
    fn should_push_to_buffer() {
        let mut buf = PtBuffer::new(b"Hello");
        buf.push(b' ');
        buf.push(b'w');
        buf.push(b'o');
        buf.push(b'r');
        buf.push(b'd');

        assert_eq!(
            buf.pieces,
            [
                Piece {
                    start: 0,
                    length: 5,
                    with_buffer: crate::WithBuffer::Original,
                },
                Piece {
                    start: 0,
                    length: 5,
                    with_buffer: crate::WithBuffer::Add,
                }
            ]
        )
    }

    #[test]
    fn should_repeatedly_remove_at_buffer_end() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(10);
        buf.remove(9);
        buf.remove(8);
        buf.remove(7);
        buf.remove(6);
        buf.remove(5);
        assert_buf_str(&buf, "Hello")
    }

    #[test]
    fn scattered_edits() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(1);
        insert_str_at(&mut buf, 1, "3");
        buf.remove(4);
        insert_str_at(&mut buf, 4, "0");
        assert_buf_str(&buf, "H3ll0 world");
        insert_str_at(&mut buf, 8, "$");
        buf.remove(7);
        assert_buf_str(&buf, "H3ll0 w$rld");
    }

    #[test]
    fn mhh() {
        let strs: Vec<&str> = "Hello world".graphemes(true).collect();
        let mut buf = PtBuffer::new(&strs);
        buf.insert(8, "$");
        buf.remove(7);
        let x: Vec<&str> = buf.iter().copied().collect();
        let string = x.join("");
        assert_eq!("Hello w$rld", string);
    }

    fn insert_str_at(buf: &mut PtBuffer<u8>, idx: usize, s: &str) {
        for (i, char) in s.bytes().enumerate() {
            buf.insert(idx + i, char)
        }
    }

    fn assert_buf_str(buf: &PtBuffer<u8>, s: &str) {
        let string: Vec<u8> = buf.iter().copied().collect();
        assert_eq!(s, String::from_utf8_lossy(&string));
    }
}
