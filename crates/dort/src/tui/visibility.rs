//! Presentation-only range index: collapsed ancestors hide contiguous subtrees.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Folds {
    closed: bool,
    exceptions: BTreeSet<PathBuf>,
}
impl Default for Folds {
    fn default() -> Self {
        Self {
            closed: true,
            exceptions: BTreeSet::new(),
        }
    }
}
impl Folds {
    pub fn contains(&self, path: &Path) -> bool {
        self.closed != self.exceptions.contains(path)
    }
    pub fn set(&mut self, path: PathBuf, closed: bool) {
        if closed != self.closed {
            self.exceptions.insert(path);
        } else {
            self.exceptions.remove(&path);
        }
    }
    pub fn all(closed: bool) -> Self {
        Self {
            closed,
            exceptions: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Segment {
    min: i32,
    count: usize,
    lazy: i32,
}
#[derive(Default)]
pub struct Mask {
    len: usize,
    tree: Vec<Segment>,
}
impl Mask {
    pub fn new(hidden: &[i32]) -> Self {
        let mut mask = Self {
            len: hidden.len(),
            tree: vec![Segment::default(); hidden.len() * 4],
        };
        if !hidden.is_empty() {
            mask.build(1, 0, hidden.len(), hidden);
        }
        mask
    }
    fn build(&mut self, p: usize, l: usize, r: usize, hidden: &[i32]) {
        if r - l == 1 {
            self.tree[p] = Segment {
                min: hidden[l],
                count: 1,
                lazy: 0,
            };
            return;
        }
        let m = (l + r) / 2;
        self.build(p * 2, l, m, hidden);
        self.build(p * 2 + 1, m, r, hidden);
        self.pull(p);
    }
    fn pull(&mut self, p: usize) {
        let a = self.tree[p * 2];
        let b = self.tree[p * 2 + 1];
        self.tree[p].min = a.min.min(b.min) + self.tree[p].lazy;
        self.tree[p].count =
            if a.min <= b.min { a.count } else { 0 } + if b.min <= a.min { b.count } else { 0 };
    }
    pub fn add(&mut self, start: usize, end: usize, delta: i32) {
        if start < end {
            self.update(1, 0, self.len, start, end, delta);
        }
    }
    fn update(&mut self, p: usize, l: usize, r: usize, start: usize, end: usize, delta: i32) {
        if end <= l || r <= start {
            return;
        }
        if start <= l && r <= end {
            self.tree[p].min += delta;
            self.tree[p].lazy += delta;
            return;
        }
        let m = (l + r) / 2;
        self.update(p * 2, l, m, start, end, delta);
        self.update(p * 2 + 1, m, r, start, end, delta);
        self.pull(p);
    }
    fn zeros(&self, p: usize, inherited: i32) -> usize {
        if self.tree[p].min + inherited == 0 {
            self.tree[p].count
        } else {
            0
        }
    }
    pub fn len(&self) -> usize {
        if self.len == 0 { 0 } else { self.zeros(1, 0) }
    }
    pub fn nth(&self, mut n: usize) -> Option<usize> {
        if n >= self.len() {
            return None;
        }
        let (mut p, mut l, mut r, mut inherited) = (1, 0, self.len, 0);
        while r - l > 1 {
            inherited += self.tree[p].lazy;
            let m = (l + r) / 2;
            let left = self.zeros(p * 2, inherited);
            if n < left {
                p *= 2;
                r = m;
            } else {
                n -= left;
                p = p * 2 + 1;
                l = m;
            }
        }
        Some(l)
    }
    pub fn position(&self, index: usize) -> Option<usize> {
        if index >= self.len {
            return None;
        }
        let (mut p, mut l, mut r, mut inherited, mut before) = (1, 0, self.len, 0, 0);
        while r - l > 1 {
            inherited += self.tree[p].lazy;
            let m = (l + r) / 2;
            if index < m {
                p *= 2;
                r = m;
            } else {
                before += self.zeros(p * 2, inherited);
                p = p * 2 + 1;
                l = m;
            }
        }
        (self.tree[p].min + inherited == 0).then_some(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_ranges_preserve_closed_descendants_and_rank() {
        let mut mask = Mask::new(&[0; 7]);
        mask.add(2, 5, 1);
        mask.add(1, 6, 1);
        assert_eq!((mask.len(), mask.nth(1)), (2, Some(6)));
        mask.add(1, 6, -1);
        assert_eq!(
            (0..mask.len())
                .map(|i| mask.nth(i).unwrap())
                .collect::<Vec<_>>(),
            [0, 1, 5, 6]
        );
        assert_eq!(mask.position(5), Some(2));
        assert_eq!(mask.position(3), None);
        mask.add(2, 5, -1);
        for i in 0..7 {
            assert_eq!(mask.nth(i), Some(i));
            assert_eq!(mask.position(i), Some(i));
        }
    }
    #[test]
    fn defaults_apply_to_new_folders() {
        let mut folds = Folds::default();
        assert!(folds.contains(Path::new("new")));
        folds.set("a".into(), false);
        assert!(!folds.contains(Path::new("a")));
        assert!(folds.contains(Path::new("a/child")));
        folds.set("a".into(), true);
        assert!(folds.contains(Path::new("a")));
    }
}
