use std::{fs, io};
use std::cmp::min;
use std::io::stdout;
use std::panic::{set_hook, take_hook};

use crossterm::{event, execute};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use tracing_subscriber::{EnvFilter, fmt};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use piece_table::PtBuffer;

use crate::screen::{Screen, Style};

mod screen;

struct Editor<'a> {
    doc: PtBuffer<'a, u8>,
    screen: Screen,
    line_endings: Vec<usize>,
}

fn main() -> io::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    init_panic_hook();

    let args: Vec<String> = std::env::args().collect();
    let file = fs::read_to_string(&args[1])?;
    let mut editor = Editor {
        doc: PtBuffer::new(file.as_bytes()),
        screen: Screen::new()?,
        line_endings: vec![],
    };

    editor.main_loop()
}

pub fn init_panic_hook() {
    let original_hook = take_hook();
    set_hook(Box::new(move |panic_info| {
        disable_raw_mode().unwrap();
        execute!(stdout(), LeaveAlternateScreen).unwrap();
        original_hook(panic_info);
    }));
}

impl Editor<'_> {
    fn main_loop(&mut self) -> io::Result<()> {
        self.draw_doc();

        self.screen.draw(0, self.screen.height() - 1, &format!("offset: {}", self.screen.line_offset()), Style(Color::White, Color::Blue));

        let mut needs_redraw = false;

        loop {
            if needs_redraw {
                self.screen.clear(screen::DEFAULT_BG);
                self.draw_doc();
            };

            self.screen.draw(0, self.screen.height() - 1, &format!("offset: {}", self.screen.line_offset()), Style(Color::White, Color::Blue));
            self.screen.present();
            if let Some(command) = read_char()? {
                needs_redraw = match command {
                    Command::Quit => break,
                    Command::MoveLeft => self.cursor_left(),
                    Command::MoveRight => self.cursor_right(),
                    Command::MoveDown => self.cursor_down(),
                    Command::MoveUp => self.cursor_up(),
                    Command::NewLine => {
                        let idx = self.get_cursor_absolute_position();
                        self.doc.insert(idx, b'\n');
                        self.cursor_next_line();
                        true
                    }
                    Command::Char(c) => {
                        let idx = self.get_cursor_absolute_position();
                        self.cursor_right();
                        self.doc.insert(idx, c as u8);
                        true
                    }
                    Command::DeleteForward => {
                        let idx = self.get_cursor_absolute_position();
                        self.doc[idx];
                        self.doc.remove(idx);

                        true
                    }
                    Command::DeleteBackWard => {
                        let idx = self.get_cursor_absolute_position();
                        if idx == 0 {
                            continue;
                        }

                        self.doc.remove(idx - 1);
                        self.cursor_left();

                        true
                    }
                    Command::Tab => {
                        let idx = self.get_cursor_absolute_position();
                        self.doc.insert(idx, b' ');
                        self.doc.insert(idx, b' ');

                        true
                    }
                    Command::WordLeft => {
                        let idx = self.get_cursor_absolute_position();
                        let mut redraw = false;
                        let mut cs = vec![];
                        let idx = self.doc.len() - idx;
                        for c in self.doc.iter().rev() {
                            redraw = redraw && self.cursor_left();
                            cs.push(*c);

                            if *c == b' ' {
                                break
                            }
                        }

                        let cow = String::from_utf8_lossy(&cs);

                        panic!("{cow}");
                        redraw
                    }
                };
            }
        }


        Ok(())
    }

    fn get_cursor_absolute_position(&self) -> usize {
        let (x, y) = self.screen.cursor();
        let idx = self.doc.line_column_to_idx(x, y);
        idx
    }

    fn draw_doc(&mut self) {
        let mut line_count = 0;
        let mut column_count = 0;
        let start = self.doc.line_column_to_idx(0, self.screen.line_offset());
        self.line_endings.clear();

        let mut line = Vec::with_capacity(self.screen.width());
        for byte in self.doc.range(start..self.screen.len()) {
            if line_count > self.screen.height() {
                break;
            }


            if *byte == b'\n' {
                self.line_endings.push(line.len() + 1);
                column_count = 0;
                self.screen.draw(column_count, line_count, &String::from_utf8_lossy(line.as_slice()), Style(Color::White, screen::DEFAULT_BG));
                line.clear();
                line_count += 1;
                continue;
            }

            line.push(*byte);
            column_count += 1;
        }
    }

    pub(crate) fn cursor_left(&self) -> bool {
        let (x, y) = self.screen.cursor();
        if x == 0 {
            let y = y.saturating_sub(1);
            let x = self.line_endings[y];
            self.screen.set_cursor(x.saturating_sub(1), y);
            true
        } else {
            self.screen.set_cursor(x.saturating_sub(1), y);
            false
        }
    }

    pub(crate) fn cursor_right(&self) -> bool {
        let (x, y) = self.screen.cursor();
        if x >= self.line_endings[y] - 1 {
            let y = y + 1;
            let x = 0;
            self.screen.set_cursor(x, y);
            true
        } else {
            self.screen.set_cursor(x + 1, y);
            false
        }
    }

    pub(crate) fn cursor_next_line(&self) -> bool {
        let (_, y) = self.screen.cursor();
        self.screen.set_cursor(0, y + 1);
        true
    }

    pub(crate) fn cursor_down(&self) -> bool {
        let (mut x, mut y) = self.screen.cursor();
        y = y + 1;
        let ending = self.line_endings[min(y, self.screen.height() - 1)];
        if x > ending {
            x = ending - 1;
        }

        let redraw = if y > self.screen.height() - 1 {
            self.screen.inc_offset();
            true
        } else {
            false
        };

        self.screen.set_cursor(x, y);

        redraw
    }

    pub(crate) fn cursor_up(&self) -> bool {
        let (mut x, mut y) = self.screen.cursor();
        y = y.saturating_sub(1);
        let ending = self.line_endings[min(y, self.screen.height() - 1)];
        if x > ending {
            x = ending - 1;
        }

        let redraw = if y == 0 {
            self.screen.dec_offset();
            true
        } else {
            false
        };

        self.screen.set_cursor(x, y);
        redraw
    }
}


enum Command {
    Quit,
    Char(char),
    MoveLeft,
    WordLeft,
    MoveRight,
    MoveDown,
    MoveUp,
    NewLine,
    DeleteForward,
    DeleteBackWard,
    Tab,
}

fn read_char() -> io::Result<Option<Command>> {
    match event::read()? {
        Event::Key(e) => {
            match e.kind {
                KeyEventKind::Press => {
                    Ok(Some(match e.code {
                        KeyCode::Left if e.modifiers.contains(KeyModifiers::CONTROL) => Command::WordLeft,
                        KeyCode::Left => Command::MoveLeft,
                        KeyCode::Right => Command::MoveRight,
                        KeyCode::Up => Command::MoveUp,
                        KeyCode::Down => Command::MoveDown,
                        KeyCode::Esc => Command::Quit,
                        KeyCode::Char(c) => Command::Char(c),
                        KeyCode::Enter => Command::NewLine,
                        KeyCode::Delete => Command::DeleteForward,
                        KeyCode::Backspace => Command::DeleteBackWard,
                        KeyCode::Tab => Command::Tab,
                        _ => return Ok(None),
                    }))
                }
                _ => return Ok(None)
            }
        }
        _ => return Ok(None)
    }
}