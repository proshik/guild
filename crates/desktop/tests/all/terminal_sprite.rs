//! Blocks and box lines, drawn to the cell's edges — M5.14 task 10.
//!
//! `claude`'s logo is block elements, and taken from the font they stop
//! at the font's height: since the terminal's rows are 16 on 12, a line
//! of background showed between them. GPU terminals draw these characters
//! themselves, and so does this.

use guild_desktop::terminal::sprite::{Piece, drawn, snap, sprite};

const W: f32 = 8.0;
const H: f32 = 16.0;
const LIGHT: f32 = 1.0;

fn pieces(c: char) -> Vec<Piece> {
    sprite(c, W, H, LIGHT).expect("drawn").pieces
}

fn solid(x0: f32, y0: f32, x1: f32, y1: f32) -> Piece {
    Piece {
        x0,
        y0,
        x1,
        y1,
        alpha: 1.0,
    }
}

#[test]
fn a_block_fills_its_part_of_the_cell_to_the_edges() {
    assert_eq!(pieces('█'), vec![solid(0.0, 0.0, W, H)]);
    assert_eq!(pieces('▀'), vec![solid(0.0, 0.0, W, H / 2.0)]);
    assert_eq!(pieces('▄'), vec![solid(0.0, H / 2.0, W, H)]);
    assert_eq!(pieces('▐'), vec![solid(W / 2.0, 0.0, W, H)]);
    assert_eq!(pieces('▂'), vec![solid(0.0, H - H / 4.0, W, H)]);
    assert_eq!(pieces('▏'), vec![solid(0.0, 0.0, W / 8.0, H)]);
    assert_eq!(
        pieces('▛'),
        vec![
            solid(0.0, 0.0, W / 2.0, H / 2.0),
            solid(W / 2.0, 0.0, W, H / 2.0),
            solid(0.0, H / 2.0, W / 2.0, H),
        ],
        "claude's logo: three quadrants"
    );
    assert_eq!(pieces('▒')[0].alpha, 0.5);
}

#[test]
fn a_box_line_runs_from_edge_to_edge_through_the_middle() {
    let vertical = pieces('│');
    assert!(vertical.iter().any(|p| p.y0 == 0.0), "reaches the top");
    assert!(vertical.iter().any(|p| p.y1 == H), "reaches the bottom");
    assert!(
        vertical
            .iter()
            .all(|p| p.x1 - p.x0 == LIGHT && (p.x0 + p.x1) / 2.0 == W / 2.0)
    );

    let heavy = pieces('━');
    assert!(
        heavy.iter().all(|p| p.y1 - p.y0 == 2.0 * LIGHT),
        "heavy is twice light"
    );

    let cross = pieces('┼');
    for (x, y) in [(W / 2.0, 0.0), (W, H / 2.0), (W / 2.0, H), (0.0, H / 2.0)] {
        assert!(
            cross
                .iter()
                .any(|p| p.x0 <= x && x <= p.x1 && p.y0 <= y && y <= p.y1),
            "an arm reaches ({x}, {y})"
        );
    }
}

#[test]
fn a_rounded_corner_is_a_stroke_from_one_edge_to_the_other() {
    let corner = sprite('╭', W, H, LIGHT).expect("drawn");
    let stroke = &corner.strokes[0];
    assert_eq!(
        stroke.points.first(),
        Some(&(W / 2.0, H)),
        "from the bottom's middle"
    );
    assert_eq!(
        stroke.points.last(),
        Some(&(W, H / 2.0)),
        "to the right's middle"
    );
    assert_eq!(stroke.width, LIGHT);
}

#[test]
fn what_is_not_drawn_is_left_to_the_font() {
    assert!(drawn('▛') && drawn('─') && drawn('╭') && drawn('═'));
    assert!(!drawn('a') && !drawn('✻') && !drawn('⎿'));
    assert!(sprite('a', W, H, LIGHT).is_none());
}

/// **The whole box-drawing block is drawn** — task 13: after task 10 the
/// dashes, the diagonals and the double corners were still the font's,
/// at the font's height, and a `╔═╗` frame broke between its rows.
#[test]
fn every_box_drawing_character_is_drawn() {
    for code in 0x2500..=0x257f {
        let c = char::from_u32(code).expect("a character");
        assert!(drawn(c), "{c} (U+{code:04X}) is drawn");
        assert!(sprite(c, W, H, LIGHT).is_some(), "{c} has a sprite");
    }
}

fn covers(pieces: &[Piece], x: f32, y: f32) -> bool {
    pieces
        .iter()
        .any(|p| p.x0 <= x && x <= p.x1 && p.y0 <= y && y <= p.y1)
}

/// **A double corner is two corners, the outer and the inner** — task
/// 13: in `╔` the outer lines turn at the outer
/// point and the inner lines at the inner one, a light either side of
/// the middle, each reaching past its turn by half its thickness.
#[test]
fn a_double_corner_turns_its_outer_and_inner_lines_apart() {
    let (cx, cy, d, t) = (W / 2.0, H / 2.0, LIGHT, LIGHT / 2.0);
    let corner = pieces('╔');
    assert_eq!(corner.len(), 4, "{corner:?}");
    for piece in [
        solid(cx - d - t, cy - d - t, W, cy - d + t),
        solid(cx + d - t, cy + d - t, W, cy + d + t),
        solid(cx - d - t, cy - d - t, cx - d + t, H),
        solid(cx + d - t, cy + d - t, cx + d + t, H),
    ] {
        assert!(corner.contains(&piece), "{piece:?} in {corner:?}");
    }
}

/// **Where double lines cross, the middle is open**, and where a single
/// line meets a double one it stops at the line it meets.
#[test]
fn double_lines_meet_without_crossing_the_middle() {
    let (cx, cy, d) = (W / 2.0, H / 2.0, LIGHT);
    assert!(!covers(&pieces('╬'), cx, cy), "╬ is open in the middle");
    // ╠: the left line runs whole, the right one is broken by the arm.
    let tee = pieces('╠');
    assert!((0..=16).all(|y| covers(&tee, cx - d, y as f32)));
    assert!(!covers(&tee, cx + d, cy));
    // ╤: the single line down stops at the lower double line.
    let down = pieces('╤');
    assert!(covers(&down, cx, cy + d) && !covers(&down, cx, cy - d + LIGHT));
    // ╓: the single arm reaches across to the outer double line.
    assert!(covers(&pieces('╓'), cx - d, cy));
}

/// **A dashed line is its dashes, with gaps that tile across cells.**
#[test]
fn a_dashed_line_is_its_count_of_dashes() {
    for (c, count, horizontal) in [
        ('╌', 2, true),
        ('┄', 3, true),
        ('┈', 4, true),
        ('╎', 2, false),
        ('┆', 3, false),
        ('┊', 4, false),
        ('┅', 3, true),
    ] {
        let dashes = pieces(c);
        assert_eq!(dashes.len(), count, "{c}");
        let along = if horizontal { W } else { H };
        let covered: f32 = dashes
            .iter()
            .map(|p| if horizontal { p.x1 - p.x0 } else { p.y1 - p.y0 })
            .sum();
        assert!(covered < along && covered > along / 2.0, "{c}: {covered}");
        assert!(
            dashes
                .iter()
                .all(|p| p.x0 >= 0.0 && p.x1 <= W && p.y0 >= 0.0 && p.y1 <= H),
            "{c} stays in its cell"
        );
    }
    assert_eq!(pieces('┅')[0].y1 - pieces('┅')[0].y0, 2.0 * LIGHT, "heavy");
}

/// **A diagonal runs corner to corner**, so a line of them is unbroken.
#[test]
fn a_diagonal_is_a_stroke_from_corner_to_corner() {
    let rising = sprite('╱', W, H, LIGHT).expect("drawn").strokes;
    assert_eq!(rising.len(), 1);
    assert_eq!(rising[0].points, vec![(W, 0.0), (0.0, H)]);
    let falling = sprite('╲', W, H, LIGHT).expect("drawn").strokes;
    assert_eq!(falling[0].points, vec![(0.0, 0.0), (W, H)]);
    assert_eq!(sprite('╳', W, H, LIGHT).expect("drawn").strokes.len(), 2);
}

/// **Two rows of one background meet on a device pixel** — task 12. A
/// row 16.67 px tall starts on a fraction of a pixel, and two quads
/// rounded each on its own left a hairline between them.
/// Snapped, a row's bottom is exactly the next one's top, and every edge
/// is a whole device pixel.
#[test]
fn a_row_ends_on_the_pixel_the_next_one_starts_on() {
    let pitch = 50.0 / 3.0;
    for scale in [1.0_f32, 2.0] {
        for row in 0..40 {
            let top = pitch * row as f32;
            let bottom = snap(top + pitch, scale);
            assert_eq!(bottom, snap(pitch * (row + 1) as f32, scale));
            assert_eq!((bottom * scale).fract(), 0.0, "row {row} at {scale}x");
            assert!(bottom > snap(top, scale), "row {row} has no height");
        }
    }
}
