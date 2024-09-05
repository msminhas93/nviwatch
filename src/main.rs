extern crate clap;
extern crate crossterm;
extern crate nix;
extern crate nvml_wrapper as nvml;
extern crate procfs;
extern crate ratatui;
extern crate textwrap;

use clap::{Arg, Command};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use nix::unistd::{sysconf, SysconfVar};
use nix::unistd::{Uid, User};
use nvml::enum_wrappers::device::TemperatureSensor;
use nvml::Nvml;
use procfs::process::Process;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use ratatui::Terminal;
use std::error::Error;
use std::fs;
use std::io::stdout;
use std::io::{Error as IoError, ErrorKind};
use std::time::{Duration, Instant};

struct AppState {
    selected_process: usize,
    selected_gpu_tab: usize,
    gpu_infos: Vec<GpuInfo>,
    error_message: Option<String>,
    power_history: Vec<Vec<u64>>,
    utilization_history: Vec<Vec<u64>>,
    use_tabbed_graphs: bool,
}
struct GpuInfo {
    index: usize,
    name: String,
    temperature: u32,
    utilization: u32,
    memory_used: u64,
    memory_total: u64,
    power_usage: u32,
    power_limit: u32,
    clock_freq: u32,
    processes: Vec<GpuProcessInfo>,
}

#[derive(Clone)]
struct GpuProcessInfo {
    pid: u32,
    used_gpu_memory: u64,
    username: String,
    command: String,
    cpu_usage: f32,
    memory_usage: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("gpu-info-rs")
        .version("0.1.0")
        .author("Your Name")
        .about("Displays GPU information in a tabular format")
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .value_name("MILLISECONDS")
                .help("Refresh interval in milliseconds")
                .required(false),
        )
        .arg(
            Arg::new("tabbed-graphs")
                .long("tabbed-graphs")
                .help("Display GPU graphs in tabbed view")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let use_tabbed_graphs = matches.get_flag("tabbed-graphs");

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
    };
    loop {
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

fn kill_selected_process(app_state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_processes = Vec::new();
    for gpu_info in &app_state.gpu_infos {
        all_processes.extend(gpu_info.processes.iter());
    }

    // Sort processes by GPU memory usage (descending) to match the UI
    all_processes.sort_by(|a, b| b.used_gpu_memory.cmp(&a.used_gpu_memory));

    if app_state.selected_process < all_processes.len() {
        let selected_process = &all_processes[app_state.selected_process];
        let pid = selected_process.pid;
        match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
            Ok(_) => Ok(()),
            Err(nix::Error::EPERM) => Err(Box::new(IoError::new(
                ErrorKind::PermissionDenied,
                format!(
                    "Permission denied to terminate process {} ({})",
                    pid, selected_process.command
                ),
            ))),
            Err(e) => Err(Box::new(IoError::new(
                ErrorKind::Other,
                format!(
                    "Failed to terminate process {} ({}): {}",
                    pid, selected_process.command, e
                ),
            ))),
        }
    } else {
        Err(Box::new(IoError::new(
            ErrorKind::NotFound,
            "Selected process not found",
        )))
    }
}

fn render_gpu_info(f: &mut Frame, area: Rect, gpu_infos: &[GpuInfo]) {
    let block = Block::default().borders(Borders::ALL).title("GPU Info");
    f.render_widget(block.clone(), area);
    let gpu_area = block.inner(area);

    // Calculate maximum widths for each column
    let max_index_width = gpu_infos
        .iter()
        .map(|info| info.index.to_string().len())
        .max()
        .unwrap_or(0)
        .max(3);
    let max_name_width = gpu_infos
        .iter()
        .map(|info| info.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let max_temp_width = gpu_infos
        .iter()
        .map(|info| format!("{}°C", info.temperature).len())
        .max()
        .unwrap_or(0)
        .max(4);
    let max_util_width = gpu_infos
        .iter()
        .map(|info| format!("{}%", info.utilization).len())
        .max()
        .unwrap_or(0)
        .max(4);
    let max_memory_width = gpu_infos
        .iter()
        .map(|info| {
            format!(
                "{}/{}MB",
                info.memory_used / 1_048_576,
                info.memory_total / 1_048_576
            )
            .len()
        })
        .max()
        .unwrap_or(0)
        .max(6);
    let max_power_width = gpu_infos
        .iter()
        .map(|info| format!("{}/{}W", info.power_usage, info.power_limit).len())
        .max()
        .unwrap_or(0)
        .max(5);
    let max_clock_width = gpu_infos
        .iter()
        .map(|info| format!("{}MHz", info.clock_freq).len())
        .max()
        .unwrap_or(0)
        .max(5);

    // Add some padding to each width
    let index_width = max_index_width + 2;
    let name_width = max_name_width + 2;
    let temp_width = max_temp_width + 2;
    let util_width = max_util_width + 2;
    let memory_width = max_memory_width + 2;
    let power_width = max_power_width + 2;
    let clock_width = max_clock_width + 2;

    let rows: Vec<Row> = gpu_infos
        .iter()
        .map(|info| {
            let cells = vec![
                Cell::from(info.index.to_string()).style(Style::default().fg(Color::Cyan)),
                Cell::from(info.name.as_str()).style(Style::default().fg(Color::Green)),
                Cell::from(format!("{}°C", info.temperature))
                    .style(Style::default().fg(Color::Red)),
                Cell::from(format!("{}%", info.utilization))
                    .style(Style::default().fg(Color::Magenta)),
                Cell::from(format!(
                    "{}/{}MB",
                    info.memory_used / 1_048_576,
                    info.memory_total / 1_048_576
                ))
                .style(Style::default().fg(Color::Blue)),
                Cell::from(format!("{}/{}W", info.power_usage, info.power_limit))
                    .style(Style::default().fg(Color::Yellow)),
                Cell::from(format!("{}MHz", info.clock_freq))
                    .style(Style::default().fg(Color::LightCyan)),
            ];
            Row::new(cells)
        })
        .collect();

    let table = Table::new(
        rows,
        &[
            Constraint::Length(index_width as u16),
            Constraint::Length(name_width as u16),
            Constraint::Length(temp_width as u16),
            Constraint::Length(util_width as u16),
            Constraint::Length(memory_width as u16),
            Constraint::Length(power_width as u16),
            Constraint::Length(clock_width as u16),
        ],
    )
    .header(Row::new(vec![
        Cell::from("GPU").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Name").style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Temp").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Cell::from("Util").style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Memory").style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Power").style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Clock").style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .widths(&[
        Constraint::Length(index_width as u16),
        Constraint::Length(name_width as u16),
        Constraint::Length(temp_width as u16),
        Constraint::Length(util_width as u16),
        Constraint::Length(memory_width as u16),
        Constraint::Length(power_width as u16),
        Constraint::Length(clock_width as u16),
    ])
    .column_spacing(1);

    f.render_widget(table, gpu_area);
}

fn render_process_list(f: &mut Frame, area: Rect, app_state: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
        .split(area);

    let main_area = layout[0];
    let footer_area = layout[1];

    let block = Block::default()
        .borders(Borders::ALL)
        .title("GPU Processes");
    f.render_widget(block.clone(), main_area);
    let process_area = block.inner(main_area);

    let mut all_processes = Vec::new();
    for (gpu_index, gpu_info) in app_state.gpu_infos.iter().enumerate() {
        for process in &gpu_info.processes {
            all_processes.push((gpu_index, process));
        }
    }

    all_processes.sort_by(|a, b| b.1.used_gpu_memory.cmp(&a.1.used_gpu_memory));

    let rows: Vec<Row> = all_processes
        .iter()
        .enumerate()
        .map(|(index, (gpu_index, process))| {
            let style = if index == app_state.selected_process {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(gpu_index.to_string()).style(style.fg(Color::Cyan)),
                Cell::from(process.pid.to_string()).style(style.fg(Color::Yellow)),
                Cell::from(format!("{}MB", process.used_gpu_memory / 1_048_576))
                    .style(style.fg(Color::Green)),
                Cell::from(format!("{:.1}%", process.cpu_usage)).style(style.fg(Color::Magenta)),
                Cell::from(format!("{}MB", process.memory_usage / 1_048_576))
                    .style(style.fg(Color::Blue)),
                Cell::from(process.username.as_str()).style(style.fg(Color::Red)),
                Cell::from(process.command.as_str()).style(style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        &[
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(15),
            Constraint::Percentage(100),
        ],
    )
    .header(Row::new(vec![
        Cell::from("GPU").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("PID").style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("GPU Mem").style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("CPU").style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Mem").style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("User").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Cell::from("Command").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .column_spacing(1);

    if let Some(error_msg) = &app_state.error_message {
        let error_text = textwrap::wrap(error_msg, process_area.width as usize - 2);
        let error_paragraph = Paragraph::new(error_text.join("\n"))
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Error"));
        let error_area = Rect {
            x: process_area.x,
            y: process_area.y + process_area.height - 3,
            width: process_area.width,
            height: 3,
        };
        f.render_widget(error_paragraph, error_area);
    }

    f.render_widget(table, process_area);
    // Render the footer
    render_footer(f, footer_area, &app_state);
}

fn render_footer(f: &mut Frame, area: Rect, app_state: &AppState) {
    let footer_text = if app_state.use_tabbed_graphs {
        "↑↓ to navigate processes | ←→ to switch GPU tabs | x to kill process | q to quit"
    } else {
        "↑↓ to navigate processes | x to kill process | q to quit"
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(footer, area);
}

fn get_process_info(pid: u32, used_gpu_memory: u64) -> Option<GpuProcessInfo> {
    if let Ok(process) = Process::new(pid as i32) {
        if let Ok(uid) = process.uid() {
            if let Ok(Some(user)) = User::from_uid(Uid::from_raw(uid)) {
                let command = process.cmdline().unwrap_or_default().join(" ");
                let cpu_usage = process
                    .stat()
                    .ok()
                    .and_then(|stat| {
                        let total_time = stat.utime + stat.stime;
                        let clock_ticks = get_clock_ticks_per_second();
                        let uptime = get_system_uptime();
                        Some((total_time as f64 / clock_ticks as f64 / uptime * 100.0) as f32)
                    })
                    .unwrap_or(0.0);
                let memory_usage = process.stat().ok().map(|stat| stat.rss * 4096).unwrap_or(0);

                return Some(GpuProcessInfo {
                    pid,
                    used_gpu_memory,
                    username: user.name,
                    command,
                    cpu_usage,
                    memory_usage,
                });
            }
        }
    }
    None
}

fn get_clock_ticks_per_second() -> u64 {
    sysconf(SysconfVar::CLK_TCK)
        .unwrap()
        .map(|ticks| ticks as u64)
        .unwrap_or(100)
}

fn get_system_uptime() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|content| content.split_whitespace().next().map(String::from))
        .and_then(|uptime_str| uptime_str.parse().ok())
        .unwrap_or(0.0)
}

fn collect_gpu_info(nvml: &Nvml, app_state: &mut AppState) -> Result<Vec<GpuInfo>, Box<dyn Error>> {
    let device_count = nvml.device_count()?;
    let mut gpu_infos = Vec::new();

    for index in 0..device_count {
        let device = nvml.device_by_index(index)?;
        let name = device.name()?;
        let temperature = device.temperature(TemperatureSensor::Gpu)?;
        let utilization = device.utilization_rates()?.gpu;
        let memory = device.memory_info()?;

        let power_usage = device.power_usage()? / 1000; // Convert mW to W
        let power_limit = device.enforced_power_limit()? / 1000; // Convert mW to W
        let clock_freq = device.clock_info(nvml::enum_wrappers::device::Clock::Graphics)?;

        let compute_processes: Vec<GpuProcessInfo> = device
            .running_compute_processes()?
            .into_iter()
            .filter_map(|p| {
                let used_gpu_memory = match p.used_gpu_memory {
                    nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                    nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                };
                get_process_info(p.pid, used_gpu_memory)
            })
            .collect();

        let graphics_processes: Vec<GpuProcessInfo> = device
            .running_graphics_processes()?
            .into_iter()
            .filter_map(|p| {
                let used_gpu_memory = match p.used_gpu_memory {
                    nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                    nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                };
                get_process_info(p.pid, used_gpu_memory)
            })
            .collect();
        // Update historical data
        if app_state.power_history.len() <= index as usize {
            app_state.power_history.push(Vec::new());
            app_state.utilization_history.push(Vec::new());
        }
        app_state.power_history[index as usize].push(power_usage as u64);
        app_state.utilization_history[index as usize].push(utilization as u64);

        // Keep only the last 60 data points (for a 1-minute graph)
        if app_state.power_history[index as usize].len() > 60 {
            app_state.power_history[index as usize].remove(0);
            app_state.utilization_history[index as usize].remove(0);
        }
        gpu_infos.push(GpuInfo {
            index: index as usize,
            name,
            temperature,
            utilization,
            memory_used: memory.used,
            memory_total: memory.total,
            power_usage,
            power_limit,
            clock_freq,
            processes: [compute_processes, graphics_processes].concat(),
        });
    }

    Ok(gpu_infos)
}

use ratatui::symbols;
use ratatui::text::Line;
use ratatui::widgets::GraphType;
use ratatui::widgets::{Axis, Chart, Dataset, Tabs};

fn render_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
    if app_state.use_tabbed_graphs {
        render_tabbed_gpu_graphs(f, area, app_state);
    } else {
        render_all_gpu_graphs(f, area, app_state);
    }
}

fn render_tabbed_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
    // Create tab titles
    let titles: Vec<Line> = app_state
        .gpu_infos
        .iter()
        .enumerate()
        .map(|(i, _)| Line::from(format!("GPU {}", i)))
        .collect();

    // Create Tabs widget
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("GPU Graphs"))
        .select(app_state.selected_gpu_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow));

    // Render tabs
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(area);

    f.render_widget(tabs, chunks[0]);

    // Render graphs for the selected GPU
    if let Some(_gpu_info) = app_state.gpu_infos.get(app_state.selected_gpu_tab) {
        let gpu_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(chunks[1]);

        render_power_graph(f, gpu_chunks[0], app_state, app_state.selected_gpu_tab);
        render_utilization_graph(f, gpu_chunks[1], app_state, app_state.selected_gpu_tab);
    }
}

fn render_all_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
    let gpu_count = app_state.gpu_infos.len();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Percentage((100 / gpu_count) as u16);
            gpu_count
        ])
        .split(area);

    for (index, _) in app_state.gpu_infos.iter().enumerate() {
        let gpu_area = chunks[index];
        let gpu_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(gpu_area);

        render_power_graph(f, gpu_chunks[0], app_state, index);
        render_utilization_graph(f, gpu_chunks[1], app_state, index);
    }
}

fn render_power_graph(f: &mut Frame, area: Rect, app_state: &AppState, gpu_index: usize) {
    let gpu_info = &app_state.gpu_infos[gpu_index];
    let power_data: Vec<(f64, f64)> = app_state.power_history[gpu_index]
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v as f64))
        .collect();

    let power_dataset = Dataset::default()
        .name("Power (W)")
        .marker(symbols::Marker::Braille)
        .graph_type(ratatui::widgets::GraphType::Line)
        .style(Style::default().fg(Color::Yellow))
        .data(&power_data);

    let power_limit_data = vec![
        (0.0, gpu_info.power_limit as f64),
        (59.0, gpu_info.power_limit as f64),
    ];
    let power_limit_dataset = Dataset::default()
        .name("Power Limit")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Red))
        .data(&power_limit_data);

    let power_chart = Chart::new(vec![power_dataset, power_limit_dataset])
        .block(
            Block::default()
                .title(format!("GPU {} Power", gpu_index))
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time (s)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 60.0])
                .labels(
                    ["0", "15", "30", "45", "60"]
                        .iter()
                        .map(|&s| s.to_string())
                        .collect::<Vec<String>>(),
                ),
        )
        .y_axis(
            Axis::default()
                .title("Power (W)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, gpu_info.power_limit as f64 * 1.1])
                .labels(vec![
                    format!("{:.0}", 0.0),
                    format!("{:.0}", gpu_info.power_limit as f64 / 2.0),
                    format!("{:.0}", gpu_info.power_limit as f64),
                ]),
        );

    f.render_widget(power_chart, area);
}

fn render_utilization_graph(f: &mut Frame, area: Rect, app_state: &AppState, gpu_index: usize) {
    let util_data: Vec<(f64, f64)> = app_state.utilization_history[gpu_index]
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v as f64))
        .collect();

    let util_dataset = Dataset::default()
        .name("Utilization (%)")
        .marker(symbols::Marker::Braille)
        .graph_type(ratatui::widgets::GraphType::Line)
        .style(Style::default().fg(Color::Magenta))
        .data(&util_data);

    let util_baseline_dataset = Dataset::default()
        .name("Baseline")
        .marker(symbols::Marker::Braille)
        .graph_type(ratatui::widgets::GraphType::Line)
        .style(Style::default().fg(Color::Gray))
        .data(&[(0.0, 0.0), (59.0, 0.0)]);

    let util_chart = Chart::new(vec![util_dataset, util_baseline_dataset])
        .block(
            Block::default()
                .title(format!("GPU {} Utilization", gpu_index))
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time (s)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 60.0])
                .labels(
                    ["0", "15", "30", "45", "60"]
                        .iter()
                        .map(|&s| s.to_string())
                        .collect::<Vec<String>>(),
                ),
        )
        .y_axis(
            Axis::default()
                .title("Utilization (%)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 100.0])
                .labels(
                    ["0", "25", "50", "75", "100"]
                        .iter()
                        .map(|&s| s.to_string())
                        .collect::<Vec<String>>(),
                ),
        );

    f.render_widget(util_chart, area);
}

fn ui(f: &mut Frame, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(20),
                Constraint::Percentage(30),
                Constraint::Percentage(50),
            ]
            .as_ref(),
        )
        .split(f.area());

    render_gpu_info(f, chunks[0], &app_state.gpu_infos);
    render_gpu_graphs(f, chunks[1], app_state);
    render_process_list(f, chunks[2], app_state);
}
