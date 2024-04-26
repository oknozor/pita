use std::cmp::min;

use crate::Editor;

impl Editor<'_> {
    pub(crate) fn cursor_left(&self) -> bool {
        let (mut x, y) = self.editor_screen.cursor();
        if x == 0 {
            if self.editor_screen.line_offset() > 0 {
                x = self.line_endings[y].saturating_sub(1);
            }
            let y = y.saturating_sub(1);
            if y == 0 {
                self.editor_screen.dec_offset();
            }

            self.editor_screen.set_cursor(x, y);
            true
        } else {
            self.editor_screen.set_cursor(x.saturating_sub(1), y);
            false
        }
    }

    pub(crate) fn cursor_right(&self) -> bool {
        let (x, y) = self.editor_screen.cursor();
        let ending = self.line_endings[y + 1];

        if x >= ending - 1 {
            let y = y + 1;
            if y > self.editor_screen.height() - 1 {
                self.editor_screen.inc_offset();
            }
            self.editor_screen.set_cursor(0, y);
            true
        } else {
            self.editor_screen.set_cursor(x + 1, y);
            false
        }
    }

    pub(crate) fn cursor_down(&self) -> bool {
        let (mut x, mut y) = self.editor_screen.cursor();
        y = min(y + 1, self.line_endings.len() - 1);
        self.log(format!("y: {y}, or: {}", self.line_endings.len() - 1));
        let ending = self.line_endings[y];

        if x >= ending {
            x = ending - 1;
        }

        if y > self.editor_screen.height() - 1 {
            self.editor_screen.inc_offset();
            true
        } else {
            self.editor_screen.set_cursor(x, y);
            false
        }
    }

    pub(crate) fn cursor_up(&self) -> bool {
        let (mut x, mut y) = self.editor_screen.cursor();
        if y == 0 {
            self.editor_screen.dec_offset();
            true
        } else {
            y = y - 1;
            let ending = self.line_endings[y];
            if x > ending {
                x = ending - 1;
            }
            self.editor_screen.set_cursor(x, y);
            false
        }
    }

    pub(crate) fn cursor_next_line(&self) -> bool {
        let (_, y) = self.editor_screen.cursor();
        self.editor_screen.set_cursor(0, y + 1);
        true
    }

}