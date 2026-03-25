//! Tab and pane tree management.
//!
//! Each tab contains a binary tree of panes ([`PaneNode`]). Splitting a pane
//! creates a `Split` node with the original pane and a new sibling. Closing
//! a pane promotes its sibling to replace the parent split.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::pane::{Pane, PaneId};

/// Direction of a pane split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Side-by-side (left | right).
    Vertical,
    /// Top and bottom.
    Horizontal,
}

/// A rectangular region in pixel coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Width of the divider between panes in pixels.
pub const DIVIDER_WIDTH: f32 = 2.0;

/// Information about a divider for rendering and hit-testing.
#[derive(Debug, Clone, Copy)]
pub struct DividerInfo {
    pub rect: Rect,
    pub direction: SplitDirection,
    /// Path to the split node owning this divider.
    pub node_path: u64,
}

/// Binary tree node representing the pane layout.
pub enum PaneNode {
    /// A leaf node containing a single pane.
    Leaf(Box<Pane>),
    /// A split node dividing space between two children.
    Split {
        direction: SplitDirection,
        /// Position of the divider as a ratio (0.0..1.0).
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Collects all pane IDs in depth-first order.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_pane_ids(&mut ids);
        ids
    }

    fn collect_pane_ids(&self, ids: &mut Vec<PaneId>) {
        match self {
            PaneNode::Leaf(pane) => ids.push(pane.id),
            PaneNode::Split { first, second, .. } => {
                first.collect_pane_ids(ids);
                second.collect_pane_ids(ids);
            }
        }
    }

    /// Finds a mutable reference to the pane with the given ID.
    pub fn find_pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == id {
                    Some(pane)
                } else {
                    None
                }
            }
            PaneNode::Split { first, second, .. } => {
                first.find_pane_mut(id).or_else(|| second.find_pane_mut(id))
            }
        }
    }

    /// Finds an immutable reference to the pane with the given ID.
    pub fn find_pane(&self, id: PaneId) -> Option<&Pane> {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == id {
                    Some(pane)
                } else {
                    None
                }
            }
            PaneNode::Split { first, second, .. } => {
                first.find_pane(id).or_else(|| second.find_pane(id))
            }
        }
    }

    /// Computes the layout rectangles for each pane leaf.
    pub fn layout(&self, viewport: Rect) -> Vec<(PaneId, Rect)> {
        let mut result = Vec::new();
        self.layout_into(viewport, &mut result);
        result
    }

    fn layout_into(&self, viewport: Rect, result: &mut Vec<(PaneId, Rect)>) {
        match self {
            PaneNode::Leaf(pane) => {
                result.push((pane.id, viewport));
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = split_viewport(viewport, *direction, *ratio);
                first.layout_into(first_rect, result);
                second.layout_into(second_rect, result);
            }
        }
    }

    /// Collects divider info for rendering.
    pub fn dividers(&self, viewport: Rect) -> Vec<DividerInfo> {
        let mut result = Vec::new();
        self.dividers_into(viewport, &mut result, 0);
        result
    }

    fn dividers_into(&self, viewport: Rect, result: &mut Vec<DividerInfo>, path: u64) {
        if let PaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } = self
        {
            let divider_rect = compute_divider_rect(viewport, *direction, *ratio);
            result.push(DividerInfo {
                rect: divider_rect,
                direction: *direction,
                node_path: path,
            });

            let (first_rect, second_rect) = split_viewport(viewport, *direction, *ratio);
            first.dividers_into(first_rect, result, path * 2 + 1);
            second.dividers_into(second_rect, result, path * 2 + 2);
        }
    }

    /// Returns `true` if the tree contains a leaf with the given ID.
    fn contains(&self, target_id: PaneId) -> bool {
        match self {
            PaneNode::Leaf(pane) => pane.id == target_id,
            PaneNode::Split { first, second, .. } => {
                first.contains(target_id) || second.contains(target_id)
            }
        }
    }

    /// Splits the leaf node with the given pane ID, inserting a new pane.
    /// Returns `true` if the split was performed.
    pub fn split(&mut self, target_id: PaneId, direction: SplitDirection, new_pane: Pane) -> bool {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id != target_id {
                    return false;
                }
                // Swap self out with the new pane temporarily so we can
                // move the old leaf into the split's first child.
                let old_node = std::mem::replace(self, PaneNode::Leaf(Box::new(new_pane)));
                // Now self = new_pane leaf, old_node = original pane leaf.
                // Extract the new pane leaf and build the split.
                let new_pane_node = std::mem::replace(
                    self,
                    PaneNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(old_node),
                        // Temporary: will be replaced below.
                        second: Box::new(PaneNode::Leaf(Box::new(Pane {
                            id: PaneId(0),
                            terminal: std::sync::Arc::new(std::sync::Mutex::new(
                                minal_core::term::Terminal::new(1, 1),
                            )),
                            snapshot: Arc::new(ArcSwap::from_pointee(
                                minal_core::term::Terminal::new(1, 1).snapshot(),
                            )),
                            io_tx: crossbeam_channel::unbounded().0,
                            io_thread: None,
                            completion_engine: None,
                            context_collector: None,
                            ghost_text: None,
                            pending_context: None,
                            session_analyzer: None,
                            title: String::new(),
                        }))),
                    },
                );
                // Place the new pane in the second slot.
                if let PaneNode::Split { second, .. } = self {
                    **second = new_pane_node;
                }
                true
            }
            PaneNode::Split { first, second, .. } => {
                // Check which branch contains the target before recursing
                // to avoid moved-value issues.
                if first.contains(target_id) {
                    first.split(target_id, direction, new_pane)
                } else {
                    second.split(target_id, direction, new_pane)
                }
            }
        }
    }

    /// Removes the pane with the given ID. Returns the orphaned sibling node
    /// if the removal left a split with no children, or `None` if not found.
    pub fn remove_pane(&mut self, target_id: PaneId) -> RemoveResult {
        match self {
            PaneNode::Leaf(pane) => {
                if pane.id == target_id {
                    RemoveResult::RemoveSelf
                } else {
                    RemoveResult::NotFound
                }
            }
            PaneNode::Split { first, second, .. } => {
                // Check first child.
                match first.remove_pane(target_id) {
                    RemoveResult::RemoveSelf => {
                        // The first child is the target; promote the second.
                        let sibling = std::mem::replace(
                            second.as_mut(),
                            PaneNode::Leaf(Box::new(Pane {
                                id: PaneId(0),
                                terminal: std::sync::Arc::new(std::sync::Mutex::new(
                                    minal_core::term::Terminal::new(1, 1),
                                )),
                                snapshot: Arc::new(ArcSwap::from_pointee(
                                    minal_core::term::Terminal::new(1, 1).snapshot(),
                                )),
                                io_tx: crossbeam_channel::unbounded().0,
                                io_thread: None,
                                completion_engine: None,
                                context_collector: None,
                                ghost_text: None,
                                pending_context: None,
                                session_analyzer: None,
                                title: String::new(),
                            })),
                        );
                        *self = sibling;
                        return RemoveResult::Removed;
                    }
                    RemoveResult::Removed => return RemoveResult::Removed,
                    RemoveResult::NotFound => {}
                }

                // Check second child.
                match second.remove_pane(target_id) {
                    RemoveResult::RemoveSelf => {
                        // The second child is the target; promote the first.
                        let sibling = std::mem::replace(
                            first.as_mut(),
                            PaneNode::Leaf(Box::new(Pane {
                                id: PaneId(0),
                                terminal: std::sync::Arc::new(std::sync::Mutex::new(
                                    minal_core::term::Terminal::new(1, 1),
                                )),
                                snapshot: Arc::new(ArcSwap::from_pointee(
                                    minal_core::term::Terminal::new(1, 1).snapshot(),
                                )),
                                io_tx: crossbeam_channel::unbounded().0,
                                io_thread: None,
                                completion_engine: None,
                                context_collector: None,
                                ghost_text: None,
                                pending_context: None,
                                session_analyzer: None,
                                title: String::new(),
                            })),
                        );
                        *self = sibling;
                        RemoveResult::Removed
                    }
                    RemoveResult::Removed => RemoveResult::Removed,
                    RemoveResult::NotFound => RemoveResult::NotFound,
                }
            }
        }
    }

    /// Sets the divider ratio at the given path.
    pub fn set_divider_ratio_at_path(&mut self, path: u64, new_ratio: f32) {
        if let Some(PaneNode::Split { ratio, .. }) = self.find_split_at_path_mut(path) {
            *ratio = new_ratio.clamp(0.1, 0.9);
        }
    }

    fn find_split_at_path_mut(&mut self, target_path: u64) -> Option<&mut PaneNode> {
        if !matches!(self, PaneNode::Split { .. }) {
            return None;
        }
        if target_path == 0 {
            return Some(self);
        }
        match self {
            PaneNode::Split { first, second, .. } => {
                let child_path = if target_path % 2 == 1 {
                    (target_path - 1) / 2
                } else {
                    (target_path - 2) / 2
                };
                if target_path % 2 == 1 {
                    first.find_split_at_path_mut(child_path)
                } else {
                    second.find_split_at_path_mut(child_path)
                }
            }
            PaneNode::Leaf(_) => None,
        }
    }
}

/// Result of attempting to remove a pane from the tree.
pub enum RemoveResult {
    /// The pane was found and is this leaf; parent should promote sibling.
    RemoveSelf,
    /// The pane was removed and the tree was restructured.
    Removed,
    /// The target pane was not found in this subtree.
    NotFound,
}

/// Split a viewport into two rects with a divider gap.
fn split_viewport(viewport: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
    match direction {
        SplitDirection::Vertical => {
            let first_width = (viewport.width * ratio - DIVIDER_WIDTH / 2.0).max(0.0);
            let second_x = viewport.x + first_width + DIVIDER_WIDTH;
            let second_width = (viewport.width - first_width - DIVIDER_WIDTH).max(0.0);
            (
                Rect {
                    x: viewport.x,
                    y: viewport.y,
                    width: first_width,
                    height: viewport.height,
                },
                Rect {
                    x: second_x,
                    y: viewport.y,
                    width: second_width,
                    height: viewport.height,
                },
            )
        }
        SplitDirection::Horizontal => {
            let first_height = (viewport.height * ratio - DIVIDER_WIDTH / 2.0).max(0.0);
            let second_y = viewport.y + first_height + DIVIDER_WIDTH;
            let second_height = (viewport.height - first_height - DIVIDER_WIDTH).max(0.0);
            (
                Rect {
                    x: viewport.x,
                    y: viewport.y,
                    width: viewport.width,
                    height: first_height,
                },
                Rect {
                    x: viewport.x,
                    y: second_y,
                    width: viewport.width,
                    height: second_height,
                },
            )
        }
    }
}

/// Compute the divider rectangle for a split.
fn compute_divider_rect(viewport: Rect, direction: SplitDirection, ratio: f32) -> Rect {
    match direction {
        SplitDirection::Vertical => {
            let divider_x = viewport.x + viewport.width * ratio - DIVIDER_WIDTH / 2.0;
            Rect {
                x: divider_x,
                y: viewport.y,
                width: DIVIDER_WIDTH,
                height: viewport.height,
            }
        }
        SplitDirection::Horizontal => {
            let divider_y = viewport.y + viewport.height * ratio - DIVIDER_WIDTH / 2.0;
            Rect {
                x: viewport.x,
                y: divider_y,
                width: viewport.width,
                height: DIVIDER_WIDTH,
            }
        }
    }
}

/// Unique identifier for a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

/// A tab containing a tree of panes.
pub struct Tab {
    #[allow(dead_code)]
    pub id: TabId,
    pub root: PaneNode,
    pub focused_pane: PaneId,
    pub title: String,
}

impl Tab {
    /// Creates a new tab with a single pane.
    pub fn new(id: TabId, pane: Pane) -> Self {
        let focused = pane.id;
        let title = pane.title.clone();
        Self {
            id,
            root: PaneNode::Leaf(Box::new(pane)),
            focused_pane: focused,
            title,
        }
    }

    /// Returns a reference to the focused pane.
    pub fn focused_pane(&self) -> Option<&Pane> {
        self.root.find_pane(self.focused_pane)
    }

    /// Returns a mutable reference to the focused pane.
    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.root.find_pane_mut(self.focused_pane)
    }

    /// Split the focused pane in the given direction, creating a new pane.
    /// Returns `true` if the split succeeded.
    pub fn split_focused(&mut self, direction: SplitDirection, new_pane: Pane) -> bool {
        self.root.split(self.focused_pane, direction, new_pane)
    }

    /// Close the focused pane. Returns the number of remaining panes
    /// (0 means the tab should be closed).
    pub fn close_focused_pane(&mut self) -> usize {
        let result = self.root.remove_pane(self.focused_pane);
        match result {
            RemoveResult::RemoveSelf => {
                // Last pane in the tab.
                0
            }
            RemoveResult::Removed => {
                // Switch focus to the first remaining pane.
                let ids = self.root.pane_ids();
                if let Some(&first_id) = ids.first() {
                    self.focused_pane = first_id;
                }
                ids.len()
            }
            RemoveResult::NotFound => self.root.pane_ids().len(),
        }
    }

    /// Cycle focus to the next pane in depth-first order.
    pub fn focus_next_pane(&mut self) {
        let ids = self.root.pane_ids();
        if ids.len() <= 1 {
            return;
        }
        let current_idx = ids.iter().position(|id| *id == self.focused_pane);
        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % ids.len(),
            None => 0,
        };
        self.focused_pane = ids[next_idx];
    }

    /// Cycle focus to the previous pane in depth-first order.
    pub fn focus_prev_pane(&mut self) {
        let ids = self.root.pane_ids();
        if ids.len() <= 1 {
            return;
        }
        let current_idx = ids.iter().position(|id| *id == self.focused_pane);
        let prev_idx = match current_idx {
            Some(idx) => {
                if idx == 0 {
                    ids.len() - 1
                } else {
                    idx - 1
                }
            }
            None => 0,
        };
        self.focused_pane = ids[prev_idx];
    }

    /// Compute pane layout rectangles for the given viewport.
    pub fn layout(&self, viewport: Rect) -> Vec<(PaneId, Rect)> {
        self.root.layout(viewport)
    }

    /// Find which pane is at the given pixel coordinates.
    pub fn find_pane_at(&self, viewport: Rect, x: f32, y: f32) -> Option<PaneId> {
        let layouts = self.layout(viewport);
        for (pane_id, rect) in layouts {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(pane_id);
            }
        }
        None
    }

    /// Find which divider is at the given pixel coordinates (with hit margin).
    pub fn find_divider_at(&self, viewport: Rect, x: f32, y: f32) -> Option<DividerInfo> {
        let dividers = self.root.dividers(viewport);
        let hit_margin = 4.0;
        for div in dividers {
            let expanded = Rect {
                x: div.rect.x - hit_margin,
                y: div.rect.y - hit_margin,
                width: div.rect.width + hit_margin * 2.0,
                height: div.rect.height + hit_margin * 2.0,
            };
            if x >= expanded.x
                && x < expanded.x + expanded.width
                && y >= expanded.y
                && y < expanded.y + expanded.height
            {
                return Some(div);
            }
        }
        None
    }

    /// Collects dividers for rendering.
    pub fn dividers(&self, viewport: Rect) -> Vec<DividerInfo> {
        self.root.dividers(viewport)
    }

    /// Returns all pane IDs in this tab.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.root.pane_ids()
    }
}

/// Manages all tabs in the application.
pub struct TabManager {
    tabs: Vec<Tab>,
    active_tab: usize,
    next_pane_id: u64,
    next_tab_id: u64,
}

impl TabManager {
    /// Creates a new empty tab manager.
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_pane_id: 1,
            next_tab_id: 1,
        }
    }

    /// Allocates a new unique pane ID.
    pub fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    /// Adds a new tab with the given pane. Returns the tab index.
    pub fn add_tab(&mut self, pane: Pane) -> usize {
        let tab_id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        let tab = Tab::new(tab_id, pane);
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    /// Returns a reference to the active tab.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Returns a mutable reference to the active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Closes the tab at the given index, shutting down all its panes.
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            // Adjust active_tab if needed.
            if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                self.active_tab = self.tabs.len() - 1;
            }
        }
    }

    /// Closes the active tab.
    pub fn close_active_tab(&mut self) {
        self.close_tab(self.active_tab);
    }

    /// Switches to the tab at the given index (0-based).
    pub fn switch_to_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    /// Switches to the next tab (wrapping).
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Switches to the previous tab (wrapping).
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    /// Returns the number of tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Returns `true` if there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Returns tab info for rendering the tab bar.
    pub fn tab_render_info(&self) -> Vec<TabRenderInfo> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| TabRenderInfo {
                title: tab.title.clone(),
                is_active: i == self.active_tab,
            })
            .collect()
    }

    /// Find the pane with the given ID across all tabs. Returns (tab_index, &mut Pane).
    pub fn find_pane_mut(&mut self, pane_id: PaneId) -> Option<(usize, &mut Pane)> {
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(pane) = tab.root.find_pane_mut(pane_id) {
                return Some((tab_idx, pane));
            }
        }
        None
    }

    /// Remove a specific pane by ID. If it was the last pane in a tab,
    /// removes that tab. Returns `true` if the tab was also removed.
    pub fn remove_pane(&mut self, pane_id: PaneId) -> bool {
        for tab_idx in 0..self.tabs.len() {
            let tab = &mut self.tabs[tab_idx];
            let ids_before = tab.pane_ids();
            if !ids_before.contains(&pane_id) {
                continue;
            }

            // Set focus to this pane so close_focused_pane works.
            tab.focused_pane = pane_id;
            let remaining = tab.close_focused_pane();
            if remaining == 0 {
                self.close_tab(tab_idx);
                return true;
            }
            return false;
        }
        false
    }
}

/// Information needed to render a single tab in the tab bar.
#[derive(Debug, Clone)]
pub struct TabRenderInfo {
    pub title: String,
    pub is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dummy_pane(id: u64) -> Pane {
        let terminal = std::sync::Arc::new(std::sync::Mutex::new(minal_core::term::Terminal::new(
            24, 80,
        )));
        let snap = terminal
            .lock()
            .map(|t| t.snapshot())
            .unwrap_or_else(|_| minal_core::term::Terminal::new(24, 80).snapshot());
        let (io_tx, _io_rx) = crossbeam_channel::unbounded();
        Pane {
            id: PaneId(id),
            terminal,
            snapshot: Arc::new(ArcSwap::from_pointee(snap)),
            io_tx,
            io_thread: None,
            completion_engine: None,
            context_collector: None,
            ghost_text: None,
            pending_context: None,
            session_analyzer: None,
            title: format!("pane-{id}"),
        }
    }

    #[test]
    fn single_pane_layout() {
        let pane = make_dummy_pane(1);
        let tab = Tab::new(TabId(1), pane);
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let layouts = tab.layout(viewport);
        assert_eq!(layouts.len(), 1);
        assert_eq!(layouts[0].0, PaneId(1));
        assert!((layouts[0].1.width - 800.0).abs() < 0.01);
        assert!((layouts[0].1.height - 600.0).abs() < 0.01);
    }

    #[test]
    fn vertical_split_layout() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Vertical, pane2);

        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let layouts = tab.layout(viewport);
        assert_eq!(layouts.len(), 2);
        // First pane should be on the left, second on the right.
        assert!(layouts[0].1.width < 410.0); // ~400 - divider/2
        assert!(layouts[1].1.x > 390.0);
    }

    #[test]
    fn horizontal_split_layout() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Horizontal, pane2);

        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let layouts = tab.layout(viewport);
        assert_eq!(layouts.len(), 2);
        assert!(layouts[0].1.height < 310.0);
        assert!(layouts[1].1.y > 290.0);
    }

    #[test]
    fn focus_cycling() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let pane3 = make_dummy_pane(3);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Vertical, pane2);
        tab.focused_pane = PaneId(2);
        tab.split_focused(SplitDirection::Horizontal, pane3);

        tab.focused_pane = PaneId(1);

        tab.focus_next_pane();
        assert_eq!(tab.focused_pane, PaneId(2));

        tab.focus_next_pane();
        assert_eq!(tab.focused_pane, PaneId(3));

        tab.focus_next_pane();
        assert_eq!(tab.focused_pane, PaneId(1)); // wraps around

        tab.focus_prev_pane();
        assert_eq!(tab.focused_pane, PaneId(3)); // wraps back
    }

    #[test]
    fn close_pane_promotes_sibling() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Vertical, pane2);

        // Close pane 1 -> pane 2 should be promoted.
        tab.focused_pane = PaneId(1);
        let remaining = tab.close_focused_pane();
        assert_eq!(remaining, 1);
        assert_eq!(tab.focused_pane, PaneId(2));

        // Should be a single leaf now.
        assert_eq!(tab.pane_ids(), vec![PaneId(2)]);
    }

    #[test]
    fn close_last_pane_returns_zero() {
        let pane = make_dummy_pane(1);
        let mut tab = Tab::new(TabId(1), pane);
        let remaining = tab.close_focused_pane();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn find_pane_at() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Vertical, pane2);

        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };

        // Click on left side should hit pane 1.
        assert_eq!(tab.find_pane_at(viewport, 100.0, 300.0), Some(PaneId(1)));
        // Click on right side should hit pane 2.
        assert_eq!(tab.find_pane_at(viewport, 600.0, 300.0), Some(PaneId(2)));
    }

    #[test]
    fn tab_manager_basics() {
        let mut mgr = TabManager::new();
        assert!(mgr.is_empty());

        let pane1 = make_dummy_pane(1);
        mgr.add_tab(pane1);
        assert_eq!(mgr.tab_count(), 1);
        assert_eq!(mgr.active_tab, 0);

        let pane2 = make_dummy_pane(2);
        mgr.add_tab(pane2);
        assert_eq!(mgr.tab_count(), 2);

        mgr.switch_to_tab(1);
        assert_eq!(mgr.active_tab, 1);

        mgr.close_tab(0);
        assert_eq!(mgr.tab_count(), 1);
        assert_eq!(mgr.active_tab, 0);
    }

    #[test]
    fn dividers_single_pane_none() {
        let pane = make_dummy_pane(1);
        let tab = Tab::new(TabId(1), pane);
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let dividers = tab.dividers(viewport);
        assert!(dividers.is_empty());
    }

    #[test]
    fn dividers_after_split() {
        let pane1 = make_dummy_pane(1);
        let pane2 = make_dummy_pane(2);
        let mut tab = Tab::new(TabId(1), pane1);
        tab.split_focused(SplitDirection::Vertical, pane2);

        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let dividers = tab.dividers(viewport);
        assert_eq!(dividers.len(), 1);
        assert_eq!(dividers[0].direction, SplitDirection::Vertical);
    }
}
