//! Test the PTY terminal concept standalone
//!
//! Run with: cargo run --example terminal_test
//!
//! This demonstrates a fully interactive terminal running inside a terminal.
//! Press Escape to exit.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    Terminal,
};
use tokio::sync::mpsc;

fn main() -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let size = terminal.size()?;
    let rows = size.height.saturating_sub(2);
    let cols = size.width.saturating_sub(2);

    // Create PTY
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Spawn bash
    let mut cmd = CommandBuilder::new("bash");
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd)?;

    // Get reader/writer
    let reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;

    // Create vt100 parser
    let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));

    // Channel for output notification
    let (tx, mut rx) = mpsc::unbounded_channel::<()>();

    // Spawn reader thread
    let parser_clone = Arc::clone(&parser);
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut p) = parser_clone.lock() {
                        p.process(&buf[..n]);
                    }
                    let _ = tx.send(());
                }
                Err(_) => break,
            }
        }
    });

    let accent = Color::Rgb(203, 166, 247); // Mauve

    // Main loop
    loop {
        // Drain output notifications
        while rx.try_recv().is_ok() {}

        // Check if child exited
        if let Ok(Some(_status)) = child.try_wait() {
            break;
        }

        // Draw
        terminal.draw(|f| {
            let area = f.area();
            let buf = f.buffer_mut();

            // Clear
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.reset();
                    }
                }
            }

            // Draw border
            render_border(buf, area, accent);

            // Draw terminal content
            if let Ok(p) = parser.lock() {
                let screen = p.screen();
                let content_area = Rect::new(
                    area.x + 1,
                    area.y + 1,
                    area.width.saturating_sub(2),
                    area.height.saturating_sub(2),
                );
                render_screen(buf, content_area, screen);
            }
        })?;

        // Handle input
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    // Escape to quit
                    if key.code == KeyCode::Esc {
                        break;
                    }

                    // Convert key to bytes and send to PTY
                    let bytes: Vec<u8> = match key.code {
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                vec![(c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1)]
                            } else if key.modifiers.contains(KeyModifiers::ALT) {
                                vec![0x1b, c as u8]
                            } else {
                                c.to_string().into_bytes()
                            }
                        }
                        KeyCode::Enter => vec![b'\r'],
                        KeyCode::Backspace => vec![0x7f],
                        KeyCode::Tab => vec![b'\t'],
                        KeyCode::Up => b"\x1b[A".to_vec(),
                        KeyCode::Down => b"\x1b[B".to_vec(),
                        KeyCode::Right => b"\x1b[C".to_vec(),
                        KeyCode::Left => b"\x1b[D".to_vec(),
                        KeyCode::Home => b"\x1b[H".to_vec(),
                        KeyCode::End => b"\x1b[F".to_vec(),
                        KeyCode::PageUp => b"\x1b[5~".to_vec(),
                        KeyCode::PageDown => b"\x1b[6~".to_vec(),
                        KeyCode::Delete => b"\x1b[3~".to_vec(),
                        _ => vec![],
                    };

                    if !bytes.is_empty() {
                        writer.write_all(&bytes)?;
                        writer.flush()?;
                    }
                }
                _ => {}
            }
        }
    }

    // Cleanup
    let _ = child.kill();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    println!("Terminal session ended.");
    Ok(())
}

fn render_border(buf: &mut Buffer, area: Rect, color: Color) {
    // Top border
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_char('┏');
        cell.set_fg(color);
    }
    for x in (area.x + 1)..(area.x + area.width - 1) {
        if let Some(cell) = buf.cell_mut((x, area.y)) {
            cell.set_char('━');
            cell.set_fg(color);
        }
    }
    if let Some(cell) = buf.cell_mut((area.x + area.width - 1, area.y)) {
        cell.set_char('┓');
        cell.set_fg(color);
    }

    // Side borders
    for y in (area.y + 1)..(area.y + area.height - 1) {
        if let Some(cell) = buf.cell_mut((area.x, y)) {
            cell.set_char('┃');
            cell.set_fg(color);
        }
        if let Some(cell) = buf.cell_mut((area.x + area.width - 1, y)) {
            cell.set_char('┃');
            cell.set_fg(color);
        }
    }

    // Bottom border
    if let Some(cell) = buf.cell_mut((area.x, area.y + area.height - 1)) {
        cell.set_char('┗');
        cell.set_fg(color);
    }
    for x in (area.x + 1)..(area.x + area.width - 1) {
        if let Some(cell) = buf.cell_mut((x, area.y + area.height - 1)) {
            cell.set_char('━');
            cell.set_fg(color);
        }
    }
    if let Some(cell) = buf.cell_mut((area.x + area.width - 1, area.y + area.height - 1)) {
        cell.set_char('┛');
        cell.set_fg(color);
    }

    // Title
    let title = " Terminal (ESC to exit) ";
    for (i, ch) in title.chars().enumerate() {
        let x = area.x + 2 + i as u16;
        if x < area.x + area.width - 2 {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_char(ch);
                cell.set_fg(Color::Rgb(205, 214, 244));
            }
        }
    }
}

fn render_screen(buf: &mut Buffer, area: Rect, screen: &vt100::Screen) {
    let (screen_rows, screen_cols) = screen.size();

    for row in 0..area.height.min(screen_rows) {
        for col in 0..area.width.min(screen_cols) {
            let x = area.x + col;
            let y = area.y + row;

            if let Some(cell) = screen.cell(row, col) {
                let contents = cell.contents();
                let ch = contents.chars().next().unwrap_or(' ');

                // Convert colors
                let fg = convert_color(cell.fgcolor());
                let bg = convert_color(cell.bgcolor());

                let mut modifiers = Modifier::empty();
                if cell.bold() {
                    modifiers |= Modifier::BOLD;
                }
                if cell.italic() {
                    modifiers |= Modifier::ITALIC;
                }
                if cell.underline() {
                    modifiers |= Modifier::UNDERLINED;
                }

                let style = Style::default().fg(fg).bg(bg).add_modifier(modifiers);

                if let Some(buf_cell) = buf.cell_mut((x, y)) {
                    buf_cell.set_char(ch);
                    buf_cell.set_style(style);
                }
            }
        }
    }

    // Draw cursor
    let (cursor_row, cursor_col) = screen.cursor_position();
    let cursor_x = area.x + cursor_col;
    let cursor_y = area.y + cursor_row;

    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        if let Some(cell) = buf.cell_mut((cursor_x, cursor_y)) {
            cell.set_bg(Color::Rgb(203, 166, 247));
            cell.set_fg(Color::Black);
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
