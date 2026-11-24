use ratatui::style::{Color, Modifier, Style};

use crate::model::Status;

#[derive(Clone, Copy)]
pub(crate) enum Appearance {
    Color,
    Mono,
}

#[derive(Clone, Copy)]
pub(crate) enum Role {
    Value,
    Note,
    Rule,
    Heading,
    Label,
    Focus,
    Selected,
    Primary,
    Success,
    Warning,
    Blocker,
}

impl Role {
    pub(crate) fn status(status: Status) -> Self {
        match status {
            Status::Ready => Self::Value,
            Status::Blocked { .. } => Self::Blocker,
            Status::Finished => Self::Success,
        }
    }
}

impl Appearance {
    pub(crate) fn style(self, role: Role) -> Style {
        use Role::*;
        let bold = Modifier::BOLD;
        let style = Style::default();
        match self {
            Self::Mono => {
                let modifier = match role {
                    Heading | Focus => bold | Modifier::UNDERLINED,
                    Selected | Primary => bold | Modifier::REVERSED,
                    Label | Success | Warning | Blocker => bold,
                    Value | Note | Rule => Modifier::empty(),
                };
                style.add_modifier(modifier)
            }
            Self::Color => {
                let ink = Color::Rgb(217, 226, 234);
                let cyan = Color::Rgb(130, 219, 234);
                let style = style.bg(Color::Rgb(19, 28, 35));
                match role {
                    Value => style.fg(ink),
                    Note => style.fg(Color::Rgb(158, 175, 189)),
                    Rule => style.fg(Color::Rgb(79, 98, 112)),
                    Heading => style.fg(cyan).add_modifier(bold),
                    Focus => style.fg(cyan).add_modifier(bold | Modifier::UNDERLINED),
                    Label => style.fg(Color::Rgb(243, 246, 249)).add_modifier(bold),
                    Selected => style.fg(ink).bg(Color::Rgb(38, 54, 70)).add_modifier(bold),
                    Primary => style.fg(Color::Rgb(17, 36, 43)).bg(cyan).add_modifier(bold),
                    Success => style.fg(Color::Rgb(155, 218, 168)).add_modifier(bold),
                    Warning => style.fg(Color::Rgb(241, 202, 129)).add_modifier(bold),
                    Blocker => style.fg(Color::Rgb(255, 167, 166)).add_modifier(bold),
                }
            }
        }
    }
}
