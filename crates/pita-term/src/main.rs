use std::{fs, io};
use std::cell::RefCell;
use std::io::stdout;
use std::panic::{set_hook, take_hook};

use crossterm::{event, execute, terminal};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use tree_sitter_highlight::{HighlightConfiguration, Highlighter, HighlightEvent};
use unicode_segmentation::UnicodeSegmentation;

use piece_table::PtBuffer;

use crate::hl::HlQueue;
use crate::screen::{Screen, Style};

mod screen;
mod cursor;
mod hl;

struct Editor<'a> {
    doc: PtBuffer<'a, String>,
    highlighter: Highlighter,
    rust_config: HighlightConfiguration,
    highlight: HlQueue,
    editor_screen: Screen,
    log_screen: Screen,
    log_buffer: RefCell<Vec<String>>,
    line_endings: Vec<usize>,
}

fn main() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen)?;
    // execute!(stdout(), event::EnableMouseCapture)?;

    let args: Vec<String> = std::env::args().collect();
    let file = fs::read_to_string(&args[1])?;
    let string = file.to_string();
    let graphemes = string.graphemes(true);
    let src: Vec<String> = graphemes
        .map(|s| s.to_string())
        .collect();

    let (width, height) = terminal::size()?;

    let log_screen_height = ((height as f32 / 100.0) * 10.0) as usize;
    let editor_height = ((height as f32 / 100.0) * 90.0) as usize;
    let width = width as usize;
    let offset_x = 0;
    let offset_y = 0;
    let log_buffer = RefCell::new(vec![
        format!("Terminal size ({width}, {height})"),
        format!("Editor dimension ({width}, {editor_height})"),
        format!("Log dimension ({width}, {log_screen_height})"),
    ]);

    let mut highlighter = Highlighter::new();
    use tree_sitter_highlight::HighlightConfiguration;
    let rust = tree_sitter_rust::language();
    let mut rust_config = HighlightConfiguration::new(
        rust,
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        "",
        "",
    ).unwrap();

    let hl_names: Vec<String> = rust_config.query.capture_names().iter()
        .map(|s| s.to_string())
        .collect();
    rust_config.configure(&hl_names);

    let doc = PtBuffer::new(&src);
    let doc_len = doc.len();

    init_panic_hook();

    let log_screen = Screen::new(width, log_screen_height, offset_x, editor_height, Color::Black)?;
    let editor_screen = Screen::new(width, editor_height, offset_x, offset_y, screen::DEFAULT_BG)?;
    let line_endings = Vec::with_capacity(editor_screen.size());
    let highlight = HlQueue::with_capacity(doc_len);

    let mut editor = Editor {
        doc: doc,
        highlighter,
        rust_config,
        highlight,
        editor_screen,
        log_screen,
        log_buffer,
        line_endings,
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
        self.update_highlights();
        self.draw_doc();
        self.draw_logs();

        let mut needs_redraw = false;

        loop {
            self.log(format!("{:?}", self.editor_screen.cursor()));
            if needs_redraw {
                self.editor_screen.clear(screen::DEFAULT_BG);
                self.update_highlights();
                self.log(format!("doc size {}", self.doc.len()));

                self.draw_doc();
            };
            self.log_screen.clear(Color::Black);
            self.draw_logs();

            self.log_screen.present();
            self.editor_screen.present();
            if let Some(command) = read_char()? {
                needs_redraw = match command {
                    Command::Quit => break,
                    Command::MoveLeft => self.cursor_left(),
                    Command::MoveRight => self.cursor_right(),
                    Command::MoveDown => self.cursor_down(),
                    Command::MoveUp => self.cursor_up(),
                    Command::NewLine => {
                        let idx = self.get_cursor_absolute_position();
                        self.doc.insert(idx, "\n".to_string());
                        self.cursor_next_line();
                        true
                    }
                    Command::Char(c) => {
                        let (x, y) = self.editor_screen.cursor();
                        let idx = self.doc.line_column_to_idx(x, y);
                        self.doc.insert(idx, c.to_string());
                        self.line_endings[y] += 1;
                        self.log(format!("ending {} cursor {:?}", self.line_endings[y], self.editor_screen.cursor()));

                        self.cursor_right();
                        true
                    }
                    Command::DeleteForward => {
                        let idx = self.get_cursor_absolute_position();
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
                        let (_, y) = self.editor_screen.cursor();
                        let idx = self.get_cursor_absolute_position();
                        self.doc.insert(idx, " ".to_string());
                        self.doc.insert(idx, " ".to_string());
                        self.line_endings[y] += 2;
                        self.cursor_right();
                        self.cursor_right();
                        true
                    }
                    Command::WordLeft => {
                        self.log(format!("cursor: {:?}", self.editor_screen.cursor()));
                        let idx = self.get_cursor_absolute_position();
                        let mut redraw = false;
                        if self.doc[idx.saturating_sub(1)] == " " {
                            for c in self.doc.rev_range(self.doc.len() - idx..self.doc.len()) {
                                if *c != " " {
                                    break;
                                }

                                redraw = redraw || self.cursor_left();
                            }
                        } else {
                            for c in self.doc.rev_range(self.doc.len() - idx..self.doc.len()) {
                                if *c == " " {
                                    break;
                                }

                                redraw = redraw || self.cursor_left();
                            }
                        }

                        redraw
                    }

                    Command::WordRight => {
                        let idx = self.get_cursor_absolute_position();
                        let mut redraw = false;
                        if self.doc[idx] == " " {
                            for c in self.doc.range(idx..) {
                                if *c != " " {
                                    break;
                                }

                                redraw = redraw || self.cursor_right();
                            }
                        } else {
                            for c in self.doc.range(idx..) {
                                if *c == " " {
                                    break;
                                }

                                redraw = redraw || self.cursor_right();
                            }
                        }

                        redraw
                    }

                    Command::Mouse(_) => {
                        false
                    }
                };
            }
        }

        Ok(())
    }

    fn log(&self, args: impl ToString) {
        self.log_buffer.borrow_mut().push(args.to_string())
    }

    fn update_highlights(&mut self) {
        let doc: Vec<&str> = self.doc.iter().map(|c| c.as_str()).collect();
        let string = doc.join("");
        let highlights = self.highlighter.highlight(
            &self.rust_config,
            string.as_bytes(),
            None,
            |_| None,
        ).unwrap();
        let mut next_hl = vec![];
        let mut next_range = vec![];
        self.highlight.clear();
        for event in highlights {
            match event.unwrap() {
                HighlightEvent::Source { start, end } => {
                    next_range.push((start, end));
                }
                HighlightEvent::HighlightStart(t) => {
                    next_hl.push(t);
                }
                HighlightEvent::HighlightEnd => {
                    if let Some((hl, (start, end))) = next_hl.pop().zip(next_range.pop()) {
                        self.highlight.push((start, end, hl.0));
                    }
                }
            }
        }
    }

    fn get_cursor_absolute_position(&self) -> usize {
        let (x, y) = self.editor_screen.cursor();
        self.doc.line_column_to_idx(x, y)
    }

    fn draw_logs(&mut self) {
        for (idx, log_line) in self.log_buffer.borrow().iter().rev().enumerate() {
            if idx > self.log_screen.height() {
                break;
            }

            let w = self.log_screen.width();
            let log_line = if log_line.len() > w {
                &log_line[..w]
            } else {
                &log_line
            };

            self.log_screen.draw(0, idx, &format!("{idx} - {log_line}"), Style(Color::Red, Color::Black));
        }
    }
    // Draw only a portion of the doc to fill the current screen
    fn draw_doc(&mut self) {
        let mut line_count = 0;
        let mut column_count = 0;
        let mut line_ending = 0;

        let start = self.doc.line_column_to_idx(0, self.editor_screen.line_offset());
        self.line_endings.clear();


        // Always push line offset - 1 ending in case we need to jump up a line without redrawing
        {
            self.log(format!("offset {}", self.editor_screen.line_offset()));

            let start = self.doc.line_column_to_idx(0, self.editor_screen.line_offset().saturating_sub(1));
            let end = self.doc.range(start..)
                .enumerate()
                .find(|(i, c)| *c == "\n")
                .map(|(i, c)| i)
                .unwrap_or_default();
            self.line_endings.push(end + 1);
        };

        let mut current_line = Vec::with_capacity(self.editor_screen.width());
        let mut current_hl: Option<usize> = self.highlight.get(start);
        let mut color = hl_to_color(current_hl);

        for (idx, byte) in self.doc.range(start..).enumerate() {
            if line_count > self.editor_screen.height() {
                break;
            }

            let next_hl: Option<usize> = self.highlight.get(start + idx);
            // push byte to the current line buffer
            current_line.push(byte);


            // if the current highlight changed, drain the line buffer
            // and write it to the screen
            if current_hl != next_hl {
                let text = current_line.drain(..);
                line_ending += text.len();

                let text: Vec<&str> = text.map(String::as_str).collect();
                let text = text.join("");
                self.editor_screen.draw(
                    column_count,
                    line_count,
                    &text,
                    Style(color, screen::DEFAULT_BG),
                );

                current_hl = next_hl;
                column_count += text.len();
                color = hl_to_color(current_hl);
            }


            if *byte == "\n" {
                let text = current_line.drain(..);
                line_ending += text.len();
                self.line_endings.push(line_ending);
                let text: Vec<&str> = text.map(String::as_str).collect();
                let text = text.join("");
                self.editor_screen.draw(
                    column_count,
                    line_count,
                    &text,
                    Style(Color::White, screen::DEFAULT_BG),
                );
                column_count = 0;
                line_ending = 0;
                line_count += 1;
                continue;
            }
        }

        self.log(format!("{:?}", self.line_endings));
    }
}


#[derive(Debug)]
enum Command {
    Quit,
    Char(char),
    MoveLeft,
    WordLeft,
    WordRight,
    MoveRight,
    MoveDown,
    MoveUp,
    NewLine,
    DeleteForward,
    DeleteBackWard,
    Tab,
    Mouse(MouseEvent),
}

fn read_char() -> io::Result<Option<Command>> {
    match event::read()? {
        Event::Key(e) => match e.kind {
            KeyEventKind::Press => Ok(Some(match e.code {
                KeyCode::Left if e.modifiers.contains(KeyModifiers::CONTROL) => Command::WordLeft,
                KeyCode::Left => Command::MoveLeft,
                KeyCode::Right if e.modifiers.contains(KeyModifiers::CONTROL) => Command::WordRight,
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
            })),
            _ => Ok(None),
        },
        Event::Mouse(event) => Ok(Some(Command::Mouse(event))),
        _ => Ok(None),
    }
}

fn hl_to_color(current_hl: Option<usize>) -> Color {
    match current_hl {
        Some(0) => Color::from((129, 200, 190)),// Types
        Some(10) => Color::from((239, 159, 118)),// Brackets
        Some(14) => Color::from((234, 153, 156)), // Keywords
        Some(11) => Color::from((231, 130, 132)), // Punctuation
        Some(13) => Color::from((244, 184, 228)), // Lifetime
        Some(4) => Color::from((229, 200, 144)), // Imported types
        Some(20) => Color::from((202, 158, 230)), // Ref + lifetime punct
        _ => Color::White,
    }
}

