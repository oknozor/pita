use crossterm::style::{Color, Print};
use crossterm::terminal::EnterAlternateScreen;
use crossterm::{execute, queue, terminal};
use std::cell::{Cell, RefCell};
use std::io;
use std::io::{BufWriter, Stdout, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const DEFAULT_BG: Color = Color::Rgb {
    r: 59,
    g: 56,
    b: 73,
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct Style(pub Color, pub Color); // Fg, Bg

pub struct Screen {
    width: usize,
    height: usize,
    out: RefCell<BufWriter<Stdout>>,
    buf: RefCell<Vec<Option<(Style, String)>>>,
    cursor: Cell<(u16, u16)>,
    line_offset: Cell<usize>,
}

impl Screen {
    pub fn new() -> io::Result<Self> {
        let mut out = BufWriter::new(io::stdout());
        execute!(out, EnterAlternateScreen)?;
        queue!(out, crossterm::cursor::SetCursorStyle::SteadyBar)?;
        terminal::enable_raw_mode()?;
        let (width, height) = terminal::size()?;
        let buf = std::iter::repeat(Some((Style(Color::White, DEFAULT_BG), " ".into())))
            .take(width as usize * height as usize)
            .collect();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            out: RefCell::new(out),
            buf: RefCell::new(buf),
            cursor: Cell::new((0, 0)),
            line_offset: Cell::new(0),
        })
    }

    pub(crate) fn present(&self) {
        let mut out = self.out.borrow_mut();
        let buf = self.buf.borrow();

        let mut last_style = Style(Color::White, DEFAULT_BG);

        queue!(
            out,
            crossterm::style::SetForegroundColor(last_style.0),
            crossterm::style::SetBackgroundColor(last_style.1),
            crossterm::cursor::Hide
        )
        .unwrap();

        // Write everything to the buffered output.
        for y in 0..self.height {
            let mut x = 0;
            while x < self.width {
                if let Some((style, ref text)) = buf[y * self.width + x] {
                    queue!(out, crossterm::cursor::MoveTo(x as u16, y as u16)).unwrap();
                    if style != last_style {
                        queue!(
                            out,
                            crossterm::style::SetForegroundColor(style.0),
                            crossterm::style::SetBackgroundColor(style.1),
                        )
                        .unwrap();
                        last_style = style;
                    }
                    queue!(out, Print(text)).unwrap();
                }
                x += 1;
            }
        }

        let cursor_pos = self.cursor.get();
        queue!(out, crossterm::cursor::MoveTo(cursor_pos.0, cursor_pos.1)).unwrap();
        queue!(out, crossterm::cursor::Show).unwrap();

        // Make sure everything is written out from the buffer.
        out.flush().unwrap();
    }

    pub(crate) fn draw(&self, x: usize, y: usize, text: &str, style: Style) {
        if y < self.height {
            let mut buf = self.buf.borrow_mut();
            let mut x = x;
            for g in text.graphemes(true) {
                if x < self.width {
                    let width = UnicodeWidthStr::width(g);
                    if width > 0 {
                        buf[y * self.width + x] = Some((style, g.into()));
                        x += 1;
                        for _ in 1..width {
                            if x < self.width {
                                buf[y * self.width + x] = None;
                            }
                            x += 1;
                        }
                    } else {
                        // If it's a zero-width character, prepend a space
                        // to give it width.  While this isn't strictly
                        // unicode compliant, it serves the purpose of this
                        // type of editor well by making all graphemes visible,
                        // even if they're otherwise illformed.
                        let mut graph = String::from(" ");
                        graph.push_str(g);
                        buf[y * self.width + x] = Some((style, graph));
                        x += 1;
                    }
                }
            }
        }
    }

    pub(crate) fn clear(&self, col: Color) {
        for cell in self.buf.borrow_mut().iter_mut() {
            match *cell {
                Some((ref mut style, ref mut text)) => {
                    *style = Style(col, col);
                    text.clear();
                    text.push(' ');
                }
                _ => {
                    *cell = Some((Style(col, col), " ".into()));
                }
            }
        }
    }

    pub fn set_cursor(&self, x: usize, y: usize) {
        self.cursor.set((
            x.min((self.width).saturating_sub(1)) as u16,
            y.min((self.height).saturating_sub(1)) as u16,
        ));
    }

    pub fn len(&self) -> usize {
        self.height * self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn cursor(&self) -> (usize, usize) {
        let (x, y) = self.cursor.get();
        (x as usize, y as usize)
    }

    pub fn line_offset(&self) -> usize {
        self.line_offset.get()
    }

    pub fn inc_offset(&self) {
        let offset = self.line_offset.get();
        self.line_offset.set(offset + 1);
    }

    pub fn dec_offset(&self) {
        let offset = self.line_offset.get();
        self.line_offset.set(offset.saturating_sub(1));
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        terminal::disable_raw_mode().unwrap();
        let mut out = self.out.borrow_mut();
        execute!(
            out,
            terminal::Clear(terminal::ClearType::All),
            crossterm::style::ResetColor,
            terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        )
        .unwrap();
        out.flush().unwrap();
    }
}
