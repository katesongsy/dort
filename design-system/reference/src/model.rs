use ratatui::crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordId(usize);

pub(crate) struct Record {
    pub title: &'static str,
    pub tool: &'static str,
    pub target: &'static str,
    pub snapshot: &'static str,
    pub date: &'static str,
    pub status: Status,
    pub origin: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) enum Status {
    Ready,
    Blocked {
        reason: &'static str,
        next: &'static str,
    },
    Finished,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Blocked { .. } => "Blocked",
            Self::Finished => "Finished",
        }
    }
}

pub(crate) enum ReviewDecision {
    Ready(RestorePlan),
    Blocked,
    Finished,
}

pub(crate) struct RestorePlan {
    id: RecordId,
    pub snapshot: &'static str,
    pub target: &'static str,
}

impl RestorePlan {
    pub(crate) fn record<'a>(&self, app: &'a App) -> &'a Record {
        app.record(self.id)
    }

    pub(crate) fn backup_path(&self) -> String {
        format!("{}.dort-prev", self.target)
    }
}

pub(crate) struct Receipt {
    id: RecordId,
}

impl Receipt {
    pub(crate) fn record<'a>(&self, app: &'a App) -> &'a Record {
        app.record(self.id)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Region {
    Queue,
    Details,
}

pub(crate) enum Screen {
    Browse,
    Details,
    Filter(String),
    Review { plan: RestorePlan, from: Region },
    Receipt { receipt: Receipt, from: Region },
}

pub(crate) struct App {
    records: Vec<Record>,
    selected: Option<RecordId>,
    filter: String,
    screen: Screen,
    origin: bool,
    details_scroll: usize,
    modal_scroll: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct ViewContext {
    pub split: bool,
    pub scroll_limit: usize,
}

impl App {
    pub(crate) fn new(records: Vec<Record>) -> Self {
        Self {
            selected: (!records.is_empty()).then_some(RecordId(0)),
            records,
            filter: String::new(),
            screen: Screen::Browse,
            origin: false,
            details_scroll: 0,
            modal_scroll: 0,
        }
    }

    pub(crate) fn screen(&self) -> &Screen {
        &self.screen
    }

    pub(crate) fn record(&self, id: RecordId) -> &Record {
        &self.records[id.0]
    }

    pub(crate) fn record_count(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn show_origin(&self) -> bool {
        self.origin
    }

    pub(crate) fn details_scroll(&self) -> usize {
        self.details_scroll
    }

    pub(crate) fn modal_scroll(&self) -> usize {
        self.modal_scroll
    }

    pub(crate) fn filter(&self) -> &str {
        match &self.screen {
            Screen::Filter(draft) => draft,
            _ => &self.filter,
        }
    }

    pub(crate) fn matches(&self) -> Vec<RecordId> {
        let query = self.filter().trim().to_lowercase();
        self.records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                query.is_empty()
                    || [record.title, record.tool, record.target]
                        .iter()
                        .any(|value| value.to_lowercase().contains(&query))
            })
            .map(|(index, _)| RecordId(index))
            .collect()
    }

    pub(crate) fn visible_selection(&self, matches: &[RecordId]) -> Option<RecordId> {
        self.selected
            .filter(|id| matches.contains(id))
            .or_else(|| matches.first().copied())
    }

    pub(crate) fn decision(&self, id: RecordId) -> ReviewDecision {
        let record = self.record(id);
        match record.status {
            Status::Ready => ReviewDecision::Ready(RestorePlan {
                id,
                snapshot: record.snapshot,
                target: record.target,
            }),
            Status::Blocked { .. } => ReviewDecision::Blocked,
            Status::Finished => ReviewDecision::Finished,
        }
    }

    pub(crate) fn key(&mut self, key: KeyCode, view: ViewContext) {
        match &mut self.screen {
            Screen::Browse => match key {
                KeyCode::Char('/') => self.screen = Screen::Filter(self.filter.clone()),
                KeyCode::Up | KeyCode::Down => self.move_selection(key),
                KeyCode::Tab if view.split && self.selected.is_some() => {
                    self.screen = Screen::Details
                }
                KeyCode::Enter if self.selected.is_some() && !view.split => {
                    self.screen = Screen::Details
                }
                KeyCode::Enter if view.split => self.open_review(Region::Queue),
                _ => {}
            },
            Screen::Details => match key {
                KeyCode::Esc => self.screen = Screen::Browse,
                KeyCode::Tab if view.split => self.screen = Screen::Browse,
                KeyCode::Char(' ') => self.origin = !self.origin,
                KeyCode::Enter => self.open_review(Region::Details),
                _ => scroll(&mut self.details_scroll, key, view.scroll_limit),
            },
            Screen::Filter(draft) => match key {
                KeyCode::Esc => self.screen = Screen::Browse,
                KeyCode::Enter => {
                    let selection = self.visible_selection(&self.matches());
                    let Screen::Filter(draft) = std::mem::replace(&mut self.screen, Screen::Browse)
                    else {
                        unreachable!()
                    };
                    self.filter = draft;
                    self.select(selection);
                }
                KeyCode::Backspace => {
                    if let Some((index, _)) = draft.grapheme_indices(true).next_back() {
                        draft.truncate(index);
                    }
                }
                KeyCode::Char(c) if !c.is_control() => draft.push(c),
                _ => {}
            },
            Screen::Review { plan, from } => match key {
                KeyCode::Esc => self.screen = return_to(*from),
                KeyCode::Char('y') if self.modal_scroll >= view.scroll_limit => {
                    let id = plan.id;
                    let from = *from;
                    self.records[id.0].status = Status::Finished;
                    self.modal_scroll = 0;
                    self.screen = Screen::Receipt {
                        receipt: Receipt { id },
                        from,
                    };
                }
                _ => scroll(&mut self.modal_scroll, key, view.scroll_limit),
            },
            Screen::Receipt { from, .. } => match key {
                KeyCode::Esc => self.screen = return_to(*from),
                _ => scroll(&mut self.modal_scroll, key, view.scroll_limit),
            },
        }
    }

    fn open_review(&mut self, from: Region) {
        if let Some(id) = self.selected
            && let ReviewDecision::Ready(plan) = self.decision(id)
        {
            self.modal_scroll = 0;
            self.screen = Screen::Review { plan, from };
        }
    }

    fn move_selection(&mut self, key: KeyCode) {
        let matches = self.matches();
        if let Some(index) = matches.iter().position(|id| Some(*id) == self.selected) {
            let index = if key == KeyCode::Up {
                index.saturating_sub(1)
            } else {
                (index + 1).min(matches.len() - 1)
            };
            self.select(Some(matches[index]));
        }
    }

    fn select(&mut self, selected: Option<RecordId>) {
        if selected != self.selected {
            self.details_scroll = 0;
            self.selected = selected;
        }
    }
}

fn scroll(offset: &mut usize, key: KeyCode, limit: usize) {
    *offset = (*offset).min(limit);
    match key {
        KeyCode::Up => *offset = offset.saturating_sub(1),
        KeyCode::Down => *offset = (*offset + 1).min(limit),
        _ => {}
    }
}

fn return_to(region: Region) -> Screen {
    match region {
        Region::Queue => Screen::Browse,
        Region::Details => Screen::Details,
    }
}

pub(crate) fn demo() -> App {
    App::new(vec![
        Record {
            title: "Git configuration",
            tool: "Git",
            target: "~/.gitconfig",
            snapshot: "R-041",
            date: "12 Jun 2026",
            status: Status::Ready,
            origin: "Local recovery journal · sha256:9b4e2da70375",
        },
        Record {
            title: "Git includes",
            tool: "Git",
            target: "~/.gitconfig.local",
            snapshot: "R-019",
            date: "08 Jun 2026",
            status: Status::Blocked {
                reason: "Snapshot R-019 has no checksum.",
                next: "Capture a verified snapshot.",
            },
            origin: "Local recovery journal · checksum missing",
        },
        Record {
            title: "Shell startup",
            tool: "Zsh",
            target: "~/.zshrc",
            snapshot: "R-008",
            date: "03 Jun 2026",
            status: Status::Finished,
            origin: "Local recovery journal · sha256:f9c5a901be4d",
        },
        Record {
            title: "Editor settings",
            tool: "Neovim",
            target: "~/.config/チーム/atelier-e\u{0301}/workspaces/shared/profiles/neovim/recovery/confirmed/settings.lua",
            snapshot: "R-043",
            date: "13 Jun 2026",
            status: Status::Ready,
            origin: "Local recovery journal · sha256:0b51c4f880b4203f318d4ae6273f52e0",
        },
    ])
}
