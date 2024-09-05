extern crate clap;
extern crate crossterm;
extern crate flexi_logger;
extern crate log;
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
use flexi_logger::{FileSpec, Logger, WriteMode};
use log::info;
use nix::unistd::{sysconf, SysconfVar};
use nix::unistd::{Uid, User};
use nvml::enum_wrappers::device::TemperatureSensor;
use nvml::Nvml;
use procfs::process::Process;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;
use ratatui::Terminal;
use std::error::Error;
use std::fs;
use std::io::stdout;
use std::time::{Duration, Instant};

struct GpuInfo {
    index: usize,
    name: String,
    temperature: u32,
    utilization: u32,
    memory_used: u64,
    memory_total: u64,
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
    Logger::try_with_str("debug")?
        .log_to_file(FileSpec::default())
        .write_mode(WriteMode::BufferAndFlush)
        .start()?;

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
        .get_matches();

    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse().expect("Invalid number"))
        .unwrap_or(1000);

    let nvml = Nvml::init()?;
    info!("Initialized NVML");

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_update = Instant::now();
    let mut gpu_infos: Vec<GpuInfo>;

    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        if last_update.elapsed() >= Duration::from_millis(watch_interval) {
            last_update = Instant::now();
            gpu_infos = collect_gpu_info(&nvml)?;
            terminal.draw(|f| {
                let area = f.area();
                let main_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                    .split(area);

                render_gpu_info(f, main_layout[0], &gpu_infos);
                render_process_list(f, main_layout[1], &gpu_infos);
            })?;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn render_gpu_info(f: &mut Frame, area: Rect, gpu_infos: &[GpuInfo]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("GPU Info (Press 'q' to quit)");
    f.render_widget(block.clone(), area);

    let gpu_area = block.inner(area);
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
            ];
            Row::new(cells)
        })
        .collect();

    let table = Table::new(
        rows,
        &[
            Constraint::Length(3),
            Constraint::Length(30),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(20),
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
    ]))
    .widths(&[
        Constraint::Length(3),
        Constraint::Length(30),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(20),
    ])
    .column_spacing(1);

    f.render_widget(table, gpu_area);
}

fn render_process_list(f: &mut Frame, area: Rect, gpu_infos: &[GpuInfo]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("GPU Processes");
    f.render_widget(block.clone(), area);
    let process_area = block.inner(area);

    let mut all_processes = Vec::new();
    for (gpu_index, gpu_info) in gpu_infos.iter().enumerate() {
        for process in &gpu_info.processes {
            all_processes.push((gpu_index, process));
        }
    }
    all_processes.sort_by(|a, b| b.1.used_gpu_memory.cmp(&a.1.used_gpu_memory));

    let rows: Vec<Row> = all_processes
        .iter()
        .map(|(gpu_index, process)| {
            Row::new(vec![
                Cell::from(gpu_index.to_string()).style(Style::default().fg(Color::Cyan)),
                Cell::from(process.pid.to_string()).style(Style::default().fg(Color::Yellow)),
                Cell::from(format!("{}MB", process.used_gpu_memory / 1_048_576))
                    .style(Style::default().fg(Color::Green)),
                Cell::from(format!("{:.1}%", process.cpu_usage))
                    .style(Style::default().fg(Color::Magenta)),
                Cell::from(format!("{}MB", process.memory_usage / 1_048_576))
                    .style(Style::default().fg(Color::Blue)),
                Cell::from(process.username.as_str()).style(Style::default().fg(Color::Red)),
                Cell::from(process.command.as_str()),
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

    f.render_widget(table, process_area);
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

fn collect_gpu_info(nvml: &Nvml) -> Result<Vec<GpuInfo>, Box<dyn Error>> {
    let device_count = nvml.device_count()?;
    let mut gpu_infos = Vec::new();

    for index in 0..device_count {
        let device = nvml.device_by_index(index)?;
        let name = device.name()?;
        let temperature = device.temperature(TemperatureSensor::Gpu)?;
        let utilization = device.utilization_rates()?.gpu;
        let memory = device.memory_info()?;

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

        let all_processes = [compute_processes, graphics_processes].concat();

        gpu_infos.push(GpuInfo {
            index: index as usize,
            name,
            temperature,
            utilization,
            memory_used: memory.used,
            memory_total: memory.total,
            processes: all_processes,
        });
    }

    Ok(gpu_infos)
}
