use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table};
use ratatui::{DefaultTerminal, Frame};

use crate::domain::models::{
    IdleSsdOffender, KillMode, MemoryStats, SystemSnapshot, TuiView,
};
use crate::services::usage_service::UsageMap;
use crate::ui::status::fmt;

pub struct DashboardData {
    pub snap: SystemSnapshot,
    pub usage: UsageMap,
}

/// actions the host app can perform from within the dashboard
#[derive(Debug, Clone)]
pub enum TuiAction {
    TrimTop(u32),
    PurgeStandby,
    /// terminate a process; the modal has already taken the user's consent
    Terminate {
        pid: u32,
        name: String,
        size_bytes: u64,
        mode: KillMode,
    },
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

pub struct Dashboard<F, G, A>
where
    F: FnMut() -> anyhow::Result<DashboardData>,
    G: FnMut() -> anyhow::Result<Vec<IdleSsdOffender>>,
    A: FnMut(TuiAction) -> anyhow::Result<String>,
{
    get_data: F,
    get_idle: G,
    act: A,
    status_line: Option<String>,
    data: Option<DashboardData>,
    /// which view is active (`b` background, `i` idle, `k` kill modal)
    view: TuiView,
    /// idle offenders, fetched lazily the first time the `i` view is opened
    idle: Vec<IdleSsdOffender>,
    /// index of the highlighted row in the active list view
    selected: usize,
    /// view to return to when the kill modal closes
    modal_return: TuiView,
}

impl<F, G, A> Dashboard<F, G, A>
where
    F: FnMut() -> anyhow::Result<DashboardData>,
    G: FnMut() -> anyhow::Result<Vec<IdleSsdOffender>>,
    A: FnMut(TuiAction) -> anyhow::Result<String>,
{
    pub fn new(get_data: F, get_idle: G, act: A) -> Self {
        Self {
            get_data,
            get_idle,
            act,
            status_line: None,
            data: None,
            view: TuiView::Background,
            idle: Vec::new(),
            selected: 0,
            modal_return: TuiView::Background,
        }
    }

    fn refresh(&mut self) {
        self.data = Some((self.get_data)().expect("dashboard refresh failed"));
    }

    /// Fetch idle offenders on demand (the scan costs about a second, so it is
    /// never part of the 2-second background refresh).
    fn refresh_idle(&mut self) {
        match (self.get_idle)() {
            Ok(list) => {
                self.idle = list;
                self.selected = 0;
            }
            Err(e) => self.status_line = Some(format!("idle scan failed: {e}")),
        }
    }

    /// Rows currently selectable in the active view.
    fn row_count(&self) -> usize {
        match self.view {
            TuiView::Idle => self.idle.len(),
            _ => self
                .data
                .as_ref()
                .map(|d| d.snap.top_processes.len())
                .unwrap_or(0),
        }
    }

    /// The (pid, name, size) under the cursor, if any.
    fn selected_target(&self) -> Option<(u32, String, u64)> {
        match self.view {
            TuiView::Idle => self
                .idle
                .get(self.selected)
                .map(|o| (o.pid, o.name.clone(), o.memory_bytes)),
            _ => self
                .data
                .as_ref()
                .and_then(|d| d.snap.top_processes.get(self.selected))
                .map(|p| (p.pid, p.name.clone(), p.memory_bytes)),
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        self.refresh();
        let mut last_refresh = Instant::now();
        loop {
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        // The modal is a consent gate: while it is open only
                        // y/n/esc are accepted, so a stray keypress can never
                        // terminate a process (FR-014).
                        if self.view == TuiView::KillModal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    self.confirm_kill();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    self.view = self.modal_return;
                                    self.status_line = Some("kill cancelled".into());
                                }
                                _ => {}
                            }
                            continue;
                        }
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('r') => {
                                self.refresh();
                                if self.view == TuiView::Idle {
                                    self.refresh_idle();
                                }
                                last_refresh = Instant::now();
                            }
                            KeyCode::Char('b') => {
                                self.view = TuiView::Background;
                                self.selected = 0;
                            }
                            KeyCode::Char('i') => {
                                self.view = TuiView::Idle;
                                if self.idle.is_empty() {
                                    self.status_line = Some("scanning for idle writers...".into());
                                    self.refresh_idle();
                                }
                            }
                            KeyCode::Char('k') => self.open_kill_modal(),
                            KeyCode::Up => {
                                self.selected = self.selected.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                let max = self.row_count().saturating_sub(1);
                                self.selected = (self.selected + 1).min(max);
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
            // The modal must not be redrawn out from under the user mid-decision.
            if last_refresh.elapsed() >= REFRESH_EVERY && self.view != TuiView::KillModal {
                self.refresh();
                last_refresh = Instant::now();
            }
        }
    }

    /// Open the kill confirmation modal for the highlighted process.
    ///
    /// System-critical processes are refused here, before the modal is ever
    /// shown, so the blocklist cannot be confirmed past (FR-011).
    fn open_kill_modal(&mut self) {
        use crate::domain::models::KillRequest;
        use crate::services::kill_service::KillService;

        let Some((pid, name, size_bytes)) = self.selected_target() else {
            self.status_line = Some("nothing selected".into());
            return;
        };
        let probe = KillRequest {
            pid,
            name: name.clone(),
            size_bytes,
            mode: KillMode::Kill,
            consent: false,
        };
        if KillService::is_blocked(&probe) {
            self.status_line = Some(format!(
                "{name} (PID {pid}) is a protected system process — refused"
            ));
            return;
        }
        self.modal_return = self.view;
        self.view = TuiView::KillModal;
    }

    /// Dispatch the confirmed termination through the host action handler.
    fn confirm_kill(&mut self) {
        let Some((pid, name, size_bytes)) = self.selected_target() else {
            self.view = self.modal_return;
            return;
        };
        let action = TuiAction::Terminate {
            pid,
            name,
            size_bytes,
            mode: KillMode::Kill,
        };
        self.status_line = Some(match (self.act)(action) {
            Ok(msg) => msg,
            Err(e) => format!("kill failed: {e}"),
        });
        self.view = self.modal_return;
        if self.view == TuiView::Idle {
            self.refresh_idle();
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

        match self.view {
            TuiView::Idle => {
                f.render_widget(idle_table(&self.idle, self.selected), table_area)
            }
            _ => f.render_widget(
                process_table(&data.snap, &data.usage, self.selected),
                table_area,
            ),
        }

        const KEYS: &str =
            "q quit | r refresh | t trim top10 | p purge standby | b background | i idle | k kill";
        let footer_text = match &self.status_line {
            Some(msg) => format!(" {msg}  |  {KEYS}"),
            None => format!(" {KEYS}"),
        };
        f.render_widget(
            Paragraph::new(footer_text).style(Style::default().fg(Color::Yellow)),
            footer_area,
        );

        if self.view == TuiView::KillModal {
            if let Some((pid, name, size_bytes)) = self.selected_target() {
                render_kill_modal(f, &name, pid, size_bytes);
            }
        }
    }
}

/// Draw the centered kill confirmation modal.
///
/// Wording mirrors the CLI prompt (FR-010) so both surfaces ask the same
/// question before any process is terminated.
fn render_kill_modal(f: &mut Frame, name: &str, pid: u32, size_bytes: u64) {
    use ratatui::layout::Rect;
    use ratatui::widgets::Clear;

    let area = f.area();
    let width = area.width.min(60).max(20);
    let height = 7u16.min(area.height);
    let modal = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    f.render_widget(Clear, modal);
    let body = format!(
        "\n kill {} PID {} {}?\n\n y = yes, n / esc = cancel",
        name,
        pid,
        fmt(size_bytes)
    );
    f.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("confirm termination"),
            ),
        modal,
    );
}

/// Idle-offender table for the `i` view.
fn idle_table(offenders: &[IdleSsdOffender], selected: usize) -> Table<'static> {
    use ratatui::text::Span;

    let header = Row::new([
        Cell::from("PID"),
        Cell::from("APP"),
        Cell::from("IDLE"),
        Cell::from("WRITE/h"),
        Cell::from("RAM"),
        Cell::from("REASON"),
    ])
    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = offenders
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let row = Row::new([
                Cell::from(Span::from(o.pid.to_string())),
                Cell::from(Span::from(o.name.clone())),
                Cell::from(Span::from(format!("{}m", o.idle_secs / 60))),
                Cell::from(Span::from(fmt(o.writes_per_hour as u64))),
                Cell::from(Span::from(fmt(o.memory_bytes))),
                Cell::from(Span::from(o.reason.to_string())),
            ]);
            if i == selected {
                row.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                row
            }
        })
        .collect();

    let title = if offenders.is_empty() {
        "idle writers (none detected)"
    } else {
        "idle writers — k to close/kill"
    };

    Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(16),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
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

fn process_table(snap: &SystemSnapshot, usage: &UsageMap, selected: usize) -> Table<'static> {
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
        .enumerate()
        .map(|(i, p)| {
            let last = usage
                .get(&p.name.to_lowercase())
                .map_or_else(|| "unknown".to_string(), |u| ago(u.last_run_unix, now));
            let row = Row::new([
                Cell::from(Span::from(p.pid.to_string())),
                Cell::from(Span::from(p.name.clone())),
                Cell::from(Span::from(fmt(p.memory_bytes))),
                Cell::from(Span::from(last)),
            ]);
            if i == selected {
                row.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                row
            }
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
