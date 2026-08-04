use crate::app_state::AppState;
use crate::gpu::info::GpuInfo;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::cmp;

pub fn render_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
    if app_state.use_bar_charts {
        render_gpu_bar_charts(f, area, app_state);
    } else if app_state.use_tabbed_graphs {
        render_tabbed_gpu_graphs(f, area, app_state);
    } else {
        render_all_gpu_graphs(f, area, app_state);
    }
}
pub fn render_gpu_bar_charts(f: &mut Frame, area: Rect, app_state: &AppState) {
    let gpu_count = app_state.gpu_infos.len();
    // BUG: [div-by-zero] : render_gpu_bar_charts does not guard against gpu_count == 0;
    //   100 / gpu_count on line below will panic if no GPUs are present.
    //   Compare to render_all_gpu_graphs which has a guard.
    if gpu_count == 0 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Percentage((100 / gpu_count) as u16);
            gpu_count
        ])
        .split(area);

    for (index, gpu_info) in app_state.gpu_infos.iter().enumerate() {
        let gpu_area = chunks[index];
        let gpu_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(gpu_area);

        render_power_bar(f, gpu_chunks[0], gpu_info, index);
        render_utilization_bar(f, gpu_chunks[1], gpu_info, index);
    }
}
pub fn render_tabbed_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
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

pub fn render_footer(f: &mut Frame, area: Rect, app_state: &AppState) {
    let footer_text = if app_state.use_tabbed_graphs {
        "↑↓: nav | ←→: tabs | x: kill | ^d/t/b: modes | ?: help | q: quit"
    } else if app_state.use_bar_charts {
        "↑↓: nav | x: kill | ^d/t/b: modes | ?: help | q: quit"
    } else {
        "↑↓: nav | x: kill | ^d/t/b: modes | ?: help | q: quit"
    };

    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(footer, area);
}

/// Full keymap. Action first; keys for the same action share one row with `·`.
///
/// Content can exceed the terminal height — callers pass a vertical scroll
/// offset and we clamp it so ↑↓ / j k / PgUp/PgDn can reveal the rest.
pub fn render_help(f: &mut Frame, area: Rect, scroll: u16) {
    let lines = help_lines();
    let content_h = lines.len() as u16;
    // Borders take 2 rows; remaining is the viewport for scrolled text.
    let viewport = area.height.saturating_sub(2);
    let max_scroll = content_h.saturating_sub(viewport);
    let scroll = scroll.min(max_scroll);

    let title = if max_scroll > 0 {
        format!(" Help  ·  ↑↓ scroll ({}/{})  ·  Esc/? close ", scroll, max_scroll)
    } else {
        " Help  ·  Esc/? close ".to_string()
    };

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .scroll((scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    f.render_widget(paragraph, area);
}

/// Max scroll offset for the current help content in `area`.
pub fn help_max_scroll(area: Rect) -> u16 {
    let content_h = help_lines().len() as u16;
    let viewport = area.height.saturating_sub(2);
    content_h.saturating_sub(viewport)
}

fn help_lines() -> Vec<Line<'static>> {
    let title = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let action = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let key = Style::default().fg(Color::Yellow);
    let note = Style::default().fg(Color::DarkGray);
    let sep = Style::default().fg(Color::DarkGray);

    vec![
        Line::from(Span::styled("Navigation", title)),
        Line::from(""),
        Line::from(Span::styled("  Move process selection", action)),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("↑ ↓", key),
            Span::styled("  ·  ", sep),
            Span::styled("j k", key),
            Span::styled("  vim", note),
            Span::styled("  ·  ", sep),
            Span::styled("Ctrl+p  Ctrl+n", key),
            Span::styled("  emacs", note),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Switch GPU tabs  (tabbed mode only)",
            action,
        )),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("← →", key),
            Span::styled("  ·  ", sep),
            Span::styled("h l", key),
            Span::styled("  vim", note),
            Span::styled("  ·  ", sep),
            Span::styled("Ctrl+b  Ctrl+f", key),
            Span::styled("  emacs", note),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Jump to first / last process", action)),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("g g", key),
            Span::styled("  top", note),
            Span::styled("  ·  ", sep),
            Span::styled("G", key),
            Span::styled("  bottom", note),
        ]),
        Line::from(""),
        Line::from(Span::styled("Actions", title)),
        Line::from(""),
        Line::from(Span::styled("  Kill selected process", action)),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("x", key),
            Span::styled("  ·  ", sep),
            Span::styled("d d", key),
            Span::styled("  vim chord (press d twice)", note),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Graph mode", action)),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("Ctrl+d", key),
            Span::styled("  default", note),
            Span::styled("  ·  ", sep),
            Span::styled("t", key),
            Span::styled("  tabbed", note),
            Span::styled("  ·  ", sep),
            Span::styled("b", key),
            Span::styled("  bars", note),
        ]),
        Line::from(""),
        Line::from(Span::styled("  This screen / quit", action)),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("?", key),
            Span::styled("  toggle help", note),
            Span::styled("  ·  ", sep),
            Span::styled("Esc", key),
            Span::styled("  close", note),
            Span::styled("  ·  ", sep),
            Span::styled("q", key),
            Span::styled("  quit", note),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Tip: footer lists primary keys only; aliases live here.",
            note,
        )),
    ]
}

pub fn render_all_gpu_graphs(f: &mut Frame, area: Rect, app_state: &AppState) {
    let gpu_count = app_state.gpu_infos.len();
    if gpu_count == 0 {
        // Display a message when no GPUs are found
        let no_gpus_message = "No GPUs found.";
        let paragraph = Paragraph::new(no_gpus_message)
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, area);
        return;
    }

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

pub fn render_power_bar(f: &mut Frame, area: Rect, gpu_info: &GpuInfo, gpu_index: usize) {
    let power_percentage = cmp::min(
        100,
        ((gpu_info.power_usage as f64 / gpu_info.power_limit as f64) * 100.0) as u16,
    );
    let power_bar = Gauge::default()
        .block(
            Block::default()
                .title(format!("GPU {} Power", gpu_index))
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent(power_percentage)
        .label(format!(
            "{}/{}W",
            gpu_info.power_usage, gpu_info.power_limit
        ));
    f.render_widget(power_bar, area);
}

pub fn render_utilization_bar(f: &mut Frame, area: Rect, gpu_info: &GpuInfo, gpu_index: usize) {
    let util_percentage = cmp::min(100, gpu_info.utilization as u16);
    let util_bar = Gauge::default()
        .block(
            Block::default()
                .title(format!("GPU {} Utilization", gpu_index))
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(util_percentage)
        .label(format!("{}%", gpu_info.utilization));
    f.render_widget(util_bar, area);
}

pub fn render_power_graph(f: &mut Frame, area: Rect, app_state: &AppState, gpu_index: usize) {
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

    let power_chart = Chart::new(vec![power_dataset])
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

pub fn render_utilization_graph(f: &mut Frame, area: Rect, app_state: &AppState, gpu_index: usize) {
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

    let util_chart = Chart::new(vec![util_dataset])
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
