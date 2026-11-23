//! Actual-component gallery and semantic cell checks; fixture-only, no provider processes.
use super::{
    appearance::Appearance,
    components::{
        self as ui,
        design::{self, PairRow, PairTable, StripAction, Tone},
    },
    input::{Hits, Target},
    shell::Action,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::Paragraph,
};

fn gallery(width: u16, height: u16, ascii: bool, mono: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let hits = Hits::default();
    terminal
        .draw(|frame| {
            let rect = frame.area();
            hits.begin(rect);
            frame.render_widget(
                Paragraph::new("Shared components / fixture").style(ui::heading()),
                Rect::new(0, 0, width, 1),
            );
            design::filter(
                frame,
                Rect::new(0, 1, width, 1),
                "Filter packages",
                "rg",
                true,
            );
            let rows = [
                PairRow {
                    identity: 0,
                    values: ["node", "22.11.0"],
                },
                PairRow {
                    identity: 1,
                    values: ["ripgrep", "14.1.0"],
                },
            ];
            design::pairs(
                frame,
                Rect::new(0, 3, width, 4),
                PairTable {
                    headers: ["PACKAGE", "VERSION"],
                    rows: &rows,
                    selected: 1,
                },
                &hits,
                Target::ToolsRow,
            );
            frame.render_widget(
                Paragraph::new(design::badge("Manifest saved", Tone::Success)),
                Rect::new(0, 7, width, 1),
            );
            frame.render_widget(
                Paragraph::new(design::badge("Installer failed", Tone::Error)),
                Rect::new(0, 8, width, 1),
            );
            frame.render_widget(
                Paragraph::new(design::badge("Package reported present", Tone::Information)),
                Rect::new(0, 9, width, 1),
            );
            design::notice(
                frame,
                Rect::new(0, 10, width, height.saturating_sub(11)),
                "Root/native effects",
                "No automatic rollback. Command readiness is not verified.",
                Tone::Attention,
            );
            design::action_strip(
                frame,
                Rect::new(0, height - 1, width, 1),
                &[
                    StripAction {
                        key: "Enter",
                        label: "Details",
                        target: Target::Continue,
                        primary: true,
                    },
                    StripAction {
                        key: "F2",
                        label: "More",
                        target: Target::Back,
                        primary: false,
                    },
                    StripAction {
                        key: "F1",
                        label: "Help",
                        target: Target::Action(Action::Help),
                        primary: false,
                    },
                ],
                &hits,
            );
        })
        .unwrap();
    let mut buffer = terminal.backend().buffer().clone();
    Appearance::classify(
        Some("xterm-256color"),
        None,
        None,
        Some(if ascii { "C" } else { "C.UTF-8" }),
        if mono { Some("1") } else { None },
    )
    .apply(&mut buffer);
    buffer
}
fn workflow_gallery(kind: &str, width: u16, height: u16, ascii: bool, mono: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let hits = Hits::default();
    terminal.draw(|frame| {
        hits.begin(frame.area());
        let areas = ui::chrome::shell(frame, "Fixture", super::app::Tab::Tools, false, false, &hits);
        let mut footer = areas.footer;
        match kind {
            "Help" => {
                let dialog = design::choice_dialog(frame, "Help", &hits);
                let area = dialog.body;
                footer = dialog.footer;
                ui::report(frame, area, "", ui::help::document(
                    area.width, "Tools", "Inspect metadata before choosing a change.",
                    &[("Enter", "Current primary action"), ("Esc", "Return with drafts retained")],
                    &[("Privacy", "No provider runs merely by opening this dialog.".into())],
                ), 0, false);
            }
            "Choices" => {
                let dialog = design::choice_dialog(frame, "Choose package manager", &hits);
                footer = dialog.footer;
                ui::choice::draw(frame, dialog.body, &hits, ui::choice::View {
                    rows: &[("Homebrew", "available"), ("npm", "unavailable")],
                    selected: 1, details: false, scroll: 0, feedback: None,
                });
            }
            "More" => {
                let dialog = design::choice_dialog(frame, "More", &hits);
                footer = dialog.footer;
                ui::menu::draw(frame, dialog.body, &[
                    ui::menu::Item { action: ui::ActionButton { label: "Details", key: "F3", target: Target::Back }, reason: None },
                    ui::menu::Item { action: ui::ActionButton { label: "Dependencies", key: "F4", target: Target::Back }, reason: None },
                ], 0, false, None, &hits);
            }
            "Dashboard" => ui::dashboard::draw(frame, areas.body, "Scope facts", &[
                ui::dashboard::Metric { label: "Linked", value: "4 linked".into(), compact: "4 linked".into(), style: ui::success() },
                ui::dashboard::Metric { label: "Unavailable", value: "1 unavailable".into(), compact: "1 unavailable".into(), style: ui::warning() },
            ], ui::review_text("Repository: /fixture/repository\nProfiles: default\nUnavailable is not an empty observation."), 0),
            "Reader" => ui::report(frame, areas.body, "Operation details", ui::review_text(
                "Repository: /fixture/repository\nOperation: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nState: unresolved\nClosing does not repair or roll back effects."
            ), 0, true),
            "Result" => {
                let lines = vec![
                    design::badge("Installer failed", Tone::Error),
                    ui::metadata("Process", "failed"),
                    ui::metadata("Package", "reported present"),
                    ui::metadata("Binary", "unavailable"),
                    ui::metadata("Readiness", "not verified"),
                ];
                ui::report(frame, areas.body, "Installation outcome", ratatui::text::Text::from(lines), 0, true);
            }
            _ => unreachable!(),
        }
        let dialog = matches!(kind, "Help" | "Choices" | "More");
        let primary = match kind { "Choices" => "Choose", "More" => "Open", "Result" => "Fresh review", "Dashboard" => "Apply", _ => "Back" };
        let mut actions = vec![StripAction { key: if primary == "Back" { "Esc" } else { "Enter" }, label: primary, target: Target::Back, primary: primary != "Back" }];
        if !dialog {
            actions.extend([
                StripAction { key: "F2", label: "More", target: Target::More, primary: false },
                StripAction { key: "F1", label: "Help", target: Target::Action(Action::Help), primary: false },
            ]);
        }
        design::action_strip(frame, footer, &actions, &hits);
    }).unwrap();
    let mut buffer = terminal.backend().buffer().clone();
    Appearance::classify(
        Some("xterm-256color"),
        None,
        None,
        Some(if ascii { "C" } else { "C.UTF-8" }),
        if mono { Some("1") } else { None },
    )
    .apply(&mut buffer);
    buffer
}

#[test]
fn workflow_components_render_at_every_supported_gallery_size() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for (ascii, mono) in [(false, false), (true, true)] {
            for kind in ["Help", "Choices", "More", "Dashboard", "Reader", "Result"] {
                let buffer = workflow_gallery(kind, w, h, ascii, mono);
                let rendered = text(&buffer);
                assert!(rendered.contains("Dort / Fixture"), "{kind} {w}");
                let footer: String = (0..w).map(|x| buffer[(x, h - 1)].symbol()).collect();
                assert_eq!(
                    footer.trim().is_empty(),
                    matches!(kind, "Help" | "Choices" | "More"),
                    "{kind} {w}"
                );
                if kind == "Help" {
                    assert!(
                        rendered.contains("Keys in this context")
                            && rendered.contains("Current primary action")
                    );
                    assert!(buffer.content.windows(5).any(|cells| {
                        cells.iter().map(|cell| cell.symbol()).collect::<String>() == "Enter"
                            && cells
                                .iter()
                                .all(|cell| cell.modifier.contains(Modifier::REVERSED))
                    }));
                }
                if kind == "Result" {
                    for fact in [
                        "Installer failed",
                        "reported present",
                        "unavailable",
                        "not verified",
                    ] {
                        assert!(rendered.contains(fact), "{w}: {fact}");
                    }
                }
            }
        }
    }
}

#[test]
fn compact_shell_keeps_dry_run_and_tools_help_visible() {
    let mut app = super::app::App::new(true);
    app.tab = super::app::Tab::Tools;
    let rendered = super::tests::render(&app, 48, 14);
    let lines: Vec<_> = rendered.lines().collect();
    assert!(lines[0].contains("[DRY RUN]"));
    assert!(lines.last().unwrap().contains("F1 Help"));
    app.tools_view.settings_open = true;
    let rendered = super::tests::render(&app, 48, 14);
    assert!(rendered.lines().next().unwrap().contains("[DRY RUN]"));
}

fn dialog_gallery(width: u16, height: u16, ascii: bool, mono: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let hits = Hits::default();
    terminal
        .draw(|frame| {
            let area = frame.area();
            hits.begin(area);
            frame.render_widget(
                Paragraph::new("Tools / fixture background").style(ui::heading()),
                area,
            );
            let dialog = design::choice_dialog(frame, "More actions", &hits);
            let rows = [
                PairRow {
                    identity: 0,
                    values: ["Inspection details", "Metadata"],
                },
                PairRow {
                    identity: 1,
                    values: ["Inspect again", "Review"],
                },
            ];
            design::pairs(
                frame,
                dialog.body,
                PairTable {
                    headers: ["ACTION", "EFFECT"],
                    rows: &rows,
                    selected: 0,
                },
                &hits,
                Target::MoreRow,
            );
            design::action_strip(
                frame,
                dialog.footer,
                &[
                    StripAction {
                        key: "Enter",
                        label: "Open",
                        target: Target::Continue,
                        primary: true,
                    },
                    StripAction {
                        key: "Esc",
                        label: "Back",
                        target: Target::Back,
                        primary: false,
                    },
                    StripAction {
                        key: "F1",
                        label: "Help",
                        target: Target::Action(Action::Help),
                        primary: false,
                    },
                ],
                &hits,
            );
            assert!(!hits.input_focus_allowed());
            assert!(matches!(hits.target(0, 0), Some(Target::DismissDialog)));
            assert!(matches!(
                hits.target(0, height - 1),
                Some(Target::DismissDialog)
            ));
            assert!(hits.target(dialog.body.x, dialog.body.y - 1).is_none());
            assert!(matches!(
                hits.target(dialog.footer.x, dialog.footer.y),
                Some(Target::Continue)
            ));
        })
        .unwrap();
    let mut buffer = terminal.backend().buffer().clone();
    Appearance::classify(
        Some("xterm"),
        None,
        None,
        Some(if ascii { "C" } else { "C.UTF-8" }),
        if mono { Some("1") } else { None },
    )
    .apply(&mut buffer);
    buffer
}

#[test]
fn foreground_gallery_captures_background_and_has_one_action_strip() {
    for (w, h) in [(48, 14), (80, 24), (140, 40), (180, 48)] {
        for (ascii, mono) in [(false, false), (true, true)] {
            let buffer = dialog_gallery(w, h, ascii, mono);
            let rendered = text(&buffer);
            assert!(rendered.contains("More actions") && rendered.contains("Inspection details"));
            assert_eq!(rendered.matches("F1 Help").count(), 1);
            let rows: Vec<_> = rendered.lines().collect();
            let title = rows
                .iter()
                .position(|row| row.contains("More actions"))
                .unwrap();
            let footer = rows
                .iter()
                .position(|row| row.contains("Esc Back"))
                .unwrap();
            assert_eq!(footer - title + 2, usize::from((h - 2).min(32)));
            assert!(rows[footer + 1].trim().chars().count() >= usize::from((w - 2).min(144)));
            assert!(footer + 1 < usize::from(h - 1));
            if ascii {
                assert!(rendered.is_ascii());
            }
        }
    }
}

fn text(buffer: &Buffer) -> String {
    buffer
        .content
        .chunks(buffer.area.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn component_roles_survive_compact_ascii_and_monochrome_without_private_data() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for ascii in [false, true] {
            for mono in [false, true] {
                let buffer = gallery(w, h, ascii, mono);
                let rendered = text(&buffer);
                for label in [
                    "PACKAGE",
                    "VERSION",
                    "ripgrep",
                    "14.1.0",
                    "Manifest saved",
                    "Installer failed",
                    "Package reported present",
                    "Root/native effects",
                    "Enter Details",
                    "F2 More",
                    "F1 Help",
                ] {
                    assert!(rendered.contains(label), "missing {label}: {rendered}");
                }
                assert!(rendered.lines().last().unwrap().contains("Enter Details"));
                let footer = usize::from((h - 1) * w);
                assert!(
                    buffer.content[footer + 1]
                        .modifier
                        .contains(Modifier::BOLD | Modifier::REVERSED)
                );
                let secondary_label = " Enter Details ".len() + 1 + " F2 ".len();
                assert!(
                    !buffer.content[footer + secondary_label]
                        .modifier
                        .contains(Modifier::BOLD)
                );
                assert!(
                    buffer
                        .content
                        .iter()
                        .any(|cell| cell.modifier.contains(Modifier::REVERSED))
                );
                if ascii {
                    assert!(rendered.is_ascii());
                }
                if mono {
                    assert!(
                        buffer
                            .content
                            .iter()
                            .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
                    );
                } else {
                    assert!(buffer.content.iter().any(|cell| cell.fg == Color::Red));
                    assert!(buffer.content.iter().any(|cell| cell.fg == Color::Yellow));
                }
            }
        }
    }
}
#[test]
fn shared_filter_escapes_metadata_and_positions_its_focused_caret() {
    let mut terminal = Terminal::new(TestBackend::new(80, 4)).unwrap();
    terminal
        .draw(|frame| {
            design::filter(
                frame,
                Rect::new(0, 0, 80, 1),
                "Filter",
                "x\u{202e}\u{1b}",
                true,
            );
        })
        .unwrap();
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("\\u{202e}") && rendered.contains("\\u{1b}"));
    assert!(!rendered.contains('\u{202e}') && !rendered.contains('\u{1b}'));
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.fg == Color::Cyan)
    );
    let expected_x = 8 + ui::inline("x\u{202e}\u{1b}").len() as u16;
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        (expected_x, 0).into()
    );
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn css(color: Color, background: bool) -> String {
    match color {
        Color::Reset => if background { "#10141c" } else { "#e6edf3" }.into(),
        Color::Black => "#10141c".into(),
        Color::Red | Color::LightRed => "#ff8a93".into(),
        Color::Green | Color::LightGreen => "#7bd99b".into(),
        Color::Yellow | Color::LightYellow => "#f2c66d".into(),
        Color::Blue | Color::LightBlue => "#98b8ff".into(),
        Color::Magenta | Color::LightMagenta => "#c9a5f5".into(),
        Color::Cyan | Color::LightCyan => "#66d9ef".into(),
        Color::White | Color::Gray => "#e6edf3".into(),
        Color::DarkGray => "#94a3b8".into(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(_) => "#a9b6c7".into(),
    }
}
pub(super) fn html(buffer: &Buffer) -> String {
    let mut output = String::from("<pre>");
    for row in buffer.content.chunks(buffer.area.width as usize) {
        for cell in row {
            let mut fg = css(cell.fg, false);
            let mut bg = css(cell.bg, true);
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let weight = if cell.modifier.contains(Modifier::BOLD) {
                "bold"
            } else {
                "normal"
            };
            let underline = if cell.modifier.contains(Modifier::UNDERLINED) {
                "underline"
            } else {
                "none"
            };
            output.push_str(&format!("<span style=\"color:{fg};background:{bg};font-weight:{weight};text-decoration:{underline}\">{}</span>", escape(cell.symbol())));
        }
        output.push('\n');
    }
    output.push_str("</pre>");
    output
}
/// Optional isolated-child export. No normal test writes a gallery artifact.
#[test]
fn component_gallery_export_probe() {
    let Ok(directory) = std::env::var("DORT_TUI_GALLERY_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(directory);
    assert!(path.is_absolute() && path.canonicalize().unwrap() == path);
    let allowed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../temp")
        .canonicalize()
        .unwrap();
    assert!(
        path.starts_with(&allowed) && path != allowed,
        "gallery exports stay below repository temp/"
    );
    let mut output = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Dort actual component gallery</title><style>body{background:#10141c;color:#e6edf3;padding:24px;font:14px monospace}pre{line-height:1.35;overflow:auto;border:1px solid #46556b;padding:8px}h2{font-size:16px}</style><h1>Actual Ratatui component gallery</h1><p>Fixture-only cells from shared components. Reference colors, not a screenshot of a user's terminal. No real provider or application data. Inspect changes; exporting does not approve a visual baseline.</p>",
    );
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for (ascii, mono) in [(false, false), (true, true)] {
            output.push_str(&format!(
                "<h2>{w}×{h} — ASCII {ascii}, monochrome {mono}</h2>"
            ));
            output.push_str(&html(&gallery(w, h, ascii, mono)));
            output.push_str("<h2>Foreground choice / same action-strip location</h2>");
            output.push_str(&html(&dialog_gallery(w, h, ascii, mono)));
            for kind in ["Help", "Choices", "More", "Dashboard", "Reader", "Result"] {
                output.push_str(&format!("<h2>{kind} / actual shared components</h2>"));
                output.push_str(&html(&workflow_gallery(kind, w, h, ascii, mono)));
            }
        }
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join("components.html"))
        .unwrap();
    file.write_all(output.as_bytes()).unwrap();
}
