//! Selectable, single-line rows. Selection is a position; click targets own row identity.
use std::ops::Range;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Row, Table, TableState},
};

use super::{glyphs, heading, muted, pane, selected};
use crate::tui::{
    input::{Hits, Target, Wheel},
    navigation::ListViewport,
};

pub enum Header<const N: usize> {
    None,
    Heading([&'static str; N]),
    Quiet([&'static str; N]),
}

pub struct Collection<'a, const N: usize> {
    widths: [Constraint; N],
    header: Header<N>,
    title: Option<(&'a str, bool)>,
    marker: bool,
}

#[derive(Default)]
pub struct Selection {
    pub selected: Option<usize>,
    pub offset: usize,
}
impl Selection {
    pub fn at(selected: Option<usize>) -> Self {
        Self {
            selected,
            ..Self::default()
        }
    }
}

pub struct Window {
    pub range: Range<usize>,
    pub selection: Selection,
}

pub struct RowTargets<'a, F> {
    pub hits: &'a Hits,
    pub wheel: Option<Wheel>,
    pub target: F,
}

pub struct Rendered {
    body: Rect,
    ids: Vec<usize>,
    pub offset: usize,
}
impl Rendered {
    pub fn rows(&self) -> impl Iterator<Item = (Rect, usize)> + '_ {
        self.ids
            .iter()
            .copied()
            .skip(self.offset)
            .take(usize::from(self.body.height))
            .enumerate()
            .map(|(line, id)| {
                (
                    Rect::new(self.body.x, self.body.y + line as u16, self.body.width, 1),
                    id,
                )
            })
    }
}

impl<'a, const N: usize> Collection<'a, N> {
    pub fn new(widths: [Constraint; N], header: Header<N>) -> Self {
        Self {
            widths,
            header,
            title: None,
            marker: true,
        }
    }

    pub fn titled(mut self, title: &'a str, focused: bool) -> Self {
        self.title = Some((title, focused));
        self
    }

    pub fn highlight_only(mut self) -> Self {
        self.marker = false;
        self
    }

    fn row_area(&self, area: Rect) -> Rect {
        let inset = (u16::from(self.title.is_some())
            + u16::from(!matches!(self.header, Header::None)))
        .min(area.height);
        Rect::new(area.x, area.y + inset, area.width, area.height - inset)
    }

    pub fn capacity(&self, area: Rect) -> usize {
        usize::from(self.row_area(area).height)
    }

    pub fn window(
        &self,
        area: Rect,
        viewport: &ListViewport,
        len: usize,
        selected: Option<usize>,
    ) -> Window {
        let capacity = self.capacity(area);
        let first = viewport.start(len, capacity, selected);
        let range = first..first.saturating_add(capacity).min(len);
        let selection = Selection::at(selected.filter(|i| range.contains(i)).map(|i| i - first));
        Window { range, selection }
    }

    pub fn draw<'row>(
        self,
        frame: &mut Frame,
        area: Rect,
        rows: impl IntoIterator<Item = (usize, Row<'row>)>,
        selection: Selection,
        targets: RowTargets<'_, impl Fn(usize) -> Target>,
    ) -> Rendered {
        let (ids, rows): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
        let body = self.row_area(area);
        let mut table = Table::new(rows, self.widths).row_highlight_style(selected());
        match self.header {
            Header::None => (),
            Header::Heading(labels) => table = table.header(Row::new(labels).style(heading())),
            Header::Quiet(labels) => table = table.header(Row::new(labels).style(muted())),
        }
        if let Some((title, focused)) = self.title {
            table = table.block(pane(title, focused));
        }
        if self.marker {
            table = table.highlight_symbol(format!("{} ", glyphs::SELECTED));
        }
        let mut state = TableState::default()
            .with_selected(selection.selected)
            .with_offset(selection.offset);
        frame.render_stateful_widget(table, area, &mut state);
        let rendered = Rendered {
            body,
            ids,
            offset: state.offset(),
        };
        if let Some(wheel) = targets.wheel {
            targets.hits.wheel(area, wheel);
        }
        for (row, id) in rendered.rows() {
            targets.hits.click(row, (targets.target)(id));
        }
        rendered
    }
}
