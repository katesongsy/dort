//! Presentation-only hierarchy over existing typed observations. No filesystem reads.
use super::app::App;
use std::{collections::BTreeMap, path::PathBuf};
pub struct Node {
    pub key: usize,
    pub raw: Option<usize>,
    pub path: PathBuf,
    pub depth: usize,
    pub branch: bool,
}
pub fn nodes(app: &App) -> Vec<Node> {
    let mut paths = vec![];
    for raw in app.visible_file_rows() {
        let path = if app.discovering {
            app.discovery
                .as_ref()
                .and_then(|d| d.report.candidates.get(raw))
                .map(|c| c.entry.path.clone())
        } else {
            app.data.as_ref().and_then(|d| {
                d.managed
                    .get(raw)
                    .map(|r| r.target.clone())
                    .or_else(|| d.files.get(raw).map(|r| PathBuf::from(&r.0)))
            })
        };
        if let Some(path) = path {
            paths.push((path, raw));
        }
    }
    let mut base = app
        .data
        .as_ref()
        .and_then(|d| d.overview.as_ref())
        .map(|o| o.host.home.clone());
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
        let branch = branches.contains(path)
            || (app.discovering
                && raw
                    .and_then(|r| app.discovery.as_ref()?.report.candidates.get(r))
                    .is_some_and(|c| c.entry.kind == dort_core::observation::Kind::Directory));
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
pub fn visible(app: &App) -> Vec<Node> {
    let folded = if app.discovering {
        &app.inventory.candidate_folded
    } else {
        &app.inventory.managed_folded
    };
    nodes(app)
        .into_iter()
        .filter(|n| !folded.iter().any(|p| n.path != *p && n.path.starts_with(p)))
        .collect()
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
    if let Some(node) = visible(app).into_iter().find(|n| n.key == key) {
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
pub fn fold(app: &mut App, close: bool) {
    if let Some(node) = visible(app)
        .into_iter()
        .find(|n| n.key == app.file_selection())
    {
        let path = if node.branch {
            Some(node.path)
        } else if close {
            node.path.parent().map(|p| p.to_owned())
        } else {
            None
        };
        if let Some(path) = path {
            let folded = if app.discovering {
                &mut app.inventory.candidate_folded
            } else {
                &mut app.inventory.managed_folded
            };
            if close {
                folded.insert(path.clone());
            } else {
                folded.remove(&path);
            }
            if let Some(key) = visible(app).iter().find(|n| n.path == path).map(|n| n.key) {
                select(app, key);
            }
        }
    }
}
pub fn toggle(app: &mut App) {
    let Some(node) = visible(app)
        .into_iter()
        .find(|n| n.key == app.file_selection())
    else {
        return;
    };
    let eligible: Vec<_> = nodes(app)
        .into_iter()
        .filter(|n| {
            !n.branch && (n.path == node.path || (node.branch && n.path.starts_with(&node.path)))
        })
        .filter(|n| {
            !app.discovering
                || n.raw
                    .and_then(|r| app.discovery.as_ref()?.report.candidates.get(r))
                    .is_some_and(|c| c.entry.kind == dort_core::observation::Kind::Regular)
        })
        .collect();
    let all = eligible.iter().all(|n| marked(app, n));
    for n in eligible {
        if app.discovering {
            if all {
                app.inventory.candidate_marks.remove(&n.path);
            } else {
                app.inventory.candidate_marks.insert(n.path);
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
    app.status="Marks retained across filters/folds. Group marking includes matching descendants, even folded rows; discovery directories/links are not adoption items. E reviews discovery marks; A/K/D preview managed marks.".into();
}
