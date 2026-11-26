//! Hit testing the editor gutter measured from its public text layout.

/// Whether a press `x_from_left` pixels into the editor lands on the
/// numbers: the input's padding before them, and their own width.
pub fn inside(x_from_left: f32, padding: f32, width: f32) -> bool {
    x_from_left >= 0.0 && x_from_left < padding + width
}
