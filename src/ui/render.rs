use crate::app_state::AppState;
use crate::gpu::info::GpuInfo;
use crate::system_monitor::SortMode;
use crate::ui::widgets::{
    render_cpu_info, render_cpu_utilization, render_footer, render_gpu_graphs, render_help,
};
use crate::utils::format_memory_size;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

/// Minimum rows for the graphs pane (chart axes/title inside borders).
const GRAPHS_MIN: u16 = 8;
/// Borders + header + ≥1 process row + footer line.
const PROCESS_MIN: u16 = 6;
const MIN_WIDTH: u16 = 72;
/// Wider + taller floor for the split CPU|GPU layout.
const CPU_MIN_WIDTH: u16 = 96;
const CPU_MIN_HEIGHT: u16 = 28;

/// Borders (2) + header (1) + one row per GPU (at least 1 placeholder when empty).
fn gpu_info_height(num_gpus: usize) -> u16 {
    3 + num_gpus.max(1) as u16
}

fn required_height(num_gpus: usize) -> u16 {
    gpu_info_height(num_gpus) + GRAPHS_MIN + PROCESS_MIN
}

fn render_terminal_too_small(f: &mut Frame, area: Rect, min_w: u16, required_h: u16) {
    let need_w = area.width < min_w;
    let need_h = area.height < required_h;
    let size_hint = match (need_w, need_h) {
        (true, true) => format!(
            "Need at least {min_w} cols × {required_h} rows\n(now {}×{})",
            area.width, area.height
        ),
        (true, false) => format!("Need at least {min_w} columns (now {})", area.width),
        (false, true) => format!(
            "Need at least {required_h} rows (now {})",
            area.height
        ),
        (false, false) => unreachable!(),
    };
    let message = format!("Terminal too small\n{size_hint}\n\nResize to continue");

    // Vertically center a short block so the banner isn't stretched full-screen.
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(7),
            Constraint::Percentage(35),
        ])
        .split(area);
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(outer[1]);

    let paragraph = Paragraph::new(message)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" nviwatch ")
                .border_style(Style::default().fg(Color::Yellow)),
        );
    f.render_widget(paragraph, inner[1]);
}

pub fn ui(f: &mut Frame, app_state: &AppState) {
    if app_state.show_help {
        // Help is usable even in a short terminal — scroll within the pane.
        // Skip the main-dashboard minimum-size gate for this overlay.
        render_help(f, f.area(), app_state.help_scroll, app_state.cpu_monitoring);
        return;
    }

    if app_state.cpu_monitoring {
        ui_with_cpu(f, app_state);
    } else {
        ui_gpu_only(f, app_state);
    }
}

/// Upstream layout: GPU Info, GPU graphs, and the GPU process tray stacked
/// vertically. This is the default (no `--cpu`) view.
fn ui_gpu_only(f: &mut Frame, app_state: &AppState) {
    let area = f.area();
    let num_gpus = app_state.gpu_infos.len();
    let gpu_info_h = gpu_info_height(num_gpus);
    let required_h = required_height(num_gpus);

    if area.width < MIN_WIDTH || area.height < required_h {
        render_terminal_too_small(f, area, MIN_WIDTH, required_h);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(gpu_info_h),
                Constraint::Min(GRAPHS_MIN),
                Constraint::Min(PROCESS_MIN),
            ]
            .as_ref(),
        )
        .split(area);

    render_gpu_info(f, chunks[0], &app_state.gpu_infos);
    render_gpu_graphs(f, chunks[1], app_state);
    render_process_list(f, chunks[2], app_state);
}

/// `--cpu` layout: CPU panels (left) + GPU panels (right) over a full-width
/// system-wide process tray.
fn ui_with_cpu(f: &mut Frame, app_state: &AppState) {
    let area = f.area();
    if area.width < CPU_MIN_WIDTH || area.height < CPU_MIN_HEIGHT {
        render_terminal_too_small(f, area, CPU_MIN_WIDTH, CPU_MIN_HEIGHT);
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)].as_ref())
        .split(area);
    let top = root[0];
    let bottom = root[1];

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)].as_ref())
        .split(top);
    let left = columns[0];
    let right = columns[1];

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)].as_ref())
        .split(left);
    render_cpu_info(f, left_chunks[0], &app_state.cpu_stats);
    render_cpu_utilization(f, left_chunks[1], app_state);

    let num_gpus = app_state.gpu_infos.len() as u16;
    let gpu_info_h = (num_gpus + 3).clamp(4, 12);
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(gpu_info_h), Constraint::Min(0)].as_ref())
        .split(right);
    render_gpu_info(f, right_chunks[0], &app_state.gpu_infos);
    render_gpu_graphs(f, right_chunks[1], app_state);

    render_system_process_list(f, bottom, app_state);
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
    .column_spacing(1);

    f.render_widget(table, gpu_area);
}

/// Default (GPU-only) process tray.
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

    // Content widths — headers and values share left edges (left-aligned).
    // Breathing room comes from column_spacing, not `|` rules.
    const W_GPU: usize = 3;
    const W_PID: usize = 7;
    const W_GMEM: usize = 8;
    const W_CPU: usize = 6;
    const W_MEM: usize = 7;
    const W_USER: usize = 8;
    const COL_GAP: u16 = 2;

    let all_processes = app_state.processes_display_order();

    let selected_style = if app_state.pending_op.is_pending() {
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    let rows: Vec<Row> = all_processes
        .iter()
        .enumerate()
        .map(|(index, (gpu_index, process))| {
            let cpu_pct = match process.cpu_percent {
                Some(pct) => format!("{pct:.1}%"),
                None => "—".to_string(),
            };
            let cells = [
                fit(&gpu_index.to_string(), W_GPU),
                fit(&process.pid.to_string(), W_PID),
                fit(&format_memory_size(process.used_gpu_memory), W_GMEM),
                fit(&cpu_pct, W_CPU),
                fit(&format_memory_size(process.memory_usage), W_MEM),
                fit(&process.username, W_USER),
                process.command.clone(),
            ];

            if index == app_state.selected_process {
                Row::new(cells.into_iter().map(|text| Cell::from(text).style(selected_style)))
                    .style(selected_style)
            } else {
                let [gpu, pid, gpu_mem, cpu_pct, mem, user, cmd] = cells;
                Row::new(vec![
                    Cell::from(gpu).style(Style::default().fg(Color::Cyan)),
                    Cell::from(pid).style(Style::default().fg(Color::Yellow)),
                    Cell::from(gpu_mem).style(Style::default().fg(Color::Green)),
                    Cell::from(cpu_pct).style(Style::default().fg(Color::LightMagenta)),
                    Cell::from(mem).style(Style::default().fg(Color::Blue)),
                    Cell::from(user).style(Style::default().fg(Color::Red)),
                    Cell::from(cmd),
                ])
            }
        })
        .collect();

    let header_style = |fg: Color| Style::default().fg(fg).add_modifier(Modifier::BOLD);
    let table = Table::new(
        rows,
        [
            Constraint::Length(W_GPU as u16),
            Constraint::Length(W_PID as u16),
            Constraint::Length(W_GMEM as u16),
            Constraint::Length(W_CPU as u16),
            Constraint::Length(W_MEM as u16),
            Constraint::Length(W_USER as u16),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from(fit("GPU", W_GPU)).style(header_style(Color::Cyan)),
            Cell::from(fit("PID", W_PID)).style(header_style(Color::Yellow)),
            Cell::from(fit("GPU-Mem", W_GMEM)).style(header_style(Color::Green)),
            Cell::from(fit("CPU%", W_CPU)).style(header_style(Color::LightMagenta)),
            Cell::from(fit("Mem", W_MEM)).style(header_style(Color::Blue)),
            Cell::from(fit("User", W_USER)).style(header_style(Color::Red)),
            Cell::from("Command").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .bottom_margin(0),
    )
    .column_spacing(COL_GAP);

    if let Some(error_msg) = &app_state.error_message {
        let wrap_width = (process_area.width as usize).saturating_sub(2).max(1);
        let error_text = textwrap::wrap(error_msg, wrap_width);
        let error_paragraph = Paragraph::new(error_text.join("\n"))
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Error"));
        if process_area.height >= 3 {
            let error_area = Rect {
                x: process_area.x,
                y: process_area.y + process_area.height - 3,
                width: process_area.width,
                height: 3,
            };
            f.render_widget(error_paragraph, error_area);
        }
    }

    f.render_widget(table, process_area);
    render_footer(f, footer_area, app_state);
}

/// `--cpu` process tray: unified system-wide list (already sorted by sort_mode).
pub fn render_system_process_list(f: &mut Frame, area: Rect, app_state: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
        .split(area);

    let main_area = layout[0];
    let footer_area = layout[1];

    let sort_label = match app_state.sort_mode {
        SortMode::Cpu => "CPU%",
        SortMode::GpuMemory => "GPU mem",
    };
    let block = Block::default().borders(Borders::ALL).title(format!(
        "Processes — top {} by {}",
        app_state.processes.len(),
        sort_label
    ));
    f.render_widget(block.clone(), main_area);
    let process_area = block.inner(main_area);

    let selected_style = if app_state.pending_op.is_pending() {
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    let rows: Vec<Row> = app_state
        .processes
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let gpu_col = match p.gpu_index {
                Some(i) => i.to_string(),
                None => "-".to_string(),
            };
            let gpu_mem_col = match p.gpu_memory {
                Some(m) => format_memory_size(m),
                None => "-".to_string(),
            };

            let cells = [
                gpu_col,
                p.pid.to_string(),
                gpu_mem_col,
                format!("{:.1}", p.cpu_usage),
                format_memory_size(p.memory_usage),
                p.username.clone(),
                p.state.to_string(),
                p.command.clone(),
            ];

            if index == app_state.selected_process {
                Row::new(cells.into_iter().map(|text| Cell::from(text).style(selected_style)))
                    .style(selected_style)
            } else {
                let [gpu, pid, gpu_mem, cpu, mem, user, state, cmd] = cells;
                Row::new(vec![
                    Cell::from(gpu).style(Style::default().fg(Color::Cyan)),
                    Cell::from(pid).style(Style::default().fg(Color::Yellow)),
                    Cell::from(gpu_mem).style(Style::default().fg(Color::Green)),
                    Cell::from(cpu).style(Style::default().fg(Color::Magenta)),
                    Cell::from(mem).style(Style::default().fg(Color::Blue)),
                    Cell::from(user).style(Style::default().fg(Color::Red)),
                    Cell::from(state).style(Style::default().fg(Color::Gray)),
                    Cell::from(cmd),
                ])
            }
        })
        .collect();

    let table = Table::new(
        rows,
        &[
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Length(12),
            Constraint::Length(2),
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
        Cell::from("CPU%").style(
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
        Cell::from("S").style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
        Cell::from("Command").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .column_spacing(1);

    f.render_widget(table, process_area);

    if let Some(error_msg) = &app_state.error_message {
        let wrap_width = (process_area.width as usize).saturating_sub(2).max(1);
        let error_text = textwrap::wrap(error_msg, wrap_width);
        let error_paragraph = Paragraph::new(error_text.join("\n"))
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Error"));
        if process_area.height >= 3 {
            let error_area = Rect {
                x: process_area.x,
                y: process_area.y + process_area.height - 3,
                width: process_area.width,
                height: 3,
            };
            f.render_widget(error_paragraph, error_area);
        }
    }

    render_footer(f, footer_area, app_state);
}

/// Left-align header/value in a fixed width so columns share a consistent left edge.
fn fit(text: &str, width: usize) -> String {
    let truncated: String = text.chars().take(width).collect();
    format!("{truncated:<width$}", width = width)
}
