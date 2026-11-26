//! One footer line and the room a native main window must retain.

use crate::{column, scale, theme};
use gpui::{Bounds, Pixels, Size, point, px, size};
use guild_session::{Panel, WindowPlace};

/// Physical width floor leaves more room for the start screen between side docks.
const MIN_CONTENT_WIDTH: f32 = 900.0;

/// Physical height floor keeps Commit and start-screen content usable.
const MIN_CONTENT_HEIGHT: f32 = 500.0;

/// Physical content dimensions; panel model minimums do not scale with UI text.
pub fn minimum(ui_font: f32, tools_intrinsic: f32, right: f32) -> Size<Pixels> {
    let rem = scale::rem_size(ui_font);
    let token = |value: gpui::Rems| value.0 * f32::from(rem);
    let button = token(theme::space::STATUS_BUTTON);
    let inset = token(theme::space::WORK_INSET);
    let left = 2.0 * button
        + 3.0 * token(theme::space::HEAD_ICON)
        + 4.0 * inset
        + 4.0 * token(theme::space::STATUS_BUTTON_GAP)
        + token(theme::space::FOOTER_LABEL_GAP)
        + 2.0 * token(theme::space::FOOTER_SOURCE_PADDING)
        + 2.0 * button;
    let tools = (guild_session::Tool::ALL.len() - 1) as f32 * button;
    let footer = tools.max(tools_intrinsic) + 2.0 * left.max(right + inset) + inset;
    let docks = 2.0 * Panel::MIN_WIDTH
        + Panel::MIN_WORK
        + 2.0 * token(theme::space::WORK_BETWEEN)
        + 4.0
        + inset;
    size(
        px((column::narrowest(ui_font) + footer.max(docks))
            .ceil()
            .max(MIN_CONTENT_WIDTH)),
        px((Panel::MIN_HEIGHT
            + Panel::MIN_WORK
            + 2.0
            + token(
                theme::space::TITLEBAR
                    + theme::space::STATUS_BAR
                    + theme::space::WORK_GAP
                    + theme::space::WORK_GAP,
            ))
        .ceil()
        .max(MIN_CONTENT_HEIGHT)),
    )
}

/// Give up inter-icon air before the single row's minimum is reached.
pub fn icon_gap(ui_font: f32, work_width: f32, tools_intrinsic: f32, right: f32) -> f32 {
    let work_min =
        f32::from(minimum(ui_font, tools_intrinsic, right).width) - column::narrowest(ui_font);
    let slots = (guild_session::Tool::ALL.len() - 2 + 2 * 4) as f32;
    ((work_width - work_min) / slots)
        .max(0.0)
        .min(theme::space::STATUS_BUTTON_GAP.0 * f32::from(scale::rem_size(ui_font)))
}

/// The room the status bar leaves a language server's line: from the
/// centred tools' right edge to the file's facts and the right dock's
/// tools, less the bar's gap on either side of the line.
///
/// **The server is not one of the minimum's inputs.** It is the one part
/// of the bar that comes and goes on its own — a start, an index, a
/// refusal in the server's own words — and a minimum that counted it
/// grew the window past the screen and took the growth from the column.
/// It takes what is left instead, and gives way: [`server_parts`].
pub fn server_room(ui_font: f32, bar: f32, tools_right: f32, stable_right: f32) -> f32 {
    let rem = f32::from(scale::rem_size(ui_font));
    (right_room(ui_font, bar, tools_right)
        - 2.0 * theme::space::PANEL_PADDING.0 * rem
        - stable_right)
        .max(0.0)
}

/// The room the whole right side of the status bar has: from the centred
/// tools' right edge to the bar's inset. The server's line and the stable
/// side share it in the layout itself, so a stable side that grows before
/// it is measured squeezes the line rather than pushing it over the tools.
///
/// **No gap is taken here.** The minimum keeps `right + inset` either side
/// of the tools and no more, so at the minimum this room is the stable
/// side's and a little over: a gap taken here would be wider than that
/// little and cut into the stable side, which is never cut.
pub fn right_room(ui_font: f32, bar: f32, tools_right: f32) -> f32 {
    let rem = f32::from(scale::rem_size(ui_font));
    (bar - theme::space::WORK_INSET.0 * rem - tools_right).max(0.0)
}

/// The widths a server's line is built from, each with the gap before it;
/// zero for a part the line does not have.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ServerLine {
    /// The padding around the line and the spinner: always drawn.
    pub frame: f32,
    /// What the server is doing — *Indexing*, *cargo check*.
    pub title: f32,
    /// The dot and the least of the detail worth drawing before an ellipsis.
    pub detail: f32,
    /// The dot and the whole detail, up to its cut.
    pub detail_full: f32,
    /// How many other tasks run, `+2`.
    pub more: f32,
    /// The progress bar.
    pub bar: f32,
    /// `34%`: always drawn when the server says it.
    pub percent: f32,
}

/// Which parts of a server's line are drawn in the room it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerParts {
    pub title: bool,
    pub detail: bool,
    pub more: bool,
    pub bar: bool,
    /// Something is cut or left out: the tooltip carries the whole line.
    pub cut: bool,
}

/// What of a server's line fits: **the detail is cut first**, down to an
/// ellipsis and then away with the task count beside it, **then the bar
/// goes, then the title**. The spinner and the percentage stay, and the
/// rest is in the tooltip and the servers' list. A line with no
/// percentage keeps its title, cut, rather than a spinner saying nothing.
pub fn server_parts(room: f32, line: ServerLine) -> ServerParts {
    let width = |p: &ServerParts| {
        line.frame
            + line.percent
            + if p.title { line.title } else { 0.0 }
            + if p.detail { line.detail } else { 0.0 }
            + if p.more { line.more } else { 0.0 }
            + if p.bar { line.bar } else { 0.0 }
    };
    let whole = ServerParts {
        title: true,
        detail: line.detail > 0.0,
        more: line.more > 0.0,
        bar: line.bar > 0.0,
        cut: false,
    };
    let without_detail = ServerParts {
        detail: false,
        more: false,
        ..whole
    };
    let without_bar = ServerParts {
        bar: false,
        ..without_detail
    };
    let without_title = ServerParts {
        title: line.percent <= 0.0,
        ..without_bar
    };
    let mut parts = [whole, without_detail, without_bar, without_title]
        .into_iter()
        .find(|p| width(p) <= room)
        .unwrap_or(without_title);
    parts.cut = parts != whole
        || width(&whole) - line.detail + line.detail_full > room
        || width(&parts) > room;
    parts
}

/// Retain saved position where possible; fit dimensions and origin to the display.
pub fn restored_bounds(
    saved: Option<WindowPlace>,
    display: Bounds<Pixels>,
    ui_font: f32,
) -> Bounds<Pixels> {
    let least = minimum(ui_font, 0.0, 0.0);
    let valid = saved.filter(|p| {
        [p.x, p.y, p.width, p.height].iter().all(|n| n.is_finite())
            && p.width > 0.0
            && p.height > 0.0
    });
    let requested = valid.map_or(size(px(1180.0), px(760.0)), |p| {
        size(px(p.width), px(p.height))
    });
    let fitted = size(
        requested
            .width
            .max(least.width)
            .min(display.size.width.max(least.width)),
        requested
            .height
            .max(least.height)
            .min(display.size.height.max(least.height)),
    );
    let origin = valid.map_or_else(
        || display.center() - point(fitted.width / 2.0, fitted.height / 2.0),
        |p| point(px(p.x), px(p.y)),
    );
    Bounds {
        origin: point(
            origin
                .x
                .max(display.left())
                .min((display.right() - fitted.width).max(display.left())),
            origin
                .y
                .max(display.top())
                .min((display.bottom() - fitted.height).max(display.top())),
        ),
        size: fitted,
    }
}

/// Fit two saved side docks together, keeping their minimums and the work visible.
pub(crate) fn fit_sides(left: f32, right: f32, available: f32) -> (f32, f32) {
    let finite = |n: f32| {
        if n.is_finite() {
            n.max(Panel::MIN_WIDTH)
        } else {
            Panel::MIN_WIDTH
        }
    };
    let (left, right) = (finite(left), finite(right));
    let spare = (available - 2.0 * Panel::MIN_WIDTH).max(0.0);
    let excess = left + right - 2.0 * Panel::MIN_WIDTH;
    if excess <= spare {
        return (left, right);
    }
    let factor = if excess > 0.0 { spare / excess } else { 0.0 };
    (
        Panel::MIN_WIDTH + (left - Panel::MIN_WIDTH) * factor,
        Panel::MIN_WIDTH + (right - Panel::MIN_WIDTH) * factor,
    )
}

impl crate::Root {
    fn frame_minimum(&self) -> Size<Pixels> {
        minimum(
            self.settings.ui_font_size,
            f32::from(self.status_tools_width.get()) * self.settings.ui_font_size
                / scale::BASE_UI_FONT_SIZE,
            f32::from(self.status_right_width.get()) * self.settings.ui_font_size
                / scale::BASE_UI_FONT_SIZE,
        )
    }

    pub(crate) fn enforce_frame_minimum(
        &self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let least = self.frame_minimum();
        crate::system::set_minimum_content_size(window, least);
        let actual = window.viewport_size();
        if !window.is_fullscreen() && (actual.width < least.width || actual.height < least.height) {
            window.resize(size(
                actual.width.max(least.width),
                actual.height.max(least.height),
            ));
            // TestWindow has no native resize callback. Production receives it from AppKit.
            #[cfg(test)]
            window.bounds_changed(cx);
            #[cfg(not(test))]
            let _ = cx;
        }
    }

    pub(crate) fn fit_frame_panels(&mut self) {
        let before = (self.settings.column_width, self.settings.panels.clone());
        // A width saved before the floor rose, or at a smaller interface
        // size, is raised to the floor here, on the first frame.
        let floor = column::narrowest(self.settings.ui_font_size);
        let room = (f32::from(self.viewport.width)
            - (f32::from(self.frame_minimum().width) - floor))
            .max(floor);
        let column = if self.settings.column_width.is_finite() {
            column::width(self.settings.column_width, self.settings.ui_font_size).min(room)
        } else {
            floor
        };
        self.settings.column_width = column;
        let taken = if self.settings.column_folded {
            self.rem_px(theme::space::WORK_INSET)
        } else {
            column
        };
        let available = f32::from(self.viewport.width)
            - taken
            - self
                .rem_px(theme::space::WORK_INSET + theme::space::WORK_GAP + theme::space::WORK_GAP)
            - 4.0
            - Panel::MIN_WORK;
        let bottom_max =
            (f32::from(self.viewport.height) - self.bottom_frame_height() - Panel::MIN_WORK)
                .max(Panel::MIN_HEIGHT);
        let mut panels = self.panels();
        if panels.left.open && panels.right.open {
            let (left, right) = fit_sides(panels.left.size, panels.right.size, available);
            panels.left.size = left;
            panels.right.size = right;
        } else {
            // Closed docks retain an independent size until they share the work area.
            let fit = |value: f32| {
                if value.is_finite() {
                    value.clamp(Panel::MIN_WIDTH, available.max(Panel::MIN_WIDTH))
                } else {
                    Panel::MIN_WIDTH
                }
            };
            panels.left.size = fit(panels.left.size);
            panels.right.size = fit(panels.right.size);
        }
        panels.bottom.size = if panels.bottom.size.is_finite() {
            panels.bottom.size.clamp(Panel::MIN_HEIGHT, bottom_max)
        } else {
            Panel::MIN_HEIGHT
        };
        self.settings.panels = Some(panels);
        if before != (self.settings.column_width, self.settings.panels.clone()) {
            self.store_settings();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimum_derives_from_footer_and_dock_budgets() {
        for (font, w, h) in [
            (10., 900., 500.),
            (13., 900., 500.),
            // The column's floor grows with the interface, and the window
            // with it: 202 at 13, 249 at 16, 311 at 20 (`column::narrowest`).
            (16., 924., 500.),
            (20., 1155., 500.),
        ] {
            assert_eq!(minimum(font, 0., 0.), size(px(w), px(h)), "UI{font}");
        }
        // Keep the derived height if future UI scales need more than the floor.
        assert!(minimum(100., 0., 0.).height > px(500.));
        assert!(minimum(13., 400., 0.).width > px(900.));
        assert!(minimum(13., 0., 300.).width > px(900.));
        // The work's width at the window's minimum: what the column's floor
        // leaves of the 900.
        assert_eq!(icon_gap(13., 900. - column::narrowest(13.), 154., 0.), 0.);
        let least = 900. - column::narrowest(13.);
        assert!(icon_gap(13., least + 20., 154., 0.) < icon_gap(13., 800., 154., 0.));
        assert_eq!(icon_gap(13., 800., 154., 0.), 2.);
    }
    /// Indexing with a long path and 34%, in the widths the bar draws at
    /// 13 points: padding and spinner, title, dot and the least of the
    /// detail, the detail to its 240 cut, the bar, the percentage.
    const INDEXING: ServerLine = ServerLine {
        frame: 32.0,
        title: 48.0,
        detail: 38.0,
        detail_full: 248.0,
        more: 0.0,
        bar: 50.0,
        percent: 24.0,
    };

    fn parts(title: bool, detail: bool, bar: bool) -> (bool, bool, bool) {
        (title, detail, bar)
    }

    fn drawn(p: ServerParts) -> (bool, bool, bool) {
        (p.title, p.detail, p.bar)
    }

    #[test]
    fn a_server_line_gives_way_detail_then_bar_then_title() {
        let all = INDEXING.frame
            + INDEXING.title
            + INDEXING.detail_full
            + INDEXING.bar
            + INDEXING.percent;
        let whole = server_parts(all, INDEXING);
        assert_eq!(drawn(whole), parts(true, true, true));
        assert!(!whole.cut, "everything fits: the tooltip stays one line");
        // Shy of the whole detail: drawn, cut by its ellipsis.
        let cut = server_parts(all - 1.0, INDEXING);
        assert_eq!(drawn(cut), parts(true, true, true));
        assert!(cut.cut);
        let least = all - INDEXING.detail_full + INDEXING.detail;
        assert_eq!(
            drawn(server_parts(least, INDEXING)),
            parts(true, true, true)
        );
        // Below the least of the detail, the detail goes, and its count with it.
        let more = ServerLine {
            more: 20.0,
            ..INDEXING
        };
        let gone = server_parts(least + more.more - 1.0, more);
        assert_eq!(drawn(gone), parts(true, false, true));
        assert!(!gone.more);
        let no_detail = least - INDEXING.detail;
        assert_eq!(
            drawn(server_parts(no_detail - 1.0, INDEXING)),
            parts(true, false, false),
            "then the bar"
        );
        let no_bar = no_detail - INDEXING.bar;
        let last = server_parts(no_bar - 1.0, INDEXING);
        assert_eq!(drawn(last), parts(false, false, false), "then the title");
        assert!(last.cut);
        // The spinner and the percentage stay even with no room at all.
        assert_eq!(drawn(server_parts(0.0, INDEXING)), drawn(last));
        // With no percentage there is nothing else to say: the title stays.
        let starting = ServerLine {
            title: 90.0,
            ..ServerLine::default()
        };
        assert!(server_parts(0.0, starting).title);
        assert!(server_parts(0.0, starting).cut);
    }

    /// The approved widths: 1512 with the column at 232, the default 1180,
    /// and the 900 minimum with the column at 180. Five centred tools, and
    /// the file's facts on the right — `↩ rust LF 1:1`, 134 points.
    #[test]
    fn indexing_gives_way_at_the_approved_widths() {
        let room = |window: f32, column: f32| {
            let g = crate::panel_corners::Geometry::new(window, Some(column), None, 6.0)
                .relative_to(column);
            let tools = 5.0 * 24.0 + 4.0 * 2.0;
            server_room(13.0, window - column, g.center() + tools / 2.0, 134.0)
        };
        let wide = server_parts(room(1512.0, 232.0), INDEXING);
        assert_eq!(drawn(wide), parts(true, true, true));
        let default = server_parts(room(1180.0, 232.0), INDEXING);
        assert_eq!(drawn(default), parts(true, true, true));
        assert!(default.cut, "only the detail is cut at 1180");
        let least = server_parts(room(900.0, 180.0), INDEXING);
        assert!(
            !least.detail && !least.bar,
            "the detail and the bar give way at 900"
        );
        // And none of it is the minimum's: it stays the floor whatever runs.
        assert_eq!(minimum(13.0, 128.0, 134.0).width, px(900.0));
    }

    /// At the minimum the stable side has the room it was counted with:
    /// the right side's bound never cuts it, at any scale, however wide it
    /// is, so only the server's line gives way.
    #[test]
    fn the_right_side_bound_never_cuts_the_stable_side_at_the_minimum() {
        for font in [10.0, 13.0, 16.0, 20.0] {
            for stable in [0.0, 134.0, 400.0, 900.0] {
                // Wider than the row the minimum reserves, as a counted
                // Problems badge makes it: then no slack is left over.
                let reserved = (guild_session::Tool::ALL.len() - 1) as f32
                    * theme::space::STATUS_BUTTON.0
                    * f32::from(scale::rem_size(font));
                let tools = reserved + 40.0;
                let window = f32::from(minimum(font, tools, stable).width);
                let column = column::narrowest(font);
                let g = crate::panel_corners::Geometry::new(window, Some(column), None, 6.0)
                    .relative_to(column);
                let room = right_room(font, window - column, g.center() + tools / 2.0);
                assert!(
                    room >= stable,
                    "{room} < {stable} at {font} points in a {window} window"
                );
            }
        }
    }

    #[test]
    fn restoration_fits_saved_small_large_offscreen_and_invalid_bounds() {
        let display = Bounds::new(point(px(100.), px(40.)), size(px(1920.), px(1080.)));
        for (w, h, expected) in [
            (10., 20., size(px(900.), px(500.))),
            (9000., 8000., display.size),
        ] {
            let bounds = restored_bounds(
                Some(WindowPlace {
                    x: -5000.,
                    y: 9000.,
                    width: w,
                    height: h,
                }),
                display,
                13.,
            );
            assert_eq!(bounds.size, expected);
            assert!(bounds.left() >= display.left() && bounds.right() <= display.right());
            assert!(bounds.top() >= display.top() && bounds.bottom() <= display.bottom());
        }
        assert!(
            restored_bounds(
                Some(WindowPlace {
                    x: f32::NAN,
                    y: 0.,
                    width: 0.,
                    height: 0.
                }),
                display,
                13.
            )
            .size
            .width
                >= px(900.)
        );
        let (left, right) = fit_sides(9000., -10., 500.);
        assert_eq!(left + right, 500.);
        assert!(left >= 160. && right >= 160.);
    }
}
