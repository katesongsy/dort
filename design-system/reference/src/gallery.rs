use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::Path,
};

use ratatui::{
    buffer::{Buffer, CellWidth},
    crossterm::event::KeyCode::{self, *},
    layout::Rect,
    style::{Color, Modifier},
};

use crate::{appearance::Appearance, model, render::render};

struct Capture {
    file: &'static str,
    title: &'static str,
    size: (u16, u16),
    keys: &'static [KeyCode],
    appearance: Appearance,
}

const CAPTURES: &[Capture] = &[
    Capture {
        file: "workspace-wide.html",
        title: "Wide workspace",
        size: (100, 19),
        keys: &[],
        appearance: Appearance::Color,
    },
    Capture {
        file: "workspace-compact.html",
        title: "Compact workspace",
        size: (44, 14),
        keys: &[],
        appearance: Appearance::Color,
    },
    Capture {
        file: "details-long.html",
        title: "Long record details",
        size: (44, 25),
        keys: &[Down, Down, Down, Enter, Char(' ')],
        appearance: Appearance::Color,
    },
    Capture {
        file: "blocked.html",
        title: "Blocked record",
        size: (90, 18),
        keys: &[Down, Tab],
        appearance: Appearance::Color,
    },
    Capture {
        file: "review.html",
        title: "Restore review",
        size: (90, 19),
        keys: &[Enter],
        appearance: Appearance::Color,
    },
    Capture {
        file: "receipt.html",
        title: "Simulation receipt",
        size: (76, 16),
        keys: &[Enter, Enter, Down, Down, Down, Down, Char('y')],
        appearance: Appearance::Color,
    },
    Capture {
        file: "monochrome.html",
        title: "Monochrome workspace",
        size: (90, 18),
        keys: &[Down, Tab],
        appearance: Appearance::Mono,
    },
    Capture {
        file: "filter-empty.html",
        title: "Empty filter result",
        size: (100, 16),
        keys: &[Char('/'), Char('t'), Char('m'), Char('u'), Char('x'), Enter],
        appearance: Appearance::Color,
    },
];

pub(crate) fn write(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    if !directory.symlink_metadata()?.is_dir() {
        return Err(io::Error::other(
            "Gallery output must be a directory, not a link.",
        ));
    }
    for capture in CAPTURES {
        let (columns, rows) = capture.size;
        let area = Rect::new(0, 0, columns, rows);
        let mut app = model::demo();
        for &key in capture.keys {
            let mut buffer = Buffer::empty(area);
            let view =
                render(&mut buffer, area, &app, capture.appearance).expect("gallery viewport fits");
            app.key(key, view);
        }
        let mut buffer = Buffer::empty(area);
        render(&mut buffer, area, &app, capture.appearance);
        let html = document(&buffer, capture.title);
        write_capture(directory, capture.file, html.as_bytes())?;
    }
    Ok(())
}

fn write_capture(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let stage = directory.join(format!(".{name}.new"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)?;
    let result = file.write_all(bytes);
    drop(file);
    let result = result.and_then(|()| fs::rename(&stage, directory.join(name)));
    if result.is_err() {
        let _ = fs::remove_file(stage);
    }
    result
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CellStyle(Color, Color, Modifier);

impl CellStyle {
    fn css(self, width: u16, base: Self) -> String {
        let Self(fg, bg, attrs) = self;
        let mut colors = (
            css_color(fg, base.0, "#e4e4e4"),
            css_color(bg, base.1, "#171717"),
        );
        if attrs.contains(Modifier::REVERSED) {
            colors = (colors.1, colors.0);
        }
        let mut css = format!(
            "grid-column:span {width};color:{};background-color:{}",
            colors.0, colors.1
        );
        if attrs.contains(Modifier::BOLD) {
            css.push_str(";font-weight:700");
        }
        if attrs.contains(Modifier::UNDERLINED) {
            css.push_str(";text-decoration:underline");
        }
        css
    }
}

fn css_color(color: Color, base: Color, fallback: &str) -> String {
    let color = if color == Color::Reset { base } else { color };
    match color {
        Color::Reset => fallback.to_owned(),
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        _ => unreachable!("reference styles use RGB or the terminal default"),
    }
}

fn document(buffer: &Buffer, title: &str) -> String {
    let base = &buffer[(0, 0)];
    let base = CellStyle(base.fg, base.bg, base.modifier);
    let base_color = css_color(base.1, Color::Reset, "#171717");
    let columns = buffer.area.width;
    let rows = buffer.area.height;
    let title = escape(title);
    let mut html = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} | Dort Recovery</title>\n<style>\n\
         body{{margin:0;background:{base_color}}}\n\
         main{{width:max-content;font:13px/1.35 ui-monospace,Menlo,Consolas,monospace;\
         font-variant-ligatures:none}}\n\
         .row{{display:grid;grid-template-columns:repeat({columns},1ch);white-space:pre}}\n\
         </style>\n</head>\n<body>\n\
         <main aria-label=\"{title}\" data-columns=\"{columns}\" data-rows=\"{rows}\">\n"
    );
    for y in 0..rows {
        html.push_str("<div class=\"row\">");
        let mut x = 0;
        while x < columns {
            let first = &buffer[(x, y)];
            let style = CellStyle(first.fg, first.bg, first.modifier);
            let start = x;
            let mut symbols = String::new();
            while x < columns {
                let cell = &buffer[(x, y)];
                if style != CellStyle(cell.fg, cell.bg, cell.modifier) {
                    break;
                }
                symbols.push_str(cell.symbol());
                x += cell.cell_width().max(1);
            }
            let css = style.css(x - start, base);
            write!(html, "<span style=\"{css}\">{}</span>", escape(&symbols))
                .expect("write to string");
        }
        html.push_str("</div>\n");
    }
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn escape(text: &str) -> String {
    let mut html = String::new();
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            _ => html.push(c),
        }
    }
    html
}
