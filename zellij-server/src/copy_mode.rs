//! Native vim-like copy mode for Scroll/Search.
//!
//! Drives [`Selection`](crate::panes::selection::Selection) directly (half-open
//! ranges). A free cursor is a 1-cell selection (`end.col = start.col + 1`).

use crate::panes::grid::Grid;
use zellij_utils::position::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopySession {
    pub cursor: Position,
    pub visual_anchor: Option<Position>,
    pub linewise: bool,
}

impl CopySession {
    pub fn new_at_origin() -> Self {
        Self::new_at_position(Position::new(0, 0))
    }

    pub fn new_at_position(cursor: Position) -> Self {
        Self {
            cursor,
            visual_anchor: None,
            linewise: false,
        }
    }

    pub fn selection_start_end(&self) -> (Position, Position) {
        let anchor = self.visual_anchor.unwrap_or(self.cursor);
        if self.linewise {
            let start_line = anchor.line.0.min(self.cursor.line.0);
            let end_line = anchor.line.0.max(self.cursor.line.0);
            return (
                Position::new(start_line as i32, 0),
                Position::new(end_line as i32, u16::MAX),
            );
        }
        // Half-open range: include both the anchor cell and the cursor cell.
        if self.cursor >= anchor {
            (
                anchor,
                Position::new(
                    self.cursor.line.0 as i32,
                    (self.cursor.column.0 as u16).saturating_add(1),
                ),
            )
        } else {
            (
                self.cursor,
                Position::new(
                    anchor.line.0 as i32,
                    (anchor.column.0 as u16).saturating_add(1),
                ),
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyModeKey {
    Left,
    Right,
    Up,
    Down,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    LineStart,
    LineSelect,
    LineEnd,
    WordEnd,
    WordStart,
    WordBack,
    Yank,
    Esc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punct,
}

fn class_of(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Apply the current copy session as a native selection on the grid.
pub fn sync_selection(grid: &mut Grid, session: &CopySession) {
    let (start, end) = session.selection_start_end();
    grid.set_selection_range(&start, &end);
}

pub fn clear_selection(grid: &mut Grid) {
    grid.reset_selection();
}

/// Result of handling a key while copy mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyModeActiveResult {
    /// Stay in copy mode; selection already synced (or no visual change).
    Continue,
    /// Exit copy mode (caller clears selection). Optionally yank after.
    Exit {
        yank: bool,
    },
    /// Scroll the viewport one line, then re-sync (free-cursor edge only).
    ScrollUp,
    ScrollDown,
}

pub fn handle_active_key(
    session: &mut CopySession,
    grid: &mut Grid,
    key: CopyModeKey,
) -> CopyModeActiveResult {
    let rows = grid.height.max(1);
    let cols = grid.width.max(1);
    let max_row = (rows - 1) as isize;
    let max_col = cols.saturating_sub(1);

    match key {
        CopyModeKey::Left => {
            session.cursor.column.0 = session.cursor.column.0.saturating_sub(1);
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::Right => {
            if session.cursor.column.0 < max_col {
                session.cursor.column.0 += 1;
            }
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::Up => {
            if session.cursor.line.0 > 0 {
                session.cursor.line.0 -= 1;
                sync_selection(grid, session);
                CopyModeActiveResult::Continue
            } else if session.visual_anchor.is_none() {
                // Keep the one-cell cursor selection in the grid. The viewport
                // scroll moves it with the content, and resync updates the
                // session coordinates accordingly.
                CopyModeActiveResult::ScrollUp
            } else {
                sync_selection(grid, session);
                CopyModeActiveResult::Continue
            }
        },
        CopyModeKey::Down => {
            if session.cursor.line.0 < max_row {
                session.cursor.line.0 += 1;
                sync_selection(grid, session);
                CopyModeActiveResult::Continue
            } else if session.visual_anchor.is_none() {
                // Keep the one-cell cursor selection in the grid. The viewport
                // scroll moves it with the content, and resync updates the
                // session coordinates accordingly.
                CopyModeActiveResult::ScrollDown
            } else {
                sync_selection(grid, session);
                CopyModeActiveResult::Continue
            }
        },
        CopyModeKey::LineStart => {
            session.cursor.column.0 = 0;
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::LineSelect => {
            if session.visual_anchor.is_none() {
                session.visual_anchor = Some(session.cursor);
            }
            session.linewise = true;
            session.cursor.column.0 = 0;
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::LineEnd => {
            session.cursor.column.0 = max_col;
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::WordEnd | CopyModeKey::WordStart | CopyModeKey::WordBack => {
            let target = match key {
                CopyModeKey::WordEnd => next_word_end(grid, session.cursor),
                CopyModeKey::WordStart => next_word_start(grid, session.cursor),
                _ => prev_word_start(grid, session.cursor),
            };
            if let Some(target) = target {
                session.cursor = target;
            }
            sync_selection(grid, session);
            CopyModeActiveResult::Continue
        },
        CopyModeKey::Yank => CopyModeActiveResult::Exit { yank: true },
        CopyModeKey::Esc => {
            if session.visual_anchor.is_some() {
                session.visual_anchor = None;
                session.linewise = false;
                sync_selection(grid, session);
                CopyModeActiveResult::Continue
            } else {
                CopyModeActiveResult::Exit { yank: false }
            }
        },
        // Page / half-page keys are no-ops while copy mode is active.
        CopyModeKey::HalfPageUp
        | CopyModeKey::HalfPageDown
        | CopyModeKey::PageUp
        | CopyModeKey::PageDown => CopyModeActiveResult::Continue,
    }
}

pub fn toggle_visual(session: &mut CopySession, grid: &mut Grid) {
    session.visual_anchor = match session.visual_anchor {
        None => Some(session.cursor),
        Some(_) => None,
    };
    session.linewise = false;
    sync_selection(grid, session);
}

type ScanCell = (isize, usize, usize, CharClass);

fn viewport_cells(grid: &Grid) -> Vec<ScanCell> {
    let mut cells = Vec::new();
    for (row, line) in grid.viewport.iter().enumerate() {
        let mut col = 0usize;
        for terminal_character in &line.columns {
            let ch = terminal_character.character;
            let width = terminal_character.width();
            if width == 0 {
                continue;
            }
            cells.push((row as isize, col, width, class_of(ch)));
            col += width;
        }
        cells.push((row as isize, col, 1, CharClass::Space));
    }
    cells
}

fn cell_index_at(cells: &[ScanCell], cur: Position) -> usize {
    cells
        .iter()
        .position(|&(row, col, width, _)| {
            row > cur.line.0 || (row == cur.line.0 && col + width > cur.column.0)
        })
        .unwrap_or(cells.len())
}

fn covers(cells: &[ScanCell], i: usize, cur: Position) -> bool {
    i < cells.len() && cells[i].0 == cur.line.0 && cells[i].1 <= cur.column.0
}

fn clamp_cell(cell: ScanCell, rows: usize, cols: usize) -> Option<Position> {
    let (row, col, _, _) = cell;
    if row < 0 || row as usize >= rows {
        return None;
    }
    Some(Position::new(
        row as i32,
        (col.min(cols.saturating_sub(1))) as u16,
    ))
}

fn next_word_end(grid: &Grid, cur: Position) -> Option<Position> {
    let cells = viewport_cells(grid);
    let rows = grid.height;
    let cols = grid.width;
    let mut i = cell_index_at(&cells, cur);
    if covers(&cells, i, cur) {
        i += 1;
    }
    while cells.get(i)?.3 == CharClass::Space {
        i += 1;
    }
    let class = cells[i].3;
    while i + 1 < cells.len() && cells[i + 1].3 == class {
        i += 1;
    }
    clamp_cell(cells[i], rows, cols)
}

fn next_word_start(grid: &Grid, cur: Position) -> Option<Position> {
    let cells = viewport_cells(grid);
    let rows = grid.height;
    let cols = grid.width;
    let mut i = cell_index_at(&cells, cur);
    if covers(&cells, i, cur) && cells[i].3 != CharClass::Space {
        let class = cells[i].3;
        while cells.get(i)?.3 == class {
            i += 1;
        }
    }
    while cells.get(i)?.3 == CharClass::Space {
        i += 1;
    }
    clamp_cell(cells[i], rows, cols)
}

fn prev_word_start(grid: &Grid, cur: Position) -> Option<Position> {
    let cells = viewport_cells(grid);
    let rows = grid.height;
    let cols = grid.width;
    let mut i = cell_index_at(&cells, cur);
    if i == 0 {
        return None;
    }
    i -= 1;
    while cells[i].3 == CharClass::Space {
        if i == 0 {
            return None;
        }
        i -= 1;
    }
    let class = cells[i].3;
    while i > 0 && cells[i - 1].3 == class {
        i -= 1;
    }
    clamp_cell(cells[i], rows, cols)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScrollPosition {
    // Kept for per-pane bookkeeping / future use. Bottom-exit now uses the
    // pane's real `is_scrolled` flag rather than an armed counter.
}

pub fn is_scroll_group(mode: zellij_utils::data::InputMode) -> bool {
    matches!(
        mode,
        zellij_utils::data::InputMode::Scroll
            | zellij_utils::data::InputMode::Search
            | zellij_utils::data::InputMode::EnterSearch
    )
}

pub fn is_steady(mode: zellij_utils::data::InputMode) -> bool {
    matches!(
        mode,
        zellij_utils::data::InputMode::Normal
            | zellij_utils::data::InputMode::Scroll
            | zellij_utils::data::InputMode::Search
            | zellij_utils::data::InputMode::EnterSearch
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::kitty_graphics::KittyImageStore;
    use crate::panes::link_handler::LinkHandler;
    use crate::panes::sixel::SixelImageStore;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use zellij_utils::data::{Palette, Style};

    fn grid_with_text(rows: usize, cols: usize, text: &str) -> Grid {
        let mut grid = Grid::new(
            rows,
            cols,
            Rc::new(RefCell::new(Palette::default())),
            Rc::new(RefCell::new(HashMap::new())),
            Rc::new(RefCell::new(LinkHandler::new())),
            Rc::new(RefCell::new(None)),
            Rc::new(RefCell::new(SixelImageStore::default())),
            Rc::new(RefCell::new(KittyImageStore::default())),
            Style::default(),
            false,
            true,
            true,
            true,
            false,
        );
        let mut vte_parser = vte::Parser::new();
        vte_parser.advance(&mut grid, text.as_bytes());
        grid
    }

    #[test]
    fn free_cursor_selection_is_one_cell() {
        let session = CopySession::new_at_origin();
        let (start, end) = session.selection_start_end();
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 1));
    }

    #[test]
    fn scroll_position_default() {
        let _pos = ScrollPosition::default();
    }

    #[test]
    fn word_motions_and_selection_sync() {
        let mut grid = grid_with_text(5, 40, "hello world foo");
        let mut session = CopySession::new_at_origin();
        sync_selection(&mut grid, &session);
        assert_eq!(grid.get_selected_text().as_deref(), Some("h"));

        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::WordEnd),
            CopyModeActiveResult::Continue
        );
        assert_eq!(session.cursor, Position::new(0, 4)); // end of "hello"
        assert_eq!(grid.get_selected_text().as_deref(), Some("o"));

        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::WordStart),
            CopyModeActiveResult::Continue
        );
        assert_eq!(session.cursor, Position::new(0, 6)); // start of "world"
        assert_eq!(grid.get_selected_text().as_deref(), Some("w"));

        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::WordBack),
            CopyModeActiveResult::Continue
        );
        assert_eq!(session.cursor, Position::new(0, 0));
    }

    #[test]
    fn yank_range_covers_visual_selection() {
        let mut grid = grid_with_text(5, 40, "abcdef");
        let mut session = CopySession::new_at_origin();
        sync_selection(&mut grid, &session);
        toggle_visual(&mut session, &mut grid);
        session.cursor = Position::new(0, 3);
        sync_selection(&mut grid, &session);
        // half-open: cols 0..4 → "abcd"
        assert_eq!(grid.get_selected_text().as_deref(), Some("abcd"));
        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::Yank),
            CopyModeActiveResult::Exit { yank: true }
        );
    }

    #[test]
    fn linewise_visual_selection_covers_complete_lines() {
        let mut grid = grid_with_text(5, 40, "first\r\nsecond\r\nthird");
        let mut session = CopySession::new_at_origin();
        session.visual_anchor = Some(Position::new(0, 0));
        session.cursor = Position::new(1, 3);
        session.linewise = true;
        sync_selection(&mut grid, &session);
        assert_eq!(grid.get_selected_text().as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn esc_exits_visual_then_copy_mode() {
        let mut grid = grid_with_text(3, 20, "abc");
        let mut session = CopySession::new_at_origin();
        sync_selection(&mut grid, &session);
        toggle_visual(&mut session, &mut grid);
        assert!(session.visual_anchor.is_some());
        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::Esc),
            CopyModeActiveResult::Continue
        );
        assert!(session.visual_anchor.is_none());
        assert_eq!(
            handle_active_key(&mut session, &mut grid, CopyModeKey::Esc),
            CopyModeActiveResult::Exit { yank: false }
        );
    }
}
