// TODO: introduce a cursor to make instan piece split
// TODO: How could piece table handle multi cursor ?
// TODO: implement insert, delete and slice

use std::{ops::Range, str::FromStr};

pub struct PieceTable {
    original_file: String,
    add_file: String,
    pieces: Vec<Piece>,
}

struct Piece {
    which: WhichBuffer,
    position: usize,
    lenght: usize,
}

enum WhichBuffer {
    Original,
    Add,
}

impl Piece {
    fn new(which: WhichBuffer, position: usize, lenght: usize) -> Piece {
        Self {
            which,
            position,
            lenght,
        }
    }
}

impl From<&str> for PieceTable {
    fn from(value: &str) -> Self {
        PieceTable {
            original_file: value.to_string(),
            add_file: String::new(),
            pieces: vec![Piece::new(WhichBuffer::Original, 0, value.len())],
        }
    }
}

impl PieceTable {
    pub fn slice(range: Range<usize>) -> String {
        todo!()
    }

    pub fn insert(&mut self, position: usize, text: &str) {
        self.add_file.push_str(text);
        let mut cursor = 0;
        for piece in &self.pieces {
            if cursor < position && (cursor + piece.lenght) > position {}
            cursor += piece.lenght;
        }
        todo!()
    }

    pub fn delete(&mut self, start: usize, end: usize) {
        todo!()
    }
}
