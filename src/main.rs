use clap::Parser;
use crossterm::event::EnableMouseCapture;
use crossterm::event::{self, Event, KeyCode, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::block::Block;
use ratatui::Frame;

use std::fs::metadata;
use std::sync::Arc;
use std::thread;

mod file;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct CliOpts {
    file: String,
}

fn main() {
    let cli = CliOpts::parse();
    let file = match file::File::open(&cli.file) {
        Ok(f) => f,
        Err(e) => panic!("Failed to open file '{}': {}", cli.file, e),
    };

    let metadata = file::Metadata::new();

    launch_background_work(&file, &metadata);

    let mut terminal = ratatui::init();
    run(&mut terminal, &cli, &file, &metadata);
    ratatui::restore();
}

fn launch_background_work(file: &file::FilePtr, metadata: &file::MetadataPtr) {
    let file = Arc::clone(file);
    let metadata = Arc::clone(metadata);

    thread::spawn(move || {
        file.build_linemap(&metadata);
    });
}

enum Command {
    Idle,
    Cmd(String),
    Error(String),
}

struct UIState {
    cur_line: u64,
    cur_col: u64,
    filename: String,
    cmd: Command,
}

impl UIState {
    fn scroll_to_y(&mut self, metadata: &file::MetadataPtr, line: u64) {
        let metadata = metadata.lock().unwrap();
        let newpos = if metadata.num_lines > 0 {
            std::cmp::min(line, metadata.num_lines - 1)
        } else {
            0
        };
        self.cur_line = newpos;
    }

    fn scroll_up(&mut self, amt: u64) {
        // Avoid underflow
        let newpos: u64 = if amt > self.cur_line {
            0
        } else {
            self.cur_line - amt
        };
        self.cur_line = newpos;
    }

    fn scroll_down(&mut self, metadata: &file::MetadataPtr, amt: u64) {
        let metadata = metadata.lock().unwrap();
        let newpos = if metadata.num_lines > 0 {
            std::cmp::min(self.cur_line + amt, metadata.num_lines - 1)
        } else {
            0
        };
        self.cur_line = newpos;
    }

    fn scroll_left(&mut self, amt: u64) {
        // Avoid underflow
        let newpos: u64 = if amt > self.cur_col {
            0
        } else {
            self.cur_col - amt
        };
        self.cur_col = newpos;
    }

    fn scroll_right(&mut self, metadata: &file::MetadataPtr, amt: u64) {
        let metadata = metadata.lock().unwrap();
        let max_col = metadata.max_num_cols;
        let newpos = if max_col > 0 {
            std::cmp::min(self.cur_col + amt, max_col - 1)
        } else {
            0
        };
        self.cur_col = newpos;
    }
}

enum EventResult {
    Continue,
    Exit,
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    cli: &CliOpts,
    file: &file::FilePtr,
    metadata: &file::MetadataPtr,
) -> () {
    let mut ui = UIState {
        cur_line: 0,
        cur_col: 0,
        filename: cli.file.clone(),
        cmd: Command::Idle,
    };

    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    loop {
        terminal
            .draw(|f| render(f, file, metadata, &ui))
            .expect("failed to draw frame");

        if event::poll(std::time::Duration::from_millis(1000)).expect("failed to poll event") {
            let event = event::read().expect("failed to read event");
            match handle_event(&event, file, metadata, &mut ui) {
                EventResult::Exit => break,
                _ => {}
            };
        }
    }
}

fn handle_event(
    event: &Event,
    file: &file::FilePtr,
    metadata: &file::MetadataPtr,
    ui: &mut UIState,
) -> EventResult {
    // Any keypress clears an error
    if let Command::Error(_) = ui.cmd {
        ui.cmd = Command::Idle;
    }

    match event {
        Event::Key(key) => match (key.code, &mut ui.cmd) {
            (KeyCode::Enter, Command::Cmd(cmd)) => {
                return parse_cmd(&cmd.clone(), metadata, ui);
            }
            (KeyCode::Char(':'), Command::Idle) => {
                ui.cmd = Command::Cmd(String::from(":"));
            }
            (KeyCode::Char(c), Command::Cmd(ref mut cmd)) => cmd.push(c),
            (KeyCode::Backspace, Command::Cmd(ref mut cmd)) => {
                cmd.pop();
            }
            (KeyCode::Esc, Command::Cmd(_)) => ui.cmd = Command::Idle,
            (KeyCode::Up, _) => ui.scroll_up(1),
            (KeyCode::Down, _) => ui.scroll_down(metadata, 1),
            (KeyCode::Left, _) => ui.scroll_left(1),
            (KeyCode::Right, _) => ui.scroll_right(metadata, 1),
            _ => {}
        },
        Event::Mouse(mouse) => match (mouse.kind, &mut ui.cmd) {
            (MouseEventKind::ScrollUp, _) => ui.scroll_up(1),
            (MouseEventKind::ScrollDown, _) => ui.scroll_down(metadata, 1),
            (MouseEventKind::ScrollLeft, _) => ui.scroll_left(1),
            (MouseEventKind::ScrollRight, _) => ui.scroll_right(metadata, 1),
            _ => {}
        },
        _ => {}
    };
    return EventResult::Continue;
}

fn try_parse_lineno(cmd: &str) -> Option<u64> {
    if cmd.len() >= 2 {
        if let Ok(line) = cmd[1..].parse::<u64>() {
            return Some(line);
        }
    }
    return None;
}

fn parse_cmd(cmd: &str, metadata: &file::MetadataPtr, ui: &mut UIState) -> EventResult {
    if cmd == ":q" {
        return EventResult::Exit;
    } else if let Some(lineno) = try_parse_lineno(cmd) {
        // We present the line number as 1-based, but should allow :0 as input
        let lineno = std::cmp::max(lineno, 1) - 1;
        ui.scroll_to_y(metadata, lineno);
        ui.cmd = Command::Idle;
        return EventResult::Continue;
    } else {
        ui.cmd = Command::Error(format!("Invalid command: '{}'", cmd));
        return EventResult::Continue;
    }
}

fn render(frame: &mut Frame, file: &file::FilePtr, metadata: &file::MetadataPtr, ui: &UIState) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Min(1), Constraint::Length(1)]);
    let [content_area, ui_area] = vertical.areas(frame.area());

    render_content(frame, content_area, file, metadata, ui);
    render_ui(frame, ui_area, file, metadata, ui);
}

fn render_content(
    frame: &mut Frame,
    rect: Rect,
    file: &file::FilePtr,
    metadata: &file::MetadataPtr,
    ui: &UIState,
) {
    let view_col_begin: u64 = ui.cur_col;
    let view_col_end = view_col_begin + rect.width as u64;

    let view_line_begin = ui.cur_line;
    let view_line_end = view_line_begin + rect.height as u64;

    let mut lines: Vec<Line> = vec![];

    let metadata = metadata.lock().unwrap();
    for line_idx in view_line_begin..view_line_end {
        if line_idx >= metadata.num_lines {
            break;
        }
        let line = Line::from(file.get_text(&metadata, line_idx, view_col_begin, view_col_end));
        lines.push(line);
    }

    frame.render_widget(Text::from(lines), rect);
}

fn render_ui(
    frame: &mut Frame,
    rect: Rect,
    file: &file::FilePtr,
    metadata: &file::MetadataPtr,
    ui: &UIState,
) {
    use ratatui::style::Stylize;

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Fill(1), Constraint::Min(0)]);
    let [cmd_area, line_area] = horizontal.areas(rect);

    let block = Block::new().gray().on_dark_gray();
    let metadata = metadata.lock().unwrap();

    // Line and Col are 1-based in the UI
    let line_no = ui.cur_line + 1;
    let col_no = ui.cur_col + 1;

    let line_percent = if metadata.num_lines > 0 {
        line_no * 100 / metadata.num_lines
    } else {
        0
    };
    let linedescr = format!(
        " {}% ☰ {}/{} ㏑:{} ",
        line_percent, line_no, metadata.num_lines, col_no
    );
    let linedescr_text = Text::from(linedescr).right_aligned();

    let cmd_text = match &ui.cmd {
        Command::Idle => Text::from(ui.filename.clone()).italic(),
        Command::Cmd(cmd) => Text::from(cmd.clone()),
        Command::Error(err) => Text::from(err.clone()).yellow(),
    }
    .left_aligned();

    frame.render_widget(block, rect);
    frame.render_widget(cmd_text, cmd_area);
    frame.render_widget(linedescr_text, line_area);
}
