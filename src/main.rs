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
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use nix::unistd::{Uid, User};
use nvml::enum_wrappers::device::TemperatureSensor;
use nvml::struct_wrappers::device::ProcessInfo;
use nvml::Nvml;
use procfs::process::Process;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Terminal;
use std::collections::HashMap;
use std::error::Error;
use std::io::stdout;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn Error>> {
    // Parse command-line arguments
    let matches = Command::new("gpu-info-rs")
        .version("0.1.0")
        .author("Your Name <your.email@example.com>")
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

    // Get the watch interval if specified
    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse::<u64>().expect("Invalid number"))
        .unwrap_or(1000); // Default to 1 second if not specified

    // Initialize NVML
    let nvml = Nvml::init()?;

    // Set up terminal interface
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_update = Instant::now();

    loop {
        // Check for 'q' key press
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Update only if the watch interval has passed
        if last_update.elapsed() >= Duration::from_millis(watch_interval) {
            last_update = Instant::now();

            // Get the number of available devices
            let device_count = nvml.device_count()?;

            // Create a vector to store GPU information
            let mut gpu_infos = Vec::new();

            // Loop through all devices
            for index in 0..device_count {
                let device = nvml.device_by_index(index)?;

                let name = device.name()?;
                let temperature = device.temperature(TemperatureSensor::Gpu)?;
                let utilization = device.utilization_rates()?.gpu;
                let memory = device.memory_info()?;

                // Get the list of processes running on the GPU
                let processes: Vec<ProcessInfo> = device.running_compute_processes()?;

                // Aggregate memory usage by user
                let mut user_memory_map: HashMap<String, u64> = HashMap::new();
                for process in processes {
                    let pid = process.pid;
                    let used_memory = match process.used_gpu_memory {
                        nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes / 1_048_576, // Convert bytes to MB
                        nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                    };

                    if let Some(username) = get_user_by_pid(pid) {
                        *user_memory_map.entry(username).or_insert(0) += used_memory;
                    }
                }

                // Sort users by memory usage in descending order
                let mut sorted_users: Vec<_> = user_memory_map.into_iter().collect();
                sorted_users.sort_by(|a, b| b.1.cmp(&a.1));

                // Format user memory usage, including all users
                let user_memory: String = sorted_users
                    .iter()
                    .map(|(user, mem)| format!("{}({}M)", user, mem))
                    .collect::<Vec<String>>()
                    .join(" ");

                // Wrap the user memory string to a maximum width that fits your display
                let wrapped_user_memory = textwrap::fill(&user_memory, 50); // Adjust width as needed

                gpu_infos.push(vec![
                    index.to_string(),
                    name,
                    format!("{}°C", temperature),
                    format!("{}%", utilization),
                    format!("{}/{}MB", memory.used / 1_048_576, memory.total / 1_048_576),
                    wrapped_user_memory,
                ]);
            }

            // Draw the TUI
            terminal.draw(|f| {
                let size = f.area();
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title("GPU Info (Press 'q' to quit)");
                f.render_widget(block, size);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(100)].as_ref())
                    .split(size);

                let rows: Vec<Row> = gpu_infos
                    .iter()
                    .map(|info| {
                        let cells: Vec<Cell> = info.iter().map(|c| Cell::from(c.clone())).collect();
                        Row::new(cells).style(Style::default().fg(Color::White))
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
                        Constraint::Length(100),
                    ],
                )
                .header(
                    Row::new(vec![
                        Cell::from("GPU"),
                        Cell::from("Name"),
                        Cell::from("Temp"),
                        Cell::from("Util"),
                        Cell::from("Memory"),
                        Cell::from("User Mem"),
                    ])
                    .style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                )
                .block(Block::default().borders(Borders::ALL));

                f.render_widget(table, chunks[0]);
            })?;
        }
    }

    // Restore the terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn get_user_by_pid(pid: u32) -> Option<String> {
    if let Ok(process) = Process::new(pid as i32) {
        if let Ok(uid) = process.uid() {
            if let Ok(Some(user)) = User::from_uid(Uid::from_raw(uid)) {
                return Some(user.name);
            }
        }
    }
    None
}
