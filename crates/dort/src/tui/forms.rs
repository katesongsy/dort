//! Visible field/button controls backed by the existing retained draft state.
use super::{app::App, components as ui, shell::Form};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

enum Control<'a> {
    Field(&'a str, &'a str),
    Button(&'a str),
}
impl Control<'_> {
    fn height(&self) -> u16 {
        if matches!(self, Self::Field(..)) {
            3
        } else {
            1
        }
    }
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, form: Form) {
    let shell = &app.shell;
    let (title, focused, controls, help): (&str, usize, Vec<Control<'_>>, &str) = match form {
        Form::Adopt => (
            "Adopt file · 1 Details > 2 Review > 3 Confirm",
            shell.adoption.focused,
            vec![
                Control::Field("TARGET", &shell.adoption.target),
                Control::Field("ID", &shell.adoption.id),
                Control::Field("SOURCE", &shell.adoption.source),
                Control::Button("Review read-only preview"),
                Control::Button("Choose repository"),
                Control::Button("Create/attach local repository"),
                Control::Button("Discard adoption draft"),
            ],
            "SOURCE is repository-relative; mapping belongs to base. Review privately before adoption.",
        ),
        Form::Check => (
            "Check standalone manifest",
            shell.manifest_focused,
            vec![
                Control::Field("Manifest path", &shell.manifest_draft),
                Control::Button("Check manifest"),
                Control::Button("Temporary profiles"),
                Control::Button("Discard standalone draft and additions"),
            ],
            "Structural check only. No repository or Git needed; live files/tools not inspected.",
        ),
        Form::KeepId | Form::RemoveId => (
            if form == Form::KeepId {
                "Keep local file"
            } else {
                "Delete managed link"
            },
            shell.unmanage_focused,
            vec![
                Control::Field("Exact ID", &shell.unmanage_id),
                Control::Button("Review read-only preview"),
            ],
            "Both modes retain the source. Deletion requires exact-ID acknowledgement, then literal yes.",
        ),
        Form::Choose | Form::Init => (
            if form == Form::Choose {
                "Choose exact root · Session only"
            } else {
                "Create/attach local repository"
            },
            shell.path_focused,
            vec![
                Control::Field("Path", shell.draft(form)),
                Control::Button(if form == Form::Choose {
                    "Select repository"
                } else {
                    "Review read-only preview"
                }),
                Control::Button("Back"),
            ],
            "Exact path; relative to launch directory. No tilde/environment expansion. Enter advances to the action.",
        ),
    };
    let block = ui::pane(title, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let total_height: u16 = controls.iter().map(Control::height).sum();
    let budget = inner.height.saturating_sub(2).max(3);
    let mut start = 0;
    if total_height > budget {
        let mut needed = 0;
        for i in (0..=focused.min(controls.len() - 1)).rev() {
            if needed + controls[i].height() > budget {
                break;
            }
            needed += controls[i].height();
            start = i;
        }
    }
    let mut y = inner.y;
    for (i, control) in controls.iter().enumerate().skip(start) {
        let h = control.height();
        if y + h > inner.bottom().saturating_sub(1) {
            break;
        }
        let r = Rect::new(inner.x, y, inner.width, h);
        match control {
            Control::Field(label, value) => {
                ui::input(frame, r, label, value, focused == i && !app.busy)
            }
            Control::Button(label) => frame.render_widget(
                Paragraph::new(Line::from(ui::button(label, "", focused == i))),
                r,
            ),
        }
        y += h;
    }
    if y < inner.bottom().saturating_sub(1) {
        ui::prose(
            frame,
            Rect::new(
                inner.x,
                y,
                inner.width,
                inner.bottom().saturating_sub(y + 1),
            ),
            help,
            ui::muted(),
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Control {}/{}  ", focused + 1, controls.len()),
                ui::muted(),
            ),
            Span::raw("Tab next · PgDn full details · Esc back"),
        ])),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
