use crate::app_state::AppState;
use crate::gpu::info::GpuInfo;
use crate::ui::widgets::{render_footer, render_gpu_graphs};
use crate::utils::formatting::format_memory_size;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

/// Extra columns of breathing room around auto-sized content.
pub(crate) const PADDING: usize = 2;

/// `content_width -> content_width + PADDING` (const fn pointer for maps/arrays).
pub(crate) const PAD_WIDTH: fn(usize) -> usize = |w| w + PADDING;

fn max_col_width(gpu_infos: &[GpuInfo], floor: usize, measure: impl Fn(&GpuInfo) -> usize) -> usize {
    gpu_infos
        .iter()
        .map(measure)
        .max()
        .unwrap_or(0)
        .max(floor)
}

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

    // Per-column: (header, color, min width, measure). One walk instead of 7 copies.
    // TODO(theme): hoist these colors into a shared UI palette enum.
    let columns: [(&str, Color, usize, fn(&GpuInfo) -> usize); 7] = [
        ("GPU", Color::Cyan, 3, |i| i.index.to_string().len()),
        ("Name", Color::Green, 4, |i| i.name.len()),
        ("Temp", Color::Red, 4, |i| format!("{}°C", i.temperature).len()),
        ("Util", Color::Magenta, 4, |i| format!("{}%", i.utilization).len()),
        ("Memory", Color::Blue, 6, |i| {
            format!(
                "{}/{}MB",
                i.memory_used / 1_048_576,
                i.memory_total / 1_048_576
            )
            .len()
        }),
        ("Power", Color::Yellow, 5, |i| {
            format!("{}/{}W", i.power_usage, i.power_limit).len()
        }),
        ("Clock", Color::LightCyan, 5, |i| {
            format!("{}MHz", i.clock_freq).len()
        }),
    ];

    let need_padding: [usize; 7] = std::array::from_fn(|i| {
        let (_, _, floor, measure) = columns[i];
        PAD_WIDTH(max_col_width(gpu_infos, floor, measure))
    });

    let rows: Vec<Row> = gpu_infos
        .iter()
        .map(|info| {
            Row::new(vec![
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
            ])
        })
        .collect();

    let header = Row::new(
        columns
            .iter()
            .map(|(title, color, _, _)| {
                Cell::from(*title).style(
                    Style::default()
                        .fg(*color)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>(),
    );

    let table = Table::new(
        rows,
        need_padding
            .iter()
            .map(|&w| Constraint::Length(w as u16))
            .collect::<Vec<Constraint>>(),
    )
    .header(header)
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

    // Same sort/order as kill / yank (AppState::processes_display_order).
    let all_processes = app_state.processes_display_order();

    // Pending highlight is constant for the whole frame — compute once.
    let pending = app_state.pending_op.is_pending();
    let selected_style = if pending {
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::DarkGray)
    };

    let rows: Vec<Row> = all_processes
        .iter()
        .enumerate()
        .map(|(index, (gpu_index, process))| {
            let is_selected = index == app_state.selected_process;
            let style = if is_selected {
                selected_style
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

    let header_cols: [(&str, Color); 7] = [
        ("GPU", Color::Cyan),
        ("PID", Color::Yellow),
        ("GPU Mem", Color::Green),
        ("CPU", Color::Magenta),
        ("Mem", Color::Blue),
        ("User", Color::Red),
        ("Command", Color::Reset),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(15),
            Constraint::Percentage(100),
        ],
    )
    .header(Row::new(
        header_cols
            .iter()
            .map(|(title, color)| {
                let mut style = Style::default().add_modifier(Modifier::BOLD);
                if *color != Color::Reset {
                    style = style.fg(*color);
                }
                Cell::from(*title).style(style)
            })
            .collect::<Vec<_>>(),
    ))
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
    render_footer(f, footer_area, app_state);
}
