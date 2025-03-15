use clap::Parser;
use crossterm::event::EnableMouseCapture;
use crossterm::event::{self, Event, KeyCode, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::block::Block;
use ratatui::Frame;

mod fileview;
use fileview::FileView;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct CliOpts {
    file: String,
}

fn main() {
    let cli = CliOpts::parse();
    let mut fileview = match FileView::open(&cli.file) {
        Ok(f) => f,
        Err(e) => panic!("Failed to open file '{}': {}", cli.file, e),
    };

    fileview.build_linemap();

    let mut terminal = ratatui::init();
    run(&mut terminal, &mut fileview, &cli);
    ratatui::restore();
}

enum Command {
    Idle,
    Cmd(String),
    Error(String),
}

struct UIState {
    num_lines: u64,
    cur_line: u64,
    cur_col: u64,
    filename: String,
    cmd: Command,
}

impl UIState {
    fn scroll_up(&mut self, amt: u64) {
        // Avoid underflow
        let newpos: u64 = if amt > self.cur_line {
            0
        } else {
            self.cur_line - amt
        };
        self.cur_line = newpos;
    }

    fn scroll_down(&mut self, amt: u64) {
        let newpos = std::cmp::min(self.cur_line + amt, self.num_lines - 1);
        self.cur_line = newpos;
    }
}

enum EventResult {
    Continue,
    Exit,
}

fn run(terminal: &mut ratatui::DefaultTerminal, fileview: &mut FileView, cli: &CliOpts) -> () {
    let mut ui = UIState {
        num_lines: fileview.num_lines,
        cur_line: 0,
        cur_col: 0,
        filename: cli.file.clone(),
        cmd: Command::Idle,
    };

    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    loop {
        terminal
            .draw(|f| render(f, fileview, &ui))
            .expect("failed to draw frame");

        let event = event::read().expect("failed to read event");
        match handle_event(&event, &mut ui) {
            EventResult::Exit => break,
            _ => {}
        };
    }
}

fn handle_event(event: &Event, ui: &mut UIState) -> EventResult {
    // Any keypress clears an error
    if let Command::Error(_) = ui.cmd {
        ui.cmd = Command::Idle;
    }

    match event {
        Event::Key(key) => match (key.code, &mut ui.cmd) {
            (KeyCode::Enter, Command::Cmd(cmd)) => {
                return parse_cmd(&cmd.clone(), ui);
            }
            (KeyCode::Char(':'), Command::Idle) => {
                ui.cmd = Command::Cmd(String::from(":"));
            }
            (KeyCode::Char(c), Command::Cmd(ref mut cmd)) => cmd.push(c),
            (KeyCode::Backspace, Command::Cmd(ref mut cmd)) => {
                cmd.pop();
            }
            (KeyCode::Esc, Command::Cmd(_)) => ui.cmd = Command::Idle,
            (KeyCode::Down, Command::Idle) => ui.scroll_down(1),
            (KeyCode::Up, Command::Idle) => ui.scroll_up(1),
            _ => {}
        },
        Event::Mouse(mouse) => match (mouse.kind, &mut ui.cmd) {
            (MouseEventKind::ScrollDown, Command::Idle) => ui.scroll_down(1),
            (MouseEventKind::ScrollUp, Command::Idle) => ui.scroll_up(1),
            _ => {}
        },
        _ => {}
    };
    return EventResult::Continue;
}

fn parse_cmd(cmd: &str, ui: &mut UIState) -> EventResult {
    match cmd {
        ":q" => return EventResult::Exit,
        _ => {
            ui.cmd = Command::Error(format!("Invalid command: '{}'", cmd));
            return EventResult::Continue;
        }
    }
}

fn render(frame: &mut Frame, fileview: &FileView, ui: &UIState) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Min(1), Constraint::Length(1)]);
    let [content_area, ui_area] = vertical.areas(frame.area());

    render_content(frame, content_area, ui, fileview);
    render_ui(frame, ui_area, ui);
}

fn render_content(frame: &mut Frame, rect: Rect, ui: &UIState, fileview: &FileView) {
    let view_col_begin: u64 = ui.cur_col;
    let view_col_end = view_col_begin + rect.width as u64;

    let view_line_begin = ui.cur_line;
    let view_line_end = view_line_begin + rect.height as u64;

    let mut lines: Vec<Line> = vec![];

    for line_idx in view_line_begin..view_line_end {
        if line_idx >= fileview.num_lines {
            break;
        }
        let line = Line::from(fileview.get_text(line_idx, view_col_begin, view_col_end));
        lines.push(line);
    }

    frame.render_widget(Text::from(lines), rect);
}

fn render_ui(frame: &mut Frame, rect: Rect, ui: &UIState) {
    use ratatui::style::Stylize;

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Fill(1), Constraint::Min(0)]);
    let [cmd_area, line_area] = horizontal.areas(rect);

    let block = Block::new().gray().on_dark_gray();

    // Line is 1-based in the UI
    let line_no = ui.cur_line + 1;
    let line_percent = line_no * 100 / ui.num_lines;
    let linedescr = format!(
        " {}% ☰ {}/{} ㏑:{} ",
        line_percent, line_no, ui.num_lines, ui.cur_col
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
