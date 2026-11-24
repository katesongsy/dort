mod appearance;
mod model;
mod render;
mod text;

use std::{env, io, process::ExitCode};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::{appearance::Appearance, model::Screen};

fn main() -> ExitCode {
    let result = match env::args_os().skip(1).collect::<Vec<_>>().as_slice() {
        [] => demo(appearance(false)),
        [arg] if arg == "--mono" => demo(appearance(true)),
        _ => Err(io::Error::other("Usage: dort-reference [--mono]")),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn appearance(mono: bool) -> Appearance {
    if mono || env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        Appearance::Mono
    } else {
        Appearance::Color
    }
}

fn demo(appearance: Appearance) -> io::Result<()> {
    if !std::io::IsTerminal::is_terminal(&io::stdin())
        || !std::io::IsTerminal::is_terminal(&io::stdout())
    {
        return Err(io::Error::other("Run the demo in a terminal."));
    }
    let mut terminal = ratatui::try_init().inspect_err(|_| {
        let _ = ratatui::try_restore();
    })?;
    let mut app = model::demo();
    let result = (|| {
        loop {
            let mut view = None;
            terminal.draw(|frame| {
                let area = frame.area();
                view = render::render(frame.buffer_mut(), area, &app, appearance);
            })?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
                continue;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break;
            }
            if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
                continue;
            }
            if key.code == KeyCode::Char('q')
                && (view.is_none() || matches!(app.screen(), Screen::Browse))
            {
                break;
            }
            if let Some(view) = view
                && (key.kind == KeyEventKind::Press
                    || !matches!(key.code, KeyCode::Enter | KeyCode::Char('y')))
            {
                app.key(key.code, view);
            }
        }
        Ok(())
    })();
    result.and(ratatui::try_restore())
}

#[cfg(test)]
mod tests;
