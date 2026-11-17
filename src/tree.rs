use std::{cmp::Reverse, ops::Range};

use crate::Span;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Dependency {
    pub(crate) span: Span,
    pub(crate) owner: usize,
}

#[derive(Debug)]
struct Node {
    center: u64,
    crossing: Range<usize>,
    left: Option<usize>,
    right: Option<usize>,
}

#[derive(Debug)]
pub(crate) struct Tree {
    dependencies: Vec<Dependency>,
    nodes: Vec<Node>,
    by_start: Vec<usize>,
    by_end: Vec<usize>,
    root: Option<usize>,
    extent: Option<Span>,
}

impl Tree {
    pub(crate) fn build(dependencies: Vec<Dependency>) -> Self {
        let n = dependencies.len();
        let mut ids: Vec<_> = (0..n).collect();
        ids.sort_unstable_by_key(|&id| (dependencies[id].span.start(), id));
        let extent = ids.first().map(|&first| {
            let start = dependencies[first].span.start();
            let end = dependencies.iter().map(|d| d.span.end()).max().unwrap();
            Span::new(start, end).expect("nonempty dependency extent")
        });
        let mut tree = Self {
            dependencies,
            nodes: Vec::new(),
            by_start: Vec::with_capacity(n),
            by_end: Vec::with_capacity(n),
            root: None,
            extent,
        };
        tree.root = tree.build_node(ids);
        tree
    }

    // `ids` is sorted by (start, original ID); partitioning preserves that order.
    fn build_node(&mut self, ids: Vec<usize>) -> Option<usize> {
        if ids.is_empty() {
            return None;
        }
        let center = self.dependencies[ids[ids.len() / 2]].span.start();
        let (mut left, mut right, mut crossing) = (Vec::new(), Vec::new(), Vec::new());
        for id in ids {
            let span = self.dependencies[id].span;
            if span.end() <= center {
                left.push(id);
            } else if span.start() > center {
                right.push(id);
            } else {
                crossing.push(id);
            }
        }
        let start = self.by_start.len();
        self.by_start.extend_from_slice(&crossing);
        crossing.sort_unstable_by_key(|&id| (Reverse(self.dependencies[id].span.end()), id));
        self.by_end.extend_from_slice(&crossing);
        let node = self.nodes.len();
        self.nodes.push(Node {
            center,
            crossing: start..self.by_start.len(),
            left: None,
            right: None,
        });
        let left = self.build_node(left);
        let right = self.build_node(right);
        self.nodes[node].left = left;
        self.nodes[node].right = right;
        Some(node)
    }

    pub(crate) fn len(&self) -> usize {
        self.dependencies.len()
    }

    pub(crate) fn visit(&self, change: Span, report: &mut impl FnMut(Dependency)) {
        // A measured dense-query bottleneck: when the change covers every
        // dependency, traversal and ordered-ID indirection cannot prune work.
        // Scan original records contiguously, without per-record overlap tests.
        if self
            .extent
            .is_some_and(|extent| change.start() <= extent.start() && change.end() >= extent.end())
        {
            for &dependency in &self.dependencies {
                report(dependency);
            }
            return;
        }
        if let Some(root) = self.root {
            self.visit_node(root, change, report);
        }
    }

    fn visit_node(&self, id: usize, change: Span, report: &mut impl FnMut(Dependency)) {
        let node = &self.nodes[id];
        if change.end() <= node.center {
            for &id in &self.by_start[node.crossing.clone()] {
                let dep = self.dependencies[id];
                if dep.span.start() >= change.end() {
                    break;
                }
                report(dep);
            }
            if let Some(left) = node.left {
                self.visit_node(left, change, report);
            }
        } else if change.start() > node.center {
            for &id in &self.by_end[node.crossing.clone()] {
                let dep = self.dependencies[id];
                if dep.span.end() <= change.start() {
                    break;
                }
                report(dep);
            }
            if let Some(right) = node.right {
                self.visit_node(right, change, report);
            }
        } else {
            for &id in &self.by_start[node.crossing.clone()] {
                report(self.dependencies[id]);
            }
            if let Some(left) = node.left {
                self.visit_node(left, change, report);
            }
            if let Some(right) = node.right {
                self.visit_node(right, change, report);
            }
        }
    }
}
