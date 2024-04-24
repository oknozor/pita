pub mod iter;

pub struct PtBuffer<'a, T: 'a> {
    original: &'a [T],
    adds: Vec<T>,
    pieces: Vec<Piece>,
    length: usize,
    last_insert: Option<usize>,
    last_remove: Option<Location>,
}

pub type PieceIdx = usize;
pub type Delta = usize;

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

impl<'a, T: 'a> PtBuffer<'a, T> {
    pub fn new(src: &'a [T]) -> Self {
        let piece = Piece {
            with_buffer: WithBuffer::Original,
            start: 0,
            length: src.len(),
        };

        Self {
            original: src,
            adds: vec![],
            pieces: vec![piece],
            length: src.len(),
            last_insert: None,
            last_remove: None,
        }
    }

    pub fn insert(&mut self, at: usize, item: T) {
        debug_assert!(at <= self.length);
        let piece_start = self.adds.len();
        self.adds.push(item);
        match self.index_to_piece_loc(at) {
            Location::Head(piece_idx) => self.pieces.insert(
                piece_idx,
                Piece {
                    start: piece_start,
                    length: 1,
                    with_buffer: WithBuffer::Add,
                },
            ),
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
            }
            Location::EOF => self.pieces.push(Piece {
                with_buffer: WithBuffer::Add,
                start: piece_start,
                length: 1,
            }),
        }
        self.length += 1;
    }

    pub fn remove(&mut self, at: usize) {
        debug_assert!(at < self.length);
        let location = self.index_to_piece_loc(at);
        match location {
            Location::Head(piece_idx) => {
                let remove = {
                    let mut piece = &mut self.pieces[piece_idx];
                    piece.start += 1;
                    piece.length -= 1;
                    piece.length == 0
                };

                if remove {
                    self.pieces.remove(piece_idx);
                }
            }
            Location::Tail(piece_idx, delta) => {
                self.pieces[piece_idx].length -= 1;
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
            }
            Location::EOF => {}
        }
    }
}

// Private functions
// Todo: extract to a hidden module
impl<'a, T: 'a> PtBuffer<'a, T> {
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
            WithBuffer::Add => &self.adds,
            WithBuffer::Original => self.original,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{Piece, PtBuffer};

    #[test]
    fn should_create_a_pt_buffer() {
        let _buf = PtBuffer::new(b"Hello world");
    }

    #[test]
    fn should_insert_at_buffer_start() {
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
    fn should_insert_at_buffer_end() {
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
        assert_eq!(
            buf.pieces,
            [Piece {
                start: 1,
                length: 10,
                with_buffer: crate::WithBuffer::Original,
            },]
        )
    }

    #[test]
    fn should_remove_at_buffer_end() {
        let mut buf = PtBuffer::new(b"Hello world");
        buf.remove(10);
        assert_eq!(
            buf.pieces,
            [Piece {
                start: 0,
                length: 10,
                with_buffer: crate::WithBuffer::Original,
            },]
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
}
