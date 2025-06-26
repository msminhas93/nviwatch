mod app_state;
mod gpu;
mod influxdb;
mod ui;
mod utils;

extern crate nvml_wrapper as nvml;

use crate::gpu::info::collect_gpu_info;
use crate::influxdb::{send_to_influxdb, InfluxDBConfig};
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

    let use_tabbed_graphs = matches.get_flag("tabbed-graphs");
    let use_bar_charts = matches.get_flag("bar-chart");

    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse().expect("Invalid number"))
        .unwrap_or(1000);

    let influx_url = matches.get_one::<String>("influx-url").cloned();
    let influx_org = matches.get_one::<String>("influx-org").cloned();
    let influx_bucket = matches.get_one::<String>("influx-bucket").cloned();
    let influx_token = matches.get_one::<String>("influx-token").cloned();

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
            app_state.gpu_infos = collect_gpu_info(&nvml, &mut app_state)?;

            if let (Some(url), Some(org), Some(bucket), Some(token)) =
                (influx_url.as_ref(), influx_org.as_ref(), influx_bucket.as_ref(), influx_token.as_ref())
            {
                let influx_config = InfluxDBConfig {
                    url: url.clone(),
                    org: org.clone(),
                    bucket: bucket.clone(),
                    token: token.clone(),
                };
                if let Err(e) = send_to_influxdb(&influx_config, &app_state.gpu_infos) {
                    app_state.error_message = Some(format!("InfluxDB Error: {}", e));
                }
            }
        }

        terminal.draw(|f| ui(f, &app_state))?;

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
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
