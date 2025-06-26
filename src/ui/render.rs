use crate::app_state::AppState;
use crate::gpu::info::GpuInfo;
use crate::ui::widgets::{render_footer, render_gpu_graphs};
use crate::utils::formatting::format_memory_size;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

pub fn ui(f: &mut Frame, app_state: &AppState) {
    let num_gpus = app_state.gpu_infos.len();
    let gpu_info_percentage = {
        let base_percentage = num_gpus as u16 * 5;
        base_percentage.clamp(10, 20)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(gpu_info_percentage),
                Constraint::Percentage(40),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(f.area());

    render_gpu_info(f, chunks[0], &app_state.gpu_infos);
    render_gpu_graphs(f, chunks[1], app_state);
    render_process_list(f, chunks[2], app_state);
}

pub fn render_gpu_info(f: &mut Frame, area: Rect, gpu_infos: &[GpuInfo]) {
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
                    "{}/{}",
                    format_memory_size(info.memory_used),
                    format_memory_size(info.memory_total)
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
    .widths([
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
pub fn render_process_list(f: &mut Frame, area: Rect, app_state: &AppState) {
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
                Cell::from(format_memory_size(process.used_gpu_memory))
                    .style(style.fg(Color::Green)),
                Cell::from(format!("{:.1}%", process.cpu_usage)).style(style.fg(Color::Magenta)),
                Cell::from(format_memory_size(process.memory_usage)).style(style.fg(Color::Blue)),
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
    render_footer(f, footer_area, app_state);
}
