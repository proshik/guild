//! The terminal content rectangle, shared by sizing, paint and pointer input.
use super::emulator::GridSize;
use crate::theme;
use gpui::{Bounds, Pixels, point, px, size};

pub(crate) fn content(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let x = px(theme::terminal::PADDING_X);
    let y = px(theme::terminal::PADDING_Y);
    Bounds::new(
        bounds.origin + point(x, y),
        size(
            (bounds.size.width - x * 2.).max(px(0.)),
            (bounds.size.height - y * 2.).max(px(0.)),
        ),
    )
}

pub(crate) fn grid(bounds: Bounds<Pixels>, cell: gpui::Size<Pixels>) -> GridSize {
    let area = content(bounds);
    GridSize::new(
        (f32::from(area.size.width) / f32::from(cell.width)).floor() as usize,
        (f32::from(area.size.height) / f32::from(cell.height)).floor() as usize,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_vertical_insets_return_one_real_row_without_narrowing_columns() {
        for height in [200., 399., 600., 777.] {
            let bounds = Bounds::new(point(px(40.), px(70.)), size(px(826.), px(height)));
            let cell = size(px(8.), px(16.));
            let area = content(bounds);
            assert_eq!(
                grid(bounds, cell),
                GridSize::new(100, ((height - 26.) / 16.).floor() as usize + 1)
            );
            assert_eq!(area.origin, point(px(53.), px(75.)));
            assert_eq!(area.size.width, px(800.));
            assert!(
                area.origin.y + cell.height * grid(bounds, cell).rows as f32
                    <= bounds.bottom() - px(5.)
            );
        }
    }
}
