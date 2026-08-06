mod app_state;
mod error;
mod gpu;
mod influx_local;
mod keybinds;
mod system_monitor;
mod ui;
mod utils;

use crate::error::NviError;
use crate::keybinds::{KeybindAggregate, PendingOp};
use crate::ui::render::ui;
use crate::ui::widgets::help_max_scroll;
use crate::utils::system::kill_selected_process;
use app_state::AppState;
use clap::{Arg, Command};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use nvml::Nvml;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::stdout;
use std::time::Duration;

pub const POLLING_TIMEOUT_MS: u64 = 100;

pub(crate) type Result<T> = core::result::Result<T, NviError>;

fn main() -> Result<()> {
    let matches = Command::new("nviwatch")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Manpreet Singh")
        .about("NviWatch: A blazingly fast rust based TUI for managing and monitoring NVIDIA GPU processes")
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .value_name("MILLISECONDS")
                .help("Refresh interval in milliseconds")
                .default_value("300")
                .required(false),
        )
        .arg(
            Arg::new("tabbed-graphs")
                .short('t')
                .long("tabbed-graphs")
                .help("Display GPU graphs in tabbed view")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("bar-chart")
                .short('b')
                .long("bar-chart")
                .help("Display GPU graphs as bar charts")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("cpu")
                .short('c')
                .long("cpu")
                .help("Enable CPU and system-wide process monitoring (CPU panels, system process tray, sort with 's', and system metrics streaming)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("influx-url")
                .long("influx-url")
                .value_name("URL")
                .help("InfluxDB URL")
                .required(false),
        )
        .arg(
            Arg::new("influx-org")
                .long("influx-org")
                .value_name("ORG")
                .help("InfluxDB organization")
                .required(false),
        )
        .arg(
            Arg::new("influx-bucket")
                .long("influx-bucket")
                .value_name("BUCKET")
                .help("InfluxDB bucket")
                .required(false),
        )
        .arg(
            Arg::new("influx-token")
                .long("influx-token")
                .value_name("TOKEN")
                .help("InfluxDB token")
                .required(false),
        )
        .get_matches();

    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse().expect("Invalid number"))
        .unwrap_or(300);

    let nvml = Nvml::init()?;

    // Create the runtime before raw/alternate screen so a failure never wedges the terminal.
    let runtime = tokio::runtime::Runtime::new()?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = AppState::from(&matches);

    loop {
        if app_state.should_update(watch_interval) {
            app_state.update(&nvml, &matches, &runtime)?;
        }

        terminal.draw(|f| ui(f, &app_state))?;

        // Most of the key event handling probably better off being moved to an actual event key handler
        // and we just ask it to 'start' at the start of program & handle it
        // via messages etc.

        if event::poll(Duration::from_millis(POLLING_TIMEOUT_MS))?
            && let Event::Key(key) = event::read()?
        {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

            if app_state.show_help {
                // Clamp using last drawn size approximation: terminal size from crossterm
                // isn't fetched here; help_max_scroll uses a Rect we rebuild from poll size.
                let size = terminal.size()?;
                let help_area = ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: size.width,
                    height: size.height,
                };
                let max_scroll = help_max_scroll(help_area, app_state.cpu_monitoring);
                let page = help_area.height.saturating_sub(2).max(1);

                match key.code {
                    KeyCode::Char('?') | KeyCode::Esc => {
                        app_state.show_help = false;
                        app_state.help_scroll = 0;
                    }
                    KeyCode::Char('q') => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        app_state.help_scroll = app_state.help_scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app_state.help_scroll = (app_state.help_scroll + 1).min(max_scroll);
                    }
                    KeyCode::Char('p') if ctrl => {
                        app_state.help_scroll = app_state.help_scroll.saturating_sub(1);
                    }
                    KeyCode::Char('n') if ctrl => {
                        app_state.help_scroll = (app_state.help_scroll + 1).min(max_scroll);
                    }
                    KeyCode::PageUp => {
                        app_state.help_scroll = app_state.help_scroll.saturating_sub(page);
                    }
                    KeyCode::PageDown => {
                        app_state.help_scroll =
                            (app_state.help_scroll.saturating_add(page)).min(max_scroll);
                    }
                    KeyCode::Home | KeyCode::Char('g') if !ctrl => {
                        // Single g / Home → top of help (gg chord not needed here).
                        app_state.help_scroll = 0;
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        app_state.help_scroll = max_scroll;
                    }
                    _ => {}
                }
                // Keep scroll valid if the window was resized smaller.
                app_state.help_scroll = app_state.help_scroll.min(max_scroll);
                continue;
            }

            // Chord second keys are handled below; anything else clears pending.
            let is_chord_second = matches!(
                (app_state.pending_op, key.code, ctrl),
                (PendingOp::GoTop, KeyCode::Char('g'), false)
                    | (PendingOp::Kill, KeyCode::Char('d'), false)
            );
            if app_state.pending_op.is_pending() && !is_chord_second {
                // Still allow starting a different chord / dedicated keys below;
                // clear first so stale pending does not stick across unrelated keys.
                if !matches!(
                    (key.code, ctrl),
                    (KeyCode::Char('g'), false) | (KeyCode::Char('d'), false)
                ) {
                    app_state.pending_op = PendingOp::None;
                }
            }

            if let Some(nav) = KeybindAggregate::try_from(&key).ok() {
                app_state.pending_op = PendingOp::None;
                match nav {
                    KeybindAggregate::Up => {
                        if app_state.selected_process > 0 {
                            app_state.selected_process -= 1;
                        }
                    }
                    KeybindAggregate::Down => {
                        let total = app_state.total_process_count();
                        if total > 0 && app_state.selected_process < total - 1 {
                            app_state.selected_process += 1;
                        }
                    }
                    KeybindAggregate::Left => {
                        if app_state.use_tabbed_graphs && app_state.selected_gpu_tab > 0 {
                            app_state.selected_gpu_tab -= 1;
                        }
                    }
                    KeybindAggregate::Right => {
                        // `+ 1 <` rather than `< len() - 1` so we never underflow
                        // when gpu_infos is empty.
                        if app_state.use_tabbed_graphs
                            && app_state.selected_gpu_tab + 1 < app_state.gpu_infos.len()
                        {
                            app_state.selected_gpu_tab += 1;
                        }
                    }
                }
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('?') => {
                    app_state.pending_op = PendingOp::None;
                    app_state.help_scroll = 0;
                    app_state.show_help = true;
                }
                KeyCode::Char('g') if !ctrl => {
                    if app_state.pending_op == PendingOp::GoTop {
                        app_state.selected_process = 0;
                        app_state.pending_op = PendingOp::None;
                    } else {
                        app_state.pending_op = PendingOp::GoTop;
                    }
                }
                KeyCode::Char('G') => {
                    app_state.pending_op = PendingOp::None;
                    let total = app_state.total_process_count();
                    if total > 0 {
                        app_state.selected_process = total - 1;
                    }
                }
                KeyCode::Char('d') if ctrl => {
                    // Ctrl+d - default mode
                    app_state.pending_op = PendingOp::None;
                    app_state.use_tabbed_graphs = false;
                    app_state.use_bar_charts = false;
                }
                KeyCode::Char('x') => {
                    // Single-key kill (primary); same target as dd.
                    app_state.pending_op = PendingOp::None;
                    kill_highlighted_process(&mut app_state);
                }
                KeyCode::Char('d') => {
                    // dd - kill selected process (first d arms pending)
                    if app_state.pending_op == PendingOp::Kill {
                        app_state.pending_op = PendingOp::None;
                        kill_highlighted_process(&mut app_state);
                    } else {
                        app_state.pending_op = PendingOp::Kill;
                    }
                }
                KeyCode::Char('t') => {
                    app_state.pending_op = PendingOp::None;
                    app_state.use_tabbed_graphs = true;
                    app_state.use_bar_charts = false;
                }
                KeyCode::Char('b') => {
                    app_state.pending_op = PendingOp::None;
                    app_state.use_tabbed_graphs = false;
                    app_state.use_bar_charts = true;
                }
                KeyCode::Char('s') if app_state.cpu_monitoring && !ctrl => {
                    app_state.pending_op = PendingOp::None;
                    app_state.cycle_sort_mode();
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Kill the process under the current selection (active tray order). Surfaces a UI
/// error when the selection is stale (e.g. the process exited between frames).
fn kill_highlighted_process(app_state: &mut AppState) {
    match app_state.selected_kill_target() {
        Some((pid, command)) => {
            // Clone command: kill_selected_process takes &str but we drop the
            // borrow before mutating error_message.
            let command = command.to_string();
            if let Err(e) = kill_selected_process(pid, &command) {
                app_state.error_message = Some(e.to_string());
            }
        }
        None => {
            app_state.error_message = Some("Selected process not found".to_string());
        }
    }
}
