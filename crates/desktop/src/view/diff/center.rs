//! One source-block geometry for code, number paint and curved connections.
use super::*;
use crate::diff::document::DisplayRow;
use gpui::{Pixels, Point, point};

#[derive(Clone, Copy, Debug)]
pub(super) struct Ribbon {
    pub old_end: Pixels,
    pub new_end: Pixels,
    pub control_old: Point<Pixels>,
    pub control_new: Point<Pixels>,
}
pub(super) fn ribbon(old: usize, new: usize, height: Pixels, width: Pixels) -> Ribbon {
    let old_end = height * old as f32;
    let new_end = height * new as f32;
    Ribbon {
        old_end,
        new_end,
        control_old: point(width / 2., old_end),
        control_new: point(width / 2., new_end),
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct RowPaint {
    pub fill: u32,
    pub edge: u32,
    pub top: bool,
    pub bottom: bool,
}
impl DiffTab {
    pub(super) fn center_row_paint(&self, index: usize, side: usize) -> RowPaint {
        let theme = self.look().theme;
        let colors = theme::diff_center::of(theme);
        let plain = RowPaint {
            fill: theme.surface.app,
            edge: theme.surface.divider,
            top: false,
            bottom: false,
        };
        let Some(Line::Source {
            file,
            row:
                DisplayRow::Text {
                    old,
                    new,
                    block: Some(block),
                },
        }) = self.lines().get(index)
        else {
            return plain;
        };
        let Some(block) = self
            .documents
            .get(&self.files()[*file].path)
            .and_then(|doc| doc.blocks.get(*block))
        else {
            return plain;
        };
        let row = old
            .map(|line| line - block.old.start)
            .or_else(|| new.map(|line| line - block.new.start))
            .unwrap_or(0);
        // Unified keeps its existing single-source removed/added row semantics.
        let unified = self.mode() == guild_session::DiffView::Unified;
        let side = if unified {
            usize::from(new.is_some())
        } else {
            side
        };
        let len = if side == 0 {
            block.old.len()
        } else {
            block.new.len()
        };
        let present = if side == 0 {
            old.is_some()
        } else {
            new.is_some()
        };
        let edge = if unified {
            if side == 0 {
                theme.diff.removed
            } else {
                theme.diff.added
            }
        } else if block.old.is_empty() {
            colors.added
        } else if block.new.is_empty() {
            colors.removed
        } else {
            colors.modified
        };
        RowPaint {
            fill: if present { edge } else { colors.empty },
            edge,
            top: row == 0,
            bottom: len > 0 && row + 1 == len,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unequal_ribbon_preserves_actual_extents_and_horizontal_tangents() {
        let r = ribbon(2, 5, gpui::px(26.), gpui::px(24.));
        assert_eq!((r.old_end, r.new_end), (gpui::px(52.), gpui::px(130.)));
        assert_eq!(
            r.control_old.y, r.old_end,
            "old edge must leave horizontally, not as a diagonal wedge"
        );
        assert_eq!(
            r.control_new.y, r.new_end,
            "new edge must arrive horizontally"
        );
        for (old, new) in [(0, 2), (3, 0), (3, 3)] {
            let r = ribbon(old, new, gpui::px(26.), gpui::px(24.));
            assert_eq!(r.old_end, gpui::px(26.) * old as f32);
            assert_eq!(r.new_end, gpui::px(26.) * new as f32);
        }
    }
}
