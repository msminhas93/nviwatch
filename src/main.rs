mod app_state;
mod gpu;
mod ui;
mod utils;
extern crate nvml_wrapper as nvml;
use crate::gpu::info::collect_gpu_info;
use crate::ui::render::ui;
use crate::utils::system::kill_selected_process;
use app_state::AppState;
use clap::{Arg, Command};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use nvml::Nvml;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::error::Error;
use std::io::stdout;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("nviwatch")
        .version("0.1.0")
        .author("Manpreet Singh")
        .about("NviWatch: A blazingly fast rust based TUI for managing and monitoring NVIDIA GPU processes")
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .value_name("MILLISECONDS")
                .help("Refresh interval in milliseconds")
                .default_value("100") // Set the default value to "100"
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
        .get_matches();

    let use_tabbed_graphs = matches.get_flag("tabbed-graphs");
    let use_bar_charts = matches.get_flag("bar-chart");

    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse().expect("Invalid number"))
        .unwrap_or(1000);

    let nvml = Nvml::init()?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_update = Instant::now();

    let mut app_state = AppState {
        selected_process: 0,
        selected_gpu_tab: 0,
        gpu_infos: Vec::new(),
        error_message: None,
        power_history: Vec::new(),
        utilization_history: Vec::new(),
        use_tabbed_graphs,
        use_bar_charts,
    };
    loop {
        if last_update.elapsed() >= Duration::from_millis(watch_interval) {
            last_update = Instant::now();
            collect_gpu_info(&nvml, &mut app_state)?;
            terminal.draw(|f| ui(f, &app_state))?;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Up => {
                        if app_state.selected_process > 0 {
                            app_state.selected_process -= 1;
                        }
                    }
                    KeyCode::Down => {
                        let total_processes: usize = app_state
                            .gpu_infos
                            .iter()
                            .map(|gpu| gpu.processes.len())
                            .sum();
                        if app_state.selected_process < total_processes - 1 {
                            app_state.selected_process += 1;
                        }
                    }
                    KeyCode::Left => {
                        if app_state.use_tabbed_graphs && app_state.selected_gpu_tab > 0 {
                            app_state.selected_gpu_tab -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app_state.use_tabbed_graphs
                            && app_state.selected_gpu_tab < app_state.gpu_infos.len() - 1
                        {
                            app_state.selected_gpu_tab += 1;
                        }
                    }
                    KeyCode::Char('x') => {
                        match kill_selected_process(&app_state) {
                            Ok(_) => {
                                // Refresh the process list immediately after killing a process
                                app_state.gpu_infos = collect_gpu_info(&nvml, &mut app_state)?;
                            }
                            Err(e) => {
                                // Store the error message to display it in the UI
                                app_state.error_message = Some(e.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_update.elapsed() >= Duration::from_millis(watch_interval) {
            last_update = Instant::now();
            app_state.gpu_infos = collect_gpu_info(&nvml, &mut app_state)?;
        }

        terminal.draw(|f| ui(f, &app_state))?;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
