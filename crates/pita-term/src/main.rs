// Don't loose hope, you can do this!
// fix the movement command (l,r,u,d + wl/wr) [Done]
// stabilize insertion + deleletion
// display log [Done]
// File sync (maybe needs an rw lock on the PtBuffer)
// Clip board
// Treesitter task
// Floating window (for autocompletion, then any plugin)
// Multi buffer
// LSP
//          - code action
//          - inlay hints
//          - runnables
// TODO: Plugins wasm runtime + API

use std::cell::RefCell;
use std::io::stdout;
use std::panic::{set_hook, take_hook};
use std::time::Duration;
use std::{fs, io};

use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers, MouseEvent};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use futures::{join, FutureExt, StreamExt};
use futures_timer::Delay;
use tokio::select;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};
use unicode_segmentation::UnicodeSegmentation;

use piece_table::PtBuffer;

use crate::hl::HlQueue;
use crate::screen::{Screen, Style};

mod cursor;
mod hl;
mod screen;

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

#[tokio::main]
async fn main() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen)?;
    // execute!(stdout(), event::EnableMouseCapture)?;

    let args: Vec<String> = std::env::args().collect();
    let path = args[1].clone();

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(32);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let (hl_tx, hl_rx) = tokio::sync::mpsc::channel(32);
    let event_handler = tokio::spawn(handle_events(command_tx, shutdown_tx.clone()));
    let command_handler = tokio::spawn(handle_command(path, command_rx, hl_tx, shutdown_tx));
    let hl_handler = tokio::spawn(handle_highlight(hl_rx));

    let _ = join!(event_handler, command_handler, hl_handler);

    Ok(())
}

async fn handle_highlight(hl_rx: tokio::sync::mpsc::Receiver<()>) {}

pub fn init_panic_hook() {
    let original_hook = take_hook();
    set_hook(Box::new(move |panic_info| {
        disable_raw_mode().unwrap();
        execute!(stdout(), LeaveAlternateScreen).unwrap();
        original_hook(panic_info);
    }));
}

impl Editor<'_> {
    fn log(&self, args: impl ToString) {
        self.log_buffer.borrow_mut().push(args.to_string())
    }

    fn update_highlights(&mut self) {
        let doc: Vec<&str> = self.doc.iter().map(|c| c.as_str()).collect();
        let string = doc.join("");
        let highlights = self
            .highlighter
            .highlight(&self.rust_config, string.as_bytes(), None, |_| None)
            .unwrap();
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
        let y = y + self.editor_screen.line_offset();
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

            self.log_screen.draw(
                0,
                idx,
                &format!("{idx} - {log_line}"),
                Style(Color::Red, Color::Black),
            );
        }
    }
    // Draw only a portion of the doc to fill the current screen
    fn draw_doc(&mut self) {
        let mut line_count = 0;
        let mut column_count = 0;
        let mut line_ending = 0;

        let start = self
            .doc
            .line_column_to_idx(0, self.editor_screen.line_offset());
        self.line_endings.clear();

        // Always push line offset - 1 ending in case we need to jump up a line without redrawing
        {
            let start = self
                .doc
                .line_column_to_idx(0, self.editor_screen.line_offset().saturating_sub(1));

            let end = self
                .doc
                .range(start..)
                .enumerate()
                .find(|(_i, c)| *c == "\n")
                .map(|(i, _c)| i)
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

async fn handle_events(
    tx: tokio::sync::mpsc::Sender<Command>,
    shutdown_rx: tokio::sync::broadcast::Sender<()>,
) {
    let mut stream = EventStream::new();

    loop {
        let delay = Delay::new(Duration::from_millis(1_000)).fuse();
        let event = stream.next().fuse();
        let mut shutdown = shutdown_rx.subscribe();

        select! {
            _ = delay => {},
            maybe_shutdown = shutdown.recv() => if let Ok(()) = maybe_shutdown {
                break;
            },
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(Event::Key(e))) => {
                        match e.code {
                            KeyCode::Char(c) => {
                                tx.send(Command::Char(c)).await.unwrap();
                            }
                            KeyCode::Esc => {
                                tx.send(Command::Quit).await.unwrap();
                            }
                            KeyCode::Left if e.modifiers.contains(KeyModifiers::CONTROL) => {
                                tx.send(Command::WordLeft).await.unwrap()
                            }
                            KeyCode::Left => {
                                tx.send(Command::MoveLeft).await.unwrap()
                            }
                            KeyCode::Right if e.modifiers.contains(KeyModifiers::CONTROL) => {
                                tx.send(Command::WordRight).await.unwrap()
                            }
                            KeyCode::Right => {
                                tx.send(Command::MoveRight).await.unwrap()
                            }
                            KeyCode::Up => {
                                tx.send(Command::MoveUp).await.unwrap()
                            }
                            KeyCode::Down => {
                                tx.send(Command::MoveDown).await.unwrap()
                            }
                            KeyCode::Enter => {
                                tx.send(Command::NewLine).await.unwrap()
                            }
                            KeyCode::Tab => {
                                tx.send(Command::Tab).await.unwrap()
                            }
                            KeyCode::Delete => {
                                tx.send(Command::DeleteForward).await.unwrap()
                            }
                            KeyCode::Backspace => {
                                tx.send(Command::DeleteBackWard).await.unwrap()
                            }
                            _ => {}
                        }
                    },
                    Some(Ok(e)) => {
                        println!("{e:?}");
                    }
                    Some(Err(e)) => println!("Error: {:?}\r", e),
                    None => {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_command(
    path: String,
    mut rx: tokio::sync::mpsc::Receiver<Command>,
    _hl_event: tokio::sync::mpsc::Sender<()>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
) -> io::Result<()> {
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

    init_panic_hook();

    let log_screen = Screen::new(
        width,
        log_screen_height,
        offset_x,
        editor_height,
        Color::Black,
    )?;
    let editor_screen = Screen::new(width, editor_height, offset_x, offset_y, screen::DEFAULT_BG)?;
    let file = fs::read_to_string(&path)?;
    let string = file.to_string();
    let graphemes = string.graphemes(true);
    let src: Vec<String> = graphemes.map(|s| s.to_string()).collect();
    let doc = PtBuffer::new(&src);
    let doc_len = doc.len();
    let highlight = HlQueue::with_capacity(doc_len);
    let line_endings = Vec::with_capacity(editor_screen.size());
    let highlighter = Highlighter::new();
    let rust = tree_sitter_rust::language();
    let mut rust_config =
        HighlightConfiguration::new(rust, "rust", tree_sitter_rust::HIGHLIGHTS_QUERY, "", "")
            .unwrap();

    let hl_names: Vec<String> = rust_config
        .query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    rust_config.configure(&hl_names);

    let mut editor = Editor {
        doc,
        highlighter,
        rust_config,
        highlight,
        editor_screen,
        log_screen,
        log_buffer,
        line_endings,
    };

    editor.draw_doc();
    editor.draw_logs();
    editor.editor_screen.present();

    while let Some(message) = rx.recv().await {
        let redraw = match message {
            Command::Quit => {
                shutdown_tx.send(()).unwrap();
                break;
            }

            Command::Char(c) => {
                let pos = editor.get_cursor_absolute_position();
                editor.doc.insert(pos, c.to_string());
                editor.cursor_right();
                true
            }
            Command::MoveLeft => editor.cursor_left(),
            Command::WordLeft => {
                let pos = editor.get_cursor_absolute_position();
                let c = &editor.doc[pos.saturating_sub(1)];
                if c == " " || c == "\n" {
                    for c in editor
                        .doc
                        .rev_range(editor.doc.len() - pos..editor.doc.len())
                    {
                        if c != " " && c != "\n" {
                            break;
                        }

                        editor.cursor_left();
                    }
                } else {
                    for c in editor
                        .doc
                        .rev_range(editor.doc.len() - pos..editor.doc.len())
                    {
                        if c == " " || c == "\n" {
                            break;
                        }
                        editor.cursor_left();
                    }
                }

                false
            }
            Command::WordRight => {
                let pos = editor.get_cursor_absolute_position();
                let c = &editor.doc[pos];
                if c == " " || c == "\n" {
                    for c in editor.doc.range(pos..) {
                        if c != " " && c != "\n" {
                            break;
                        }

                        editor.cursor_right();
                    }
                } else {
                    for c in editor.doc.range(pos..) {
                        if c == " " || c == "\n" {
                            break;
                        }
                        editor.cursor_right();
                    }
                }

                false
            }
            Command::MoveRight => editor.cursor_right(),
            Command::MoveDown => editor.cursor_down(),
            Command::MoveUp => editor.cursor_up(),
            Command::NewLine => {
                let pos = editor.get_cursor_absolute_position();
                editor.doc.insert(pos, "\n".to_string());
                true
            }
            // FIXME
            Command::DeleteForward => {
                let pos = editor.get_cursor_absolute_position();
                editor.log(format!("del at {pos}"));
                editor.doc.remove(pos);
                true
            }

            Command::DeleteBackWard => {
                editor.cursor_left();
                let pos = editor.get_cursor_absolute_position();
                editor.doc.remove(pos);
                true
            }
            Command::Tab => todo!(),
            Command::Mouse(_) => todo!(),
        };

        if redraw {
            editor.editor_screen.clear(Color::DarkYellow);
            editor.draw_doc();
        }

        editor.log_screen.clear(Color::Black);
        editor.draw_logs();
        editor.log_screen.present();
        editor.editor_screen.present();
    }

    Ok(())
}

fn hl_to_color(current_hl: Option<usize>) -> Color {
    match current_hl {
        Some(0) => Color::from((129, 200, 190)),  // Types
        Some(10) => Color::from((239, 159, 118)), // Brackets
        Some(14) => Color::from((234, 153, 156)), // Keywords
        Some(11) => Color::from((231, 130, 132)), // Punctuation
        Some(13) => Color::from((244, 184, 228)), // Lifetime
        Some(4) => Color::from((229, 200, 144)),  // Imported types
        Some(20) => Color::from((202, 158, 230)), // Ref + lifetime punct
        _ => Color::White,
    }
}
