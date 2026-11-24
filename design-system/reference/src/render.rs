use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    appearance::{Appearance, Role},
    model::{App, Receipt, Record, RestorePlan, ReviewDecision, Screen, Status, ViewContext},
    text::{clip, visible, width, wrap},
};

const MARGIN: u16 = 2;
const QUEUE_MIN: u16 = 30;
const RECORD_MIN: u16 = 51;
const GUTTER: u16 = 5;

struct Viewport {
    inner: Rect,
    body: Rect,
    split: Option<(Rect, Rect)>,
}

impl Viewport {
    fn new(area: Rect) -> Option<Self> {
        if area.width < 32 || area.height < 11 {
            return None;
        }
        let inner = Rect::new(
            area.x + MARGIN,
            area.y,
            area.width - 2 * MARGIN,
            area.height,
        );
        let body = Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 6);
        let split = (body.width >= QUEUE_MIN + GUTTER + RECORD_MIN).then(|| {
            let left = ((body.width - GUTTER) / 3).clamp(QUEUE_MIN, 38);
            let queue = Rect {
                width: left,
                ..body
            };
            let record = Rect::new(
                body.x + left + GUTTER,
                body.y,
                body.width - left - GUTTER,
                body.height,
            );
            (queue, record)
        });
        Some(Self { inner, body, split })
    }
}

struct Canvas<'a> {
    buffer: &'a mut Buffer,
    appearance: Appearance,
}

impl Canvas<'_> {
    fn line(&mut self, area: Rect, row: u16, text: &str, role: Role) {
        if row < area.height {
            self.buffer.set_stringn(
                area.x,
                area.y + row,
                text,
                area.width as usize,
                self.appearance.style(role),
            );
        }
    }

    fn right(&mut self, area: Rect, row: u16, text: &str, role: Role) {
        let cells = width(text).min(area.width as usize) as u16;
        self.line(
            Rect {
                x: area.right() - cells,
                width: cells,
                ..area
            },
            row,
            text,
            role,
        );
    }

    fn section(&mut self, area: Rect, title: &str, active: bool) -> Rect {
        self.line(area, 0, title, Role::Heading);
        if active {
            self.right(area, 0, "[Active]", Role::Focus);
        }
        below(area, 1)
    }

    fn rule(&mut self, area: Rect, row: u16) {
        self.line(area, row, &"─".repeat(area.width as usize), Role::Rule);
    }
}

fn below(area: Rect, rows: u16) -> Rect {
    let rows = rows.min(area.height);
    Rect {
        y: area.y + rows,
        height: area.height - rows,
        ..area
    }
}

#[derive(Default)]
struct Reader(Vec<Vec<(u16, Role, String)>>);

impl Reader {
    fn blank(&mut self) {
        self.0.push(Vec::new());
    }

    fn text(&mut self, area: Rect, role: Role, value: &str) {
        for line in wrap(&visible(value), area.width as usize) {
            self.0.push(vec![(0, role, line.to_owned())]);
        }
    }

    fn field(&mut self, area: Rect, label: &str, role: Role, value: &str) {
        const INDENT: u16 = 10;
        for (index, line) in wrap(&visible(value), (area.width - INDENT) as usize)
            .into_iter()
            .enumerate()
        {
            let mut spans = Vec::new();
            if index == 0 {
                spans.push((0, Role::Label, label.to_owned()));
            }
            spans.push((INDENT, role, line.to_owned()));
            self.0.push(spans);
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, offset: usize) -> Scroll {
        let mut visible_rows = area.height as usize;
        let overflow = self.0.len() > visible_rows;
        if overflow {
            visible_rows = visible_rows.saturating_sub(1);
        }
        let limit = self.0.len().saturating_sub(visible_rows);
        let first = offset.min(limit);
        for (row, spans) in self.0.iter().skip(first).take(visible_rows).enumerate() {
            for (indent, role, text) in spans {
                let span = Rect {
                    x: area.x + indent,
                    width: area.width - indent,
                    ..area
                };
                canvas.line(span, row as u16, text, *role);
            }
        }
        if overflow && area.height > 0 {
            let progress = format!(
                "Lines {}–{} of {}",
                first + 1,
                first + visible_rows,
                self.0.len()
            );
            canvas.line(area, area.height - 1, &progress, Role::Note);
        }
        Scroll {
            limit,
            more_below: first < limit,
        }
    }
}

#[derive(Default)]
struct Scroll {
    limit: usize,
    more_below: bool,
}

fn queue(canvas: &mut Canvas<'_>, area: Rect, app: &App, active: bool) {
    let area = canvas.section(area, "Recovery queue", active);
    let matches = app.matches();
    let selected = app.visible_selection(&matches);
    let filtering = matches!(app.screen(), Screen::Filter(_));
    let prefix = if filtering {
        "/ Filter › "
    } else {
        "/ Filter   "
    };
    let query = if app.filter().is_empty() && !filtering {
        "All records".into()
    } else {
        visible(app.filter())
    };
    canvas.line(
        area,
        0,
        prefix,
        if filtering { Role::Focus } else { Role::Label },
    );
    let filter = Rect {
        x: area.x + 11,
        width: area.width - 11,
        ..area
    };
    canvas.line(filter, 0, &clip(&query, filter.width as usize), Role::Value);

    let rows = area.height.saturating_sub(2) as usize;
    let index = matches
        .iter()
        .position(|id| Some(*id) == selected)
        .unwrap_or_default();
    let first = index.saturating_sub(rows.saturating_sub(1));
    let last = (first + rows).min(matches.len());
    let count = if matches.len() > rows {
        format!("{}–{} of {} matches", first + 1, last, matches.len())
    } else {
        format!("{} of {} records", matches.len(), app.record_count())
    };
    canvas.line(area, 1, &count, Role::Note);
    let area = below(area, 2);
    if matches.is_empty() {
        let mut empty = Reader::default();
        empty.text(area, Role::Value, "No records match this filter.");
        empty.draw(canvas, area, 0);
        return;
    }
    for (row, id) in matches[first..last].iter().enumerate() {
        let record = app.record(*id);
        let current = Some(*id) == selected;
        let title = format!(
            "{}{}",
            if current { "› " } else { "  " },
            visible(record.title)
        );
        let status = record.status.label();
        let title_limit = area.width as usize - width(status) - 1;
        let title = clip(&title, title_limit);
        if current {
            canvas.line(
                area,
                row as u16,
                &" ".repeat(area.width as usize),
                Role::Selected,
            );
        }
        canvas.line(
            area,
            row as u16,
            &title,
            if current { Role::Selected } else { Role::Value },
        );
        canvas.right(area, row as u16, status, Role::status(record.status));
    }
}

fn details(
    canvas: &mut Canvas<'_>,
    area: Rect,
    app: &App,
    active: bool,
    record: Option<&Record>,
) -> Scroll {
    let area = canvas.section(area, "Record", active);
    let mut reader = Reader::default();
    if let Some(record) = record {
        reader.text(area, Role::Label, record.title);
        reader.blank();
        reader.field(area, "Target", Role::Value, record.target);
        reader.field(
            area,
            "Snapshot",
            Role::Value,
            &format!("{} · {}", record.snapshot, record.date),
        );
        reader.field(
            area,
            "Status",
            Role::status(record.status),
            record.status.label(),
        );
        reader.blank();
        reader.text(area, Role::Heading, "Recovery check");
        match record.status {
            Status::Ready => reader.field(
                area,
                "Warning",
                Role::Warning,
                "Local edits may be replaced.",
            ),
            Status::Blocked { reason, next } => {
                reader.field(area, "Blocker", Role::Blocker, reason);
                reader.field(area, "Next", Role::Value, next);
            }
            Status::Finished => reader.text(area, Role::Value, "No further action needed."),
        }
        if app.show_origin() {
            reader.blank();
            reader.text(area, Role::Heading, "Provenance");
            reader.field(area, "Tool", Role::Value, record.tool);
            reader.field(area, "Origin", Role::Value, record.origin);
        }
    } else {
        reader.text(area, Role::Value, "Change the filter to find a record.");
    }
    reader.draw(canvas, area, app.details_scroll())
}

fn review(canvas: &mut Canvas<'_>, area: Rect, app: &App, plan: &RestorePlan) -> Scroll {
    let area = canvas.section(area, "Review restore", true);
    let mut reader = Reader::default();
    reader.text(area, Role::Label, plan.record(app).title);
    reader.blank();
    reader.text(area, Role::Heading, "Planned effects");
    reader.field(
        area,
        "Source",
        Role::Value,
        &format!("Snapshot {}", plan.snapshot),
    );
    reader.field(area, "Replace", Role::Value, plan.target);
    reader.field(area, "Back up", Role::Value, &plan.backup_path());
    reader.field(
        area,
        "Condition",
        Role::Value,
        "Back up only if the target exists.",
    );
    reader.field(area, "Stop if", Role::Warning, "The backup already exists.");
    reader.field(area, "Keep", Role::Value, "All other files unchanged.");
    reader.blank();
    reader.text(area, Role::Heading, "Execution");
    reader.text(area, Role::Value, "Simulation only. No files will change.");
    reader.draw(canvas, area, app.modal_scroll())
}

fn receipt(canvas: &mut Canvas<'_>, area: Rect, app: &App, receipt: &Receipt) -> Scroll {
    let area = canvas.section(area, "Receipt", true);
    let record = receipt.record(app);
    let mut reader = Reader::default();
    reader.text(area, Role::Label, "Restore simulated");
    reader.blank();
    reader.field(area, "Record", Role::Value, record.title);
    reader.field(area, "Status", Role::Success, "Finished");
    reader.field(
        area,
        "Mock ID",
        Role::Value,
        &format!("{} / attempt 1", record.snapshot),
    );
    reader.field(area, "Writes", Role::Value, "None");
    reader.blank();
    reader.text(
        area,
        Role::Value,
        "The queue now marks this record Finished.",
    );
    reader.draw(canvas, area, app.modal_scroll())
}

pub(crate) fn render(
    buffer: &mut Buffer,
    area: Rect,
    app: &App,
    appearance: Appearance,
) -> Option<ViewContext> {
    buffer.set_style(area, appearance.style(Role::Value));
    let mut canvas = Canvas { buffer, appearance };
    let Some(viewport) = Viewport::new(area) else {
        canvas.line(area, 0, "Resize (min 32 × 11). q Quit", Role::Warning);
        return None;
    };
    let Viewport { inner, body, split } = viewport;
    canvas.line(inner, 1, "◇  Dort Recovery", Role::Heading);
    canvas.right(inner, 1, "SIMULATION", Role::Note);
    canvas.rule(inner, 2);

    let (scroll, primary, navigation) = match app.screen() {
        Screen::Review { plan, .. } => {
            let scroll = review(&mut canvas, body, app, plan);
            let primary = if scroll.more_below {
                "↓  Read remaining effects"
            } else {
                "y  Simulate restore"
            };
            let navigation = if scroll.limit > 0 {
                "Esc Cancel   ↑↓ Scroll"
            } else {
                "Esc Cancel"
            };
            (scroll, primary, navigation)
        }
        Screen::Receipt {
            receipt: result, ..
        } => {
            let scroll = receipt(&mut canvas, body, app, result);
            let navigation = if scroll.limit > 0 {
                "↑↓ Scroll"
            } else {
                ""
            };
            (scroll, "Esc  Back to workspace", navigation)
        }
        Screen::Browse | Screen::Filter(_) | Screen::Details => {
            let active = matches!(app.screen(), Screen::Details);
            let selection = app.visible_selection(&app.matches());
            let record = selection.map(|id| app.record(id));
            let scroll = if let Some((list_area, record_area)) = split {
                let divider = Rect {
                    x: list_area.right() + GUTTER / 2,
                    width: 1,
                    ..body
                };
                for row in 0..body.height {
                    canvas.line(divider, row, "│", Role::Rule);
                }
                queue(&mut canvas, list_area, app, !active);
                details(&mut canvas, record_area, app, active, record)
            } else if active {
                details(&mut canvas, body, app, true, record)
            } else {
                queue(&mut canvas, body, app, true);
                Scroll::default()
            };
            let ready =
                selection.is_some_and(|id| matches!(app.decision(id), ReviewDecision::Ready(_)));
            let (primary, navigation) = match app.screen() {
                Screen::Filter(_) => ("Enter  Apply filter", "Type to filter   Esc Cancel"),
                Screen::Details => {
                    let primary = if ready { "Enter  Review restore" } else { "" };
                    let navigation = match (split.is_some(), app.show_origin()) {
                        (true, false) => "Tab Queue   ↑↓ Scroll   Space Show origin",
                        (true, true) => "Tab Queue   ↑↓ Scroll   Space Hide origin",
                        (false, false) => "Esc Queue  ↑↓ Scroll  Space Show origin",
                        (false, true) => "Esc Queue  ↑↓ Scroll  Space Hide origin",
                    };
                    (primary, navigation)
                }
                Screen::Browse if split.is_some() => {
                    if record.is_none() {
                        ("/  Change filter", "q Quit")
                    } else {
                        let primary = if ready { "Enter  Review restore" } else { "" };
                        (primary, "↑↓ Move   / Filter   Tab Details   q Quit")
                    }
                }
                Screen::Browse => {
                    if record.is_none() {
                        ("/  Change filter", "q Quit")
                    } else {
                        ("Enter  Open details", "↑↓ Move   / Filter   q Quit")
                    }
                }
                _ => unreachable!(),
            };
            (scroll, primary, navigation)
        }
    };
    canvas.rule(inner, inner.height - 3);
    if !primary.is_empty() {
        canvas.line(
            inner,
            inner.height - 2,
            &format!(" {primary} "),
            Role::Primary,
        );
    }
    let navigation =
        if width(navigation) > inner.width as usize && matches!(app.screen(), Screen::Details) {
            "Esc Queue  ↑↓  Space Origin"
        } else {
            navigation
        };
    canvas.line(inner, inner.height - 1, navigation, Role::Note);
    Some(ViewContext {
        split: split.is_some(),
        scroll_limit: scroll.limit,
    })
}
