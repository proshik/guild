//! Shared horizontal geometry for the areas and their frame controls.

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub column: f32,
    pub left: f32,
    /// Whole work surface to the right of workspaces, including side docks.
    pub work_width: f32,
}

impl Geometry {
    /// Translate window coordinates into the containing work-surface row.
    pub fn relative_to(self, origin: f32) -> Self {
        Self {
            column: self.column - origin,
            left: self.left - origin,
            ..self
        }
    }

    pub fn center(&self) -> f32 {
        self.column + self.work_width / 2.0
    }

    /// History stays beside the workspace toggle, independent of the column.
    pub fn history_left(&self, controls_end: f32) -> f32 {
        controls_end
    }

    /// Content controls follow the workspace boundary, clamped after fixed controls.
    pub fn content_left(&self, controls_end: f32) -> f32 {
        self.column.max(controls_end)
    }

    /// The leftmost work card is the dock when open, otherwise the areas.
    pub fn branch_left(&self) -> f32 {
        self.column
    }

    /// Dock widths include their card borders, matching the existing areas row.
    pub fn new(window: f32, column: Option<f32>, left_dock: Option<f32>, gap: f32) -> Self {
        let column = column.unwrap_or(gap);
        let left = column + left_dock.map_or(0.0, |w| w + gap);
        Self {
            column,
            left,
            work_width: (window - column - gap).max(0.0),
        }
    }
}

impl crate::Root {
    pub(crate) fn panel_geometry(&self) -> Geometry {
        let panels = self.panels();
        let gap = self.rem_px(crate::theme::space::WORK_BETWEEN);
        Geometry::new(
            f32::from(self.viewport.width),
            (!self.settings.column_folded).then_some(self.settings.column_width),
            panels.left.open.then_some(panels.left.size + 2.0),
            gap,
        )
    }
}
