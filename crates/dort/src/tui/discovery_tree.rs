//! Immutable metadata-only Discover views. Folding only updates a local range index.
use super::{
    actions::Task,
    app::{App, Tab},
    tree::{self, Node},
    visibility::{Folds, Mask},
};
use crate::services::Discovery;
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, rc::Rc, sync::Arc};

const INLINE_LIMIT: usize = 512;
#[derive(Default)]
pub struct Snapshot {
    pub matching: Arc<[usize]>,
    pub all: Arc<[Node]>,
    positions: BTreeMap<usize, usize>,
    paths: BTreeMap<PathBuf, usize>,
    ends: Vec<usize>,
}
impl Snapshot {
    fn mask(&self, folds: &Folds) -> Mask {
        let mut changes = vec![0; self.all.len() + 1];
        for (i, node) in self.all.iter().enumerate() {
            if node.branch && folds.contains(&node.path) {
                changes[i + 1] += 1;
                changes[self.ends[i]] -= 1;
            }
        }
        let mut hidden = 0;
        let counts: Vec<_> = changes
            .into_iter()
            .take(self.all.len())
            .map(|delta| {
                hidden += delta;
                hidden
            })
            .collect();
        Mask::new(&counts)
    }
}
#[derive(Clone)]
pub struct Rows {
    pub all: Arc<[Node]>,
    mask: Rc<RefCell<Mask>>,
}
impl Rows {
    pub fn new(all: Arc<[Node]>, mask: Mask) -> Self {
        Self {
            all,
            mask: Rc::new(RefCell::new(mask)),
        }
    }
    pub fn len(&self) -> usize {
        self.mask.borrow().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> Option<&Node> {
        self.all.get(self.mask.borrow().nth(i)?)
    }
    pub fn first(&self) -> Option<&Node> {
        self.get(0)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.window(0, self.len())
    }
    pub fn window(&self, start: usize, len: usize) -> impl Iterator<Item = &Node> {
        (start..start.saturating_add(len).min(self.len())).filter_map(|i| self.get(i))
    }
}
#[derive(Default)]
pub struct State {
    source: Option<Arc<Discovery>>,
    query: String,
    folded: Arc<Folds>,
    base: Option<PathBuf>,
    generation: u64,
    requested: bool,
    ready: bool,
    snapshot: Arc<Snapshot>,
    mask: Rc<RefCell<Mask>>,
    shown_marks: Option<(u64, usize)>,
}
pub struct Read {
    source: Arc<Discovery>,
    query: String,
    folded: Arc<Folds>,
    base: Option<PathBuf>,
    generation: u64,
}
pub struct Report {
    generation: u64,
    snapshot: Arc<Snapshot>,
    folded: Arc<Folds>,
    mask: Mask,
}
impl Read {
    pub fn perform(self) -> Report {
        let query = self.query.to_lowercase();
        let candidates = &self.source.report.candidates;
        let matching: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                query.is_empty()
                    || c.entry
                        .path
                        .to_str()
                        .expect("validated candidate")
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        let paths = matching
            .iter()
            .map(|&i| (candidates[i].entry.path.clone(), i))
            .collect();
        let all: Arc<[Node]> = tree::build(paths, self.base, |i| {
            candidates[i].entry.kind == dort_core::observation::Kind::Directory
        })
        .into();
        let mut ends = vec![all.len(); all.len()];
        let mut stack: Vec<usize> = vec![];
        for (i, node) in all.iter().enumerate() {
            while stack.last().is_some_and(|&j| all[j].depth >= node.depth) {
                ends[stack.pop().expect("stack")] = i;
            }
            stack.push(i);
        }
        let snapshot = Arc::new(Snapshot {
            matching: matching.into(),
            positions: all.iter().enumerate().map(|(i, n)| (n.key, i)).collect(),
            paths: all
                .iter()
                .enumerate()
                .map(|(i, n)| (n.path.clone(), i))
                .collect(),
            all,
            ends,
        });
        let mask = snapshot.mask(&self.folded);
        Report {
            generation: self.generation,
            snapshot,
            folded: self.folded,
            mask,
        }
    }
}
impl State {
    fn read(&self) -> Option<Read> {
        Some(Read {
            source: self.source.clone()?,
            query: self.query.clone(),
            folded: self.folded.clone(),
            base: self.base.clone(),
            generation: self.generation,
        })
    }
    fn ensure(&mut self, app: &App) {
        let base = app
            .data
            .as_ref()
            .and_then(|d| d.overview.as_ref())
            .map(|o| &o.host.home);
        let same_source = match (&self.source, &app.discovery) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same_source && self.query == app.inventory.candidates.query && self.base.as_ref() == base
        {
            if !Arc::ptr_eq(&self.folded, &app.inventory.candidate_folded) {
                self.folded = app.inventory.candidate_folded.clone();
                *self.mask.borrow_mut() = self.snapshot.mask(&self.folded);
                self.shown_marks = None;
            }
            return;
        }
        self.source = app.discovery.clone();
        self.query.clone_from(&app.inventory.candidates.query);
        self.folded.clone_from(&app.inventory.candidate_folded);
        self.base = base.cloned();
        self.generation = self.generation.wrapping_add(1);
        self.requested = false;
        self.ready = self.source.is_none();
        // Query/source replacement must not expose stale actionable rows.
        self.snapshot = Arc::default();
        self.mask = Rc::default();
        self.shown_marks = None;
        if self
            .source
            .as_ref()
            .is_some_and(|s| s.report.candidates.len() <= INLINE_LIMIT)
        {
            self.accept(self.read().expect("source").perform());
        }
    }
    fn accept(&mut self, report: Report) -> bool {
        if report.generation != self.generation {
            return false;
        }
        let mask = if Arc::ptr_eq(&report.folded, &self.folded) {
            report.mask
        } else {
            report.snapshot.mask(&self.folded)
        };
        self.snapshot = report.snapshot;
        self.mask = Rc::new(RefCell::new(mask));
        self.shown_marks = None;
        self.ready = true;
        true
    }
}
pub fn snapshot(app: &App) -> Arc<Snapshot> {
    let mut state = app.inventory.tree.borrow_mut();
    state.ensure(app);
    state.snapshot.clone()
}
pub fn rows(app: &App) -> Rows {
    snapshot(app);
    let state = app.inventory.tree.borrow();
    Rows {
        all: state.snapshot.all.clone(),
        mask: state.mask.clone(),
    }
}
pub fn position(app: &App, key: usize) -> Option<usize> {
    snapshot(app);
    let state = app.inventory.tree.borrow();
    state
        .mask
        .borrow()
        .position(*state.snapshot.positions.get(&key)?)
}
/// Individual folds never replace/filter the hierarchy or schedule a worker.
pub fn set_fold(app: &mut App, path: PathBuf, closed: bool) {
    snapshot(app);
    let old = app.inventory.candidate_folded.contains(&path);
    let mut state = app.inventory.tree.borrow_mut();
    // Release the cache's reference before editing: ordinary folds do not copy
    // the exceptions set. A pending read alone may retain its immutable version.
    state.folded = Arc::default();
    Arc::make_mut(&mut app.inventory.candidate_folded).set(path.clone(), closed);
    if old != closed
        && let Some(&i) = state.snapshot.paths.get(&path)
    {
        let end = state.snapshot.ends[i];
        state
            .mask
            .borrow_mut()
            .add(i + 1, end, if closed { 1 } else { -1 });
    }
    state.folded = app.inventory.candidate_folded.clone();
    state.shown_marks = None;
}
pub fn pending(app: &App) -> bool {
    snapshot(app);
    let state = app.inventory.tree.borrow();
    state.source.is_some() && !state.ready
}
pub fn shown_marks(app: &App) -> usize {
    snapshot(app);
    let mut state = app.inventory.tree.borrow_mut();
    let revision = app.inventory.candidate_mark_revision;
    if let Some((previous, count)) = state.shown_marks
        && previous == revision
    {
        return count;
    }
    let count = app
        .inventory
        .candidate_marks
        .iter()
        .filter(|p| {
            state
                .snapshot
                .paths
                .get(*p)
                .is_some_and(|&i| state.mask.borrow().position(i).is_some())
        })
        .count();
    state.shown_marks = Some((revision, count));
    count
}
pub fn contains(app: &App, raw: usize) -> bool {
    snapshot(app).matching.binary_search(&raw).is_ok()
}
pub fn next(app: &App) -> Option<Task> {
    if !app.discovering || app.tab != Tab::Files || app.busy || app.quit || app.exit_requested {
        return None;
    }
    let mut state = app.inventory.tree.borrow_mut();
    state.ensure(app);
    if state.ready || state.requested {
        return None;
    }
    let read = state.read()?;
    state.requested = true;
    Some(Task::DiscoveryTree(read))
}
pub fn accept(app: &App, report: Report) -> bool {
    let mut state = app.inventory.tree.borrow_mut();
    state.ensure(app);
    state.accept(report)
}
