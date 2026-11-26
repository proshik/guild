//! Pure window-local overlay geometry. Independent of program archives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Compact,
    Expanded,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Geometry {
    pub fn clamped(self, viewport: [f32; 2]) -> Self {
        let width = self.width.max(0.).min(viewport[0].max(0.));
        let height = self.height.max(0.).min(viewport[1].max(0.));
        Self {
            x: self.x.max(0.).min((viewport[0] - width).max(0.)),
            y: self.y.max(0.).min((viewport[1] - height).max(0.)),
            width,
            height,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
#[derive(Debug, Clone, Copy)]
pub enum Gesture {
    Move {
        pointer: [f32; 2],
        start: Geometry,
    },
    Resize {
        edge: ResizeEdge,
        pointer: [f32; 2],
        start: Geometry,
    },
}
#[derive(Debug, Clone)]
pub struct Presentation {
    pub mode: Mode,
    pub compact: Geometry,
    pub expanded: Geometry,
    pub gesture: Option<Gesture>,
    pub minimum: [[f32; 2]; 2],
}
impl Presentation {
    pub fn new(compact: Geometry, expanded: Geometry, minimum: [[f32; 2]; 2]) -> Self {
        Self {
            mode: Mode::Compact,
            compact,
            expanded,
            gesture: None,
            minimum,
        }
    }
    /// Initialize a terminal card once; later gestures remain authoritative.
    pub fn terminal_geometry(
        &mut self,
        requested: Option<&guild_session::terminal_job::Geometry>,
        viewport: [f32; 2],
    ) {
        self.mode = Mode::Compact;
        if let Some(requested) = requested {
            let width = viewport[0] * f32::from(requested.width_percent) / 100.;
            let height = viewport[1] * f32::from(requested.height_percent) / 100.;
            self.compact = Geometry {
                x: (viewport[0] - width) / 2.,
                y: (viewport[1] - height) / 2.,
                width,
                height,
            }
            .clamped(viewport);
        }
    }

    pub fn geometry(&self, viewport: [f32; 2]) -> Geometry {
        match self.mode {
            Mode::Compact => self.compact,
            Mode::Expanded => self.expanded,
        }
        .clamped(viewport)
    }
    pub fn remember(&mut self, geometry: Geometry) {
        match self.mode {
            Mode::Compact => self.compact = geometry,
            Mode::Expanded => self.expanded = geometry,
        }
    }
    /// End a pointer gesture, including a release outside the card.
    pub fn release(&mut self, pointer: [f32; 2], viewport: [f32; 2]) {
        self.motion(pointer, true, viewport);
        self.gesture = None;
    }
    pub fn motion(&mut self, pointer: [f32; 2], pressed: bool, viewport: [f32; 2]) {
        if !pressed {
            self.gesture = None;
            return;
        }
        let Some(gesture) = self.gesture else {
            return;
        };
        let mut geometry = match gesture {
            Gesture::Move { start, .. } | Gesture::Resize { start, .. } => start.clamped(viewport),
        };
        match gesture {
            Gesture::Move {
                pointer: origin, ..
            } => {
                geometry.x += pointer[0] - origin[0];
                geometry.y += pointer[1] - origin[1];
            }
            Gesture::Resize {
                edge,
                pointer: origin,
                ..
            } => {
                let min = self.minimum[usize::from(self.mode == Mode::Expanded)];
                let dx = pointer[0] - origin[0];
                let dy = pointer[1] - origin[1];
                if matches!(
                    edge,
                    ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
                ) {
                    let right = geometry.x + geometry.width;
                    geometry.x = (geometry.x + dx).max(0.).min((right - min[0]).max(0.));
                    geometry.width = right - geometry.x;
                } else if matches!(
                    edge,
                    ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
                ) {
                    geometry.width = (geometry.width + dx)
                        .max(min[0])
                        .min((viewport[0] - geometry.x).max(0.));
                }
                if matches!(
                    edge,
                    ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
                ) {
                    let bottom = geometry.y + geometry.height;
                    geometry.y = (geometry.y + dy).max(0.).min((bottom - min[1]).max(0.));
                    geometry.height = bottom - geometry.y;
                } else if matches!(
                    edge,
                    ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
                ) {
                    geometry.height = (geometry.height + dy)
                        .max(min[1])
                        .min((viewport[1] - geometry.y).max(0.));
                }
            }
        }
        self.remember(geometry.clamped(viewport));
    }
}
