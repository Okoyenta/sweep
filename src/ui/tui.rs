use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table};
use ratatui::{DefaultTerminal, Frame};

use crate::domain::models::{MemoryStats, SystemSnapshot};
use crate::services::usage_service::UsageMap;
use crate::ui::status::fmt;

pub struct DashboardData {
    pub snap: SystemSnapshot,
    pub usage: UsageMap,
}

/// actions the host app can perform from within the dashboard
#[derive(Debug, Clone, Copy)]
pub enum TuiAction {
    TrimTop(u32),
    PurgeStandby,
}

const REFRESH_EVERY: Duration = Duration::from_secs(2);

pub fn percent(used: u64, total: u64) -> u16 {
    if total == 0 {
        return 0;
    }
    ((used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as u16
}

fn ago(unix: i64, now: i64) -> String {
    let d = (now - unix).max(0);
    if d < 60 {
        "now".to_string()
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86400 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86400)
    }
}

pub struct Dashboard<F, A>
where
    F: FnMut() -> anyhow::Result<DashboardData>,
    A: FnMut(TuiAction) -> anyhow::Result<String>,
{
    get_data: F,
    act: A,
    status_line: Option<String>,
    data: Option<DashboardData>,
}

impl<F, A> Dashboard<F, A>
where
    F: FnMut() -> anyhow::Result<DashboardData>,
    A: FnMut(TuiAction) -> anyhow::Result<String>,
{
    pub fn new(get_data: F, act: A) -> Self {
        Self {
            get_data,
            act,
            status_line: None,
            data: None,
        }
    }

    fn refresh(&mut self) {
        self.data = Some((self.get_data)().expect("dashboard refresh failed"));
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        self.refresh();
        let mut last_refresh = Instant::now();
        loop {
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('r') => {
                                self.refresh();
                                last_refresh = Instant::now();
                            }
                            KeyCode::Char('t') => match (self.act)(TuiAction::TrimTop(10)) {
                                Ok(msg) => self.status_line = Some(msg),
                                Err(e) => self.status_line = Some(format!("trim failed: {e}")),
                            },
                            KeyCode::Char('p') => match (self.act)(TuiAction::PurgeStandby) {
                                Ok(msg) => self.status_line = Some(msg),
                                Err(e) => {
                                    self.status_line = Some(format!("purge failed: {e}"))
                                }
                            },
                            _ => {}
                        }
                    }
                }
            }
            if last_refresh.elapsed() >= REFRESH_EVERY {
                self.refresh();
                last_refresh = Instant::now();
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let [mem_area, swap_area, disks_area, table_area, footer_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Length(6),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .areas(f.area());

        let data = self.data.as_ref().expect("data refreshed before draw");
        let mem = &data.snap.memory;

        f.render_widget(mem_gauge(mem), mem_area);
        f.render_widget(swap_gauge(mem), swap_area);
        render_disks(f, disks_area, &data.snap);
        f.render_widget(process_table(&data.snap, &data.usage), table_area);
        let footer_text = match &self.status_line {
            Some(msg) => format!(" {msg}  |  q quit | r refresh | t trim top10 | p purge standby"),
            None => " q quit | r refresh (auto every 2s) | t trim top10 | p purge standby"
                .to_string(),
        };
        f.render_widget(
            Paragraph::new(footer_text).style(Style::default().fg(Color::Yellow)),
            footer_area,
        );
    }
}

fn gauge_style(percent: u16) -> Style {
    if percent >= 90 {
        Style::default().fg(Color::Red)
    } else if percent >= 70 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

fn mem_gauge(mem: &MemoryStats) -> Gauge<'_> {
    let pct = percent(mem.used_bytes, mem.total_bytes);
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("memory"))
        .gauge_style(gauge_style(pct))
        .percent(pct)
        .label(format!(
            "{} / {} used ({}%)",
            fmt(mem.used_bytes),
            fmt(mem.total_bytes),
            pct
        ))
}

fn swap_gauge(mem: &MemoryStats) -> Gauge<'_> {
    let pct = percent(mem.swap_used_bytes, mem.swap_total_bytes);
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("swap"))
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(pct)
        .label(format!(
            "{} / {} used",
            fmt(mem.swap_used_bytes),
            fmt(mem.swap_total_bytes)
        ))
}

fn render_disks(f: &mut Frame, area: ratatui::layout::Rect, snap: &SystemSnapshot) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("disks")
        .style(Style::default());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || snap.disks.is_empty() {
        return;
    }
    let shown: Vec<_> = snap.disks.iter().take(inner.height as usize).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, shown.len() as u32); shown.len()])
        .split(inner);
    for (disk, row_area) in shown.iter().zip(rows.iter()) {
        let pct = percent(disk.used_bytes, disk.total_bytes);
        f.render_widget(
            Gauge::default()
                .gauge_style(gauge_style(pct))
                .percent(pct)
                .label(format!(
                    "{} {} / {} used ({}%), {} free",
                    disk.name,
                    fmt(disk.used_bytes),
                    fmt(disk.total_bytes),
                    pct,
                    fmt(disk.available_bytes)
                )),
            *row_area,
        );
    }
}

fn process_table(snap: &SystemSnapshot, usage: &UsageMap) -> Table<'static> {
    use ratatui::text::Span;

    let header = Row::new([
        Cell::from("PID"),
        Cell::from("NAME"),
        Cell::from("MEM"),
        Cell::from("LAST RUN"),
    ])
    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let rows: Vec<Row> = snap
        .top_processes
        .iter()
        .map(|p| {
            let last = usage
                .get(&p.name.to_lowercase())
                .map_or_else(|| "unknown".to_string(), |u| ago(u.last_run_unix, now));
            Row::new([
                Cell::from(Span::from(p.pid.to_string())),
                Cell::from(Span::from(p.name.clone())),
                Cell::from(Span::from(fmt(p.memory_bytes))),
                Cell::from(Span::from(last)),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("top processes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_handles_zero_and_overflow() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(50, 100), 50);
        assert_eq!(percent(150, 100), 100);
    }

    #[test]
    fn ago_formats_compactly() {
        let now = 1_000_000;
        assert_eq!(ago(now - 30, now), "now");
        assert_eq!(ago(now - 120, now), "2m");
        assert_eq!(ago(now - 7200, now), "2h");
        assert_eq!(ago(now - 3 * 86400, now), "3d");
        assert_eq!(ago(now + 999, now), "now");
    }
}
