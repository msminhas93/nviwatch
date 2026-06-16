mod app_state;
mod gpu;
mod influx_local;
mod ui;
mod utils;

use crate::gpu::info::collect_gpu_info;
use crate::influx_local::InfluxDBConfig;
use crate::ui::render::ui;
use crate::utils::system::kill_selected_process;
use app_state::AppState;
use clap::{Arg, Command};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use nvml::Nvml;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::error::Error;
use std::io::stdout;
use std::time::Duration;

pub const POLLING_TIMEOUT_MS: u64 = 100;

pub(crate) type Result<T> = core::result::Result<T, Box<dyn Error + Send + Sync>>;

fn main() -> Result<()> {
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
        .unwrap_or(1000);

    let nvml = Nvml::init()?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = AppState::from(&matches);

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    loop {
        if app_state.should_update(watch_interval) {
            // TODO: [internalize] — this whole block could live on AppState:
            //   app_state.update(&nvml, config);
            // The collector, history, and influx queries all operate on app_state's data anyway.

            app_state.gpu_infos = collect_gpu_info(&nvml, &mut app_state)?;

            let config = InfluxDBConfig::try_from(&matches)?;
            let influx_client =
                influxdb::Client::new(&config.url, &config.bucket).with_token(&config.token);
            let queries: Vec<influxdb::WriteQuery> = app_state
                .gpu_infos
                .iter()
                .map(influxdb::WriteQuery::from)
                .collect();

            runtime
                .block_on(async {
                    influx_client
                        .query(queries)
                        .await
                        .map_err(|e| format!("InfluxDB Error: {}", e))
                })
                .inspect_err(|e| {
                    app_state.error_message = Some(e.to_string());
                })
                .ok();
        }

        terminal.draw(|f| ui(f, &app_state))?;

        // Most of the key event handling probably better off being moved to an actual event key handler
        // and we just ask it to 'start' at the start of program & handle it
        // via messages etc.

        if event::poll(Duration::from_millis(POLLING_TIMEOUT_MS))?
            && let Event::Key(key) = event::read()?
        {
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
                    if total_processes > 0 && app_state.selected_process < total_processes - 1 {
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
                    if let Err(e) = kill_selected_process(&app_state) {
                        app_state.error_message = Some(e.to_string());
                    }
                }
                KeyCode::Char('d') => {
                    app_state.use_tabbed_graphs = false;
                    app_state.use_bar_charts = false;
                }
                KeyCode::Char('t') => {
                    app_state.use_tabbed_graphs = true;
                    app_state.use_bar_charts = false;
                }
                KeyCode::Char('b') => {
                    app_state.use_tabbed_graphs = false;
                    app_state.use_bar_charts = true;
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
