//! Grove TUI dashboard — public entry point.
//!
//! `grove dashboard` launches a full-screen ratatui TUI showing live agent
//! status, events feed, and mail panel. Data is polled from the same SQLite
//! databases used by all other grove commands.

pub mod app;
pub mod theme;
pub mod views;
pub mod widgets;

use std::io;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

/// Launch the interactive TUI dashboard.
///
/// Sets up the terminal (raw mode + alternate screen), runs the event loop,
/// and restores the terminal on exit — including on panic.
pub fn launch_dashboard(project_root: &str) -> anyhow::Result<()> {
    // Install a panic hook that restores the terminal before printing the
    // panic message, so the terminal is not left in a broken state.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(project_root);
    app.refresh_all();

    let tick_rate = std::time::Duration::from_millis(1000);
    let mut last_tick = std::time::Instant::now();

    loop {
        terminal.draw(|f| views::render(f, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = std::time::Instant::now();
        }

        if !app.running {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
