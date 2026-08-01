mod app_state;
mod gpu;
mod influxdb;
mod keybinds;
mod ui;
mod utils;

use crate::gpu::info::collect_gpu_info;
use crate::gpu::process::GpuProcessInfo;
use crate::influxdb::{InfluxDBConfig, send_to_influxdb};
use crate::keybinds::{KeybindAggregate, PendingOp};
use crate::ui::render::ui;
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
        pending_op: PendingOp::None,
    };

    loop {
        if last_update.elapsed() >= Duration::from_millis(watch_interval) {
            last_update = Instant::now();
            app_state.gpu_infos = collect_gpu_info(&nvml, &mut app_state)?;

            if let (Some(url), Some(org), Some(bucket), Some(token)) = (
                influx_url.as_ref(),
                influx_org.as_ref(),
                influx_bucket.as_ref(),
                influx_token.as_ref(),
            ) {
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

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

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
                        let total_processes: usize = app_state
                            .gpu_infos
                            .iter()
                            .map(|gpu| gpu.processes.len())
                            .sum();
                        if total_processes > 0
                            && app_state.selected_process < total_processes - 1
                        {
                            app_state.selected_process += 1;
                        }
                    }
                    KeybindAggregate::Left => {
                        if app_state.use_tabbed_graphs && app_state.selected_gpu_tab > 0 {
                            app_state.selected_gpu_tab -= 1;
                        }
                    }
                    KeybindAggregate::Right => {
                        if app_state.use_tabbed_graphs
                            && app_state.selected_gpu_tab < app_state.gpu_infos.len() - 1
                        {
                            app_state.selected_gpu_tab += 1;
                        }
                    }
                }
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break,
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
                    let total_processes: usize = app_state
                        .gpu_infos
                        .iter()
                        .map(|gpu| gpu.processes.len())
                        .sum();
                    if total_processes > 0 {
                        app_state.selected_process = total_processes - 1;
                    }
                }
                KeyCode::Char('d') if ctrl => {
                    // Ctrl+d - default mode
                    app_state.pending_op = PendingOp::None;
                    app_state.use_tabbed_graphs = false;
                    app_state.use_bar_charts = false;
                }
                KeyCode::Char('d') => {
                    // dd - kill selected process (first d arms pending)
                    if app_state.pending_op == PendingOp::Kill {
                        app_state.pending_op = PendingOp::None;
                        let total_processes: usize = app_state
                            .gpu_infos
                            .iter()
                            .map(|gpu| gpu.processes.len())
                            .sum();
                        if app_state.selected_process < total_processes {
                            let all_processes: Vec<&GpuProcessInfo> = app_state
                                .gpu_infos
                                .iter()
                                .flat_map(|gpu| &gpu.processes)
                                .collect();
                            if let Some(process) =
                                all_processes.get(app_state.selected_process)
                            {
                                if let Err(e) =
                                    kill_selected_process(process.pid, &process.command)
                                {
                                    app_state.error_message = Some(e.to_string());
                                }
                            }
                        }
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
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
