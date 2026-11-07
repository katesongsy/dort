//! Presentation-only hierarchy over existing typed observations. No filesystem reads.
use super::app::App;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
#[derive(Clone)]
pub struct Node {
    pub key: usize,
    pub raw: Option<usize>,
    pub path: PathBuf,
    pub depth: usize,
    pub branch: bool,
}
pub fn nodes(app: &App) -> Arc<[Node]> {
    if app.discovering {
        return super::discovery_tree::snapshot(app).all.clone();
    }
    managed_nodes(app).into()
}
fn managed_nodes(app: &App) -> Vec<Node> {
    let mut paths = vec![];
    for raw in app.visible_file_rows() {
        let path = app.data.as_ref().and_then(|d| {
            d.managed
                .get(raw)
                .map(|r| r.target.clone())
                .or_else(|| d.files.get(raw).map(|r| PathBuf::from(&r.0)))
        });
        if let Some(path) = path {
            paths.push((path, raw));
        }
    }
    let base = app
        .data
        .as_ref()
        .and_then(|d| d.overview.as_ref())
        .map(|o| o.host.home.clone());
    build(paths, base, |_| false)
}
/// Pure metadata hierarchy construction, also used by the passive Discover worker.
pub(super) fn build(
    paths: Vec<(PathBuf, usize)>,
    mut base: Option<PathBuf>,
    directory: impl Fn(usize) -> bool,
) -> Vec<Node> {
    if base
        .as_ref()
        .is_none_or(|b| paths.iter().any(|(p, _)| !p.starts_with(b)))
    {
        base = paths
            .first()
            .and_then(|(p, _)| p.parent())
            .map(|p| p.to_owned());
        while base
            .as_ref()
            .is_some_and(|b| paths.iter().any(|(p, _)| !p.starts_with(b)))
        {
            base = base.and_then(|b| b.parent().map(|p| p.to_owned()));
        }
    }
    let base = base.unwrap_or_default();
    let mut map = BTreeMap::new();
    let mut branches = std::collections::BTreeSet::new();
    for (path, raw) in paths {
        let mut parent = path.parent();
        while let Some(p) = parent {
            if p == base || !p.starts_with(&base) || p.as_os_str().is_empty() {
                break;
            }
            branches.insert(p.to_owned());
            map.entry(p.to_owned()).or_insert(None);
            parent = p.parent();
        }
        map.insert(path, Some(raw));
    }
    let mut nodes = vec![];
    let mut group = 0;
    for (path, raw) in &map {
        let branch = branches.contains(path) || raw.is_some_and(&directory);
        let key = raw.unwrap_or_else(|| {
            group += 1;
            usize::MAX - group
        });
        let depth = path
            .strip_prefix(&base)
            .unwrap_or(path)
            .components()
            .count()
            .saturating_sub(1);
        nodes.push(Node {
            key,
            raw: *raw,
            path: path.clone(),
            depth,
            branch,
        });
    }
    nodes
}
pub fn visible(app: &App) -> super::discovery_tree::Rows {
    if app.discovering {
        return super::discovery_tree::rows(app);
    }
    let all = managed_nodes(app);
    let closed: std::collections::BTreeSet<_> = all
        .iter()
        .filter(|n| n.branch && app.inventory.managed_folded.contains(&n.path))
        .map(|n| n.path.as_path())
        .collect();
    let hidden: Vec<_> = all
        .iter()
        .map(|n| i32::from(n.path.ancestors().skip(1).any(|p| closed.contains(p))))
        .collect();
    let mask = super::visibility::Mask::new(&hidden);
    super::discovery_tree::Rows::new(all.into(), mask)
}
pub fn position(app: &App, key: usize) -> Option<usize> {
    if app.discovering {
        super::discovery_tree::position(app, key)
    } else {
        visible(app).iter().position(|n| n.key == key)
    }
}
pub fn marked(app: &App, node: &Node) -> bool {
    if node.branch {
        return false;
    }
    if app.discovering {
        app.inventory.candidate_marks.contains(&node.path)
    } else {
        node.raw
            .and_then(|r| app.data.as_ref()?.files.get(r))
            .is_some_and(|r| app.inventory.managed_marks.contains(&r.0))
    }
}
pub fn select(app: &mut App, key: usize) {
    if app.file_selection() != key {
        app.inventory.mark_refusal = None;
    }
    let visible = visible(app);
    if let Some(node) = position(app, key).and_then(|i| visible.get(i)) {
        app.inventory.group = if node.branch { Some(key) } else { None };
        if let Some(raw) = node.raw {
            if app.discovering {
                app.candidate_selected = raw;
            } else {
                app.selected = raw;
            }
        }
        // Synthetic groups retain an explicit cursor, not the previous file identity.
        if node.raw.is_none() {
            app.inventory.group = Some(key);
        }
    }
}
pub fn selected(app: &App) -> Option<Node> {
    let visible = visible(app);
    position(app, app.file_selection())
        .and_then(|i| visible.get(i))
        .cloned()
}
pub fn fold(app: &mut App, close: bool) {
    if let Some(node) = selected(app) {
        let path = if node.branch {
            Some(node.path)
        } else if close {
            node.path.parent().map(|p| p.to_owned())
        } else {
            None
        };
        if let Some(path) = path {
            let all = nodes(app);
            let group_key = all
                .binary_search_by(|n| n.path.cmp(&path))
                .ok()
                .map(|i| all[i].key);
            if app.discovering {
                super::discovery_tree::set_fold(app, path, close);
            } else {
                Arc::make_mut(&mut app.inventory.managed_folded).set(path, close);
            }
            if let Some(key) = group_key {
                app.inventory.group = Some(key);
                select(app, key);
            }
        }
    }
}
pub fn toggle(app: &mut App) {
    let Some(node) = selected(app) else {
        return;
    };
    let all_nodes = nodes(app);
    let scope = if node.branch {
        all_nodes.as_ref()
    } else {
        std::slice::from_ref(&node)
    };
    let matching = scope
        .iter()
        .filter(|n| {
            !n.branch && (n.path == node.path || (node.branch && n.path.starts_with(&node.path)))
        })
        .count();
    let eligible: Vec<_> = scope
        .iter()
        .filter(|n| {
            !n.branch && (n.path == node.path || (node.branch && n.path.starts_with(&node.path)))
        })
        .filter(|n| {
            !app.discovering
                || n.raw
                    .and_then(|r| app.discovery.as_ref()?.report.candidates.get(r))
                    .is_some_and(|c| c.supported_for_adoption)
        })
        .collect();
    if eligible.is_empty() {
        let reason = "Cannot mark: no eligible files. Directories/links, unowned files, hard links and special modes cannot be adopted.";
        app.inventory.mark_refusal = Some(reason.into());
        app.status = reason.into();
        return;
    }
    let skipped = matching.saturating_sub(eligible.len());
    app.inventory.mark_refusal = (skipped > 0).then(|| {
        format!(
            "{skipped} ineligible descendant(s) NOT marked; only eligible regular files selected."
        )
    });
    let all = eligible.iter().all(|n| marked(app, n));
    if app.discovering {
        app.inventory.candidate_mark_revision =
            app.inventory.candidate_mark_revision.wrapping_add(1);
    }
    for n in eligible {
        if app.discovering {
            if all {
                app.inventory.candidate_marks.remove(&n.path);
            } else {
                app.inventory.candidate_marks.insert(n.path.clone());
            }
        } else if let Some(id) = n
            .raw
            .and_then(|r| app.data.as_ref()?.files.get(r))
            .map(|r| r.0.clone())
        {
            if all {
                app.inventory.managed_marks.remove(&id);
            } else {
                app.inventory.managed_marks.insert(id);
            }
        }
    }
    app.status="Marks retained across filters/folds. Group marking includes matching descendants, even folded rows; adoption uses eligible regular files. E reviews discovery marks; A/K/D preview managed marks.".into();
}
