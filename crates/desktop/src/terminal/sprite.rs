//! Blocks and box lines, drawn to the cell's edges rather than typed —
//! M5.14 task 10. *Sprite* characters, as GPU terminals call them.
//!
//! `claude`'s logo is block elements (`▐▛███▜▌`). Taken from the font they
//! stop at the font's own height, and since the terminal's rows are 16 on 12
//! a line of background showed between them. GPU terminals draw these
//! characters themselves, to the cell's edges; so does this. Pure: the view
//! paints what it answers, in the character's colour, every edge snapped to
//! the device pixel.
//!
//! **The box table is generated from the Unicode names** (`BOX DRAWINGS
//! LIGHT DOWN AND RIGHT` and the rest), not typed: 128 characters of four
//! arms each are a place for a typo nobody would see. Since task 13 the
//! whole block is drawn: the dashes, the diagonals and the double corners
//! too, which task 10 had left to the font.

/// A filled rectangle in the cell's own pixels, from its top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// How much of the character's colour: 1 for a block, less for a shade.
    pub alpha: f32,
}

/// A line through points, for the rounded corners.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub points: Vec<(f32, f32)>,
    pub width: f32,
}

/// What a character is drawn as.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sprite {
    pub pieces: Vec<Piece>,
    pub strokes: Vec<Stroke>,
}

/// Whether this character is drawn here rather than by the font.
pub fn drawn(c: char) -> bool {
    block(c, 1.0, 1.0).is_some()
        || arms(c).is_some()
        || arc(c).is_some()
        || dashes(c).is_some()
        || diagonal(c).is_some()
}

/// A character drawn in a cell `width` by `height`, lines `light` thick —
/// `None` for one the font draws.
pub fn sprite(c: char, width: f32, height: f32, light: f32) -> Option<Sprite> {
    if let Some(pieces) = block(c, width, height) {
        return Some(Sprite {
            pieces,
            strokes: Vec::new(),
        });
    }
    if let Some(arms) = arms(c) {
        let pieces = if arms.contains(&3) {
            doubles(arms, width, height, light)
        } else {
            lines(arms, width, height, light)
        };
        return Some(Sprite {
            pieces,
            strokes: Vec::new(),
        });
    }
    if let Some(dashes) = dashes(c) {
        return Some(Sprite {
            pieces: dashed(dashes, width, height, light),
            strokes: Vec::new(),
        });
    }
    if let Some((rising, falling)) = diagonal(c) {
        let strokes = [
            rising.then(|| vec![(width, 0.0), (0.0, height)]),
            falling.then(|| vec![(0.0, 0.0), (width, height)]),
        ]
        .into_iter()
        .flatten()
        .map(|points| Stroke {
            points,
            width: light,
        })
        .collect();
        return Some(Sprite {
            pieces: Vec::new(),
            strokes,
        });
    }
    let corner = arc(c)?;
    Some(Sprite {
        pieces: Vec::new(),
        strokes: vec![rounded(corner, width, height, light)],
    })
}

/// A coordinate in logical pixels moved to the nearest device pixel.
///
/// **Both edges of a quad go through this, never its origin and a size**:
/// the rows are 16 on 12 and fall on fractions of a pixel, and a quad
/// rounded as one piece leaves its neighbour a hairline away — the seam
/// under `claude`'s prompt box, task 12, and the line through its logo,
/// task 10.
pub fn snap(value: f32, scale: f32) -> f32 {
    (value * scale).round() / scale
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

/// U+2580–259F: halves, eighths, quadrants and shades.
fn block(c: char, w: f32, h: f32) -> Option<Vec<Piece>> {
    let code = c as u32;
    let (hw, hh) = (w / 2.0, h / 2.0);
    let quadrant = |upper_left: bool, upper_right: bool, lower_left: bool, lower_right: bool| {
        [
            (upper_left, solid(0.0, 0.0, hw, hh)),
            (upper_right, solid(hw, 0.0, w, hh)),
            (lower_left, solid(0.0, hh, hw, h)),
            (lower_right, solid(hw, hh, w, h)),
        ]
        .into_iter()
        .filter_map(|(on, piece)| on.then_some(piece))
        .collect()
    };
    let shade = |alpha: f32| {
        vec![Piece {
            x0: 0.0,
            y0: 0.0,
            x1: w,
            y1: h,
            alpha,
        }]
    };
    Some(match code {
        0x2580 => vec![solid(0.0, 0.0, w, hh)],
        // Lower one eighth to seven eighths, then the full block.
        0x2581..=0x2588 => {
            let eighths = (code - 0x2580) as f32;
            vec![solid(0.0, h - h * eighths / 8.0, w, h)]
        }
        // Left seven eighths down to one eighth.
        0x2589..=0x258f => {
            let eighths = (0x2590 - code) as f32;
            vec![solid(0.0, 0.0, w * eighths / 8.0, h)]
        }
        0x2590 => vec![solid(hw, 0.0, w, h)],
        0x2591 => shade(0.25),
        0x2592 => shade(0.5),
        0x2593 => shade(0.75),
        0x2594 => vec![solid(0.0, 0.0, w, h / 8.0)],
        0x2595 => vec![solid(w - w / 8.0, 0.0, w, h)],
        0x2596 => quadrant(false, false, true, false),
        0x2597 => quadrant(false, false, false, true),
        0x2598 => quadrant(true, false, false, false),
        0x2599 => quadrant(true, false, true, true),
        0x259a => quadrant(true, false, false, true),
        0x259b => quadrant(true, true, true, false),
        0x259c => quadrant(true, true, false, true),
        0x259d => quadrant(false, true, false, false),
        0x259e => quadrant(false, true, true, false),
        0x259f => quadrant(false, true, true, true),
        _ => return None,
    })
}

/// A box character's arms — up, right, down, left — each 0 for none, 1
/// light, 2 heavy, 3 double. Heavy and double never share a character.
fn arms(c: char) -> Option<[u8; 4]> {
    BOX.binary_search_by_key(&c, |(key, _)| *key)
        .ok()
        .map(|at| BOX[at].1)
}

/// The arms as rectangles: each from the edge to the middle, reaching
/// past it by half the thickest arm so that they join without a notch.
fn lines(arms: [u8; 4], w: f32, h: f32, light: f32) -> Vec<Piece> {
    let thick = |weight: u8| match weight {
        2 => 2.0 * light,
        _ => light,
    };
    let (cx, cy) = (w / 2.0, h / 2.0);
    let across = arms
        .iter()
        .copied()
        .filter(|weight| *weight > 0)
        .map(thick)
        .fold(0.0_f32, f32::max)
        / 2.0;
    let mut pieces = Vec::new();
    for (arm, weight) in arms.into_iter().enumerate() {
        if weight == 0 {
            continue;
        }
        let t = thick(weight);
        pieces.push(match arm {
            0 => solid(cx - t / 2.0, 0.0, cx + t / 2.0, cy + across),
            1 => solid(cx - across, cy - t / 2.0, w, cy + t / 2.0),
            2 => solid(cx - t / 2.0, cy - across, cx + t / 2.0, h),
            _ => solid(0.0, cy - t / 2.0, cx + across, cy + t / 2.0),
        });
    }
    pieces
}

/// The arms of a character with a double line among them — task 13.
///
/// A double arm is two light lines a light either side of the middle, and
/// **each line stops where it turns**: in `╔` the
/// outer lines meet at the outer point and the inner ones at the inner,
/// in `╬` all four turn and the middle is open, in `╠` the far line runs
/// whole and the near one is broken by the arm. A single line meeting a
/// double one stops at the line it meets — the near one where it crosses
/// a straight double (`╤`), the far one where it turns a corner (`╓`).
///
/// Where a line stops is a distance from the middle along its arm, a
/// negative one being past the middle; every line then reaches half a
/// light further, so two that meet overlap instead of leaving a notch.
fn doubles(arms: [u8; 4], w: f32, h: f32, light: f32) -> Vec<Piece> {
    let (cx, cy, d, half) = (w / 2.0, h / 2.0, light, light / 2.0);
    let opposite = |arm: usize| (arm + 2) % 4;
    let reach = |weight: u8| if weight == 3 { d } else { 0.0 };
    let mut pieces = Vec::new();
    for (arm, weight) in arms.into_iter().enumerate() {
        if weight == 0 {
            continue;
        }
        // The arms either side of this one, the lesser coordinate first.
        let sides = if arm % 2 == 0 { [3, 1] } else { [0, 2] };
        // (offset across the arm, where the line stops)
        let lines: Vec<(f32, f32)> = if weight == 3 {
            sides
                .iter()
                .enumerate()
                .map(|(at, &side)| {
                    let across = if at == 0 { -d } else { d };
                    let stop = if arms[side] > 0 {
                        reach(arms[side])
                    } else if arms[opposite(arm)] > 0 {
                        0.0
                    } else {
                        -reach(arms[opposite(side)])
                    };
                    (across, stop)
                })
                .collect()
        } else {
            let both = sides.iter().all(|&side| arms[side] > 0);
            let stop = if !sides.iter().any(|&side| arms[side] == 3) {
                0.0
            } else if both {
                d
            } else {
                -d
            };
            vec![(0.0, stop)]
        };
        for (across, stop) in lines {
            let end = stop - half;
            pieces.push(match arm {
                0 => solid(cx + across - half, 0.0, cx + across + half, cy - end),
                1 => solid(cx + end, cy + across - half, w, cy + across + half),
                2 => solid(cx + across - half, cy + end, cx + across + half, h),
                _ => solid(0.0, cy + across - half, cx - end, cy + across + half),
            });
        }
    }
    pieces
}

/// A dashed line: how many dashes, whether it runs across, and whether
/// it is heavy — U+2504–250B and U+254C–254F.
fn dashes(c: char) -> Option<(usize, bool, bool)> {
    Some(match c {
        '╌' => (2, true, false),
        '╍' => (2, true, true),
        '╎' => (2, false, false),
        '╏' => (2, false, true),
        '┄' => (3, true, false),
        '┅' => (3, true, true),
        '┆' => (3, false, false),
        '┇' => (3, false, true),
        '┈' => (4, true, false),
        '┉' => (4, true, true),
        '┊' => (4, false, false),
        '┋' => (4, false, true),
        _ => return None,
    })
}

/// The dashes, each two thirds of its share of the cell and centred in
/// it, so the gaps are even across a row of them.
fn dashed((count, across, heavy): (usize, bool, bool), w: f32, h: f32, light: f32) -> Vec<Piece> {
    let t = if heavy { 2.0 * light } else { light };
    let step = if across { w } else { h } / count as f32;
    let gap = step / 3.0;
    (0..count)
        .map(|at| {
            let from = step * at as f32 + gap / 2.0;
            let to = step * (at + 1) as f32 - gap / 2.0;
            if across {
                solid(from, h / 2.0 - t / 2.0, to, h / 2.0 + t / 2.0)
            } else {
                solid(w / 2.0 - t / 2.0, from, w / 2.0 + t / 2.0, to)
            }
        })
        .collect()
}

/// The diagonals, U+2571–2573: whether it rises, whether it falls.
fn diagonal(c: char) -> Option<(bool, bool)> {
    Some(match c {
        '╱' => (true, false),
        '╲' => (false, true),
        '╳' => (true, true),
        _ => return None,
    })
}

/// Which way a rounded corner opens: the two arms it joins.
#[derive(Debug, Clone, Copy)]
enum Corner {
    DownRight,
    DownLeft,
    UpLeft,
    UpRight,
}

fn arc(c: char) -> Option<Corner> {
    Some(match c {
        '╭' => Corner::DownRight,
        '╮' => Corner::DownLeft,
        '╯' => Corner::UpLeft,
        '╰' => Corner::UpRight,
        _ => return None,
    })
}

/// A rounded corner: from the middle of one edge, straight, then a
/// quarter circle of radius half the cell's width into the middle of the
/// other — sampled, so the view strokes a polyline.
fn rounded(corner: Corner, w: f32, h: f32, light: f32) -> Stroke {
    let (cx, cy) = (w / 2.0, h / 2.0);
    let r = cx.min(cy);
    // Drawn for ╭ (down and right) and mirrored for the others.
    let (flip_x, flip_y) = match corner {
        Corner::DownRight => (false, false),
        Corner::DownLeft => (true, false),
        Corner::UpLeft => (true, true),
        Corner::UpRight => (false, true),
    };
    let place = |(x, y): (f32, f32)| {
        (
            if flip_x { w - x } else { x },
            if flip_y { h - y } else { y },
        )
    };
    let centre = (cx + r, cy + r);
    let mut points = vec![place((cx, h))];
    const STEPS: usize = 8;
    for step in 0..=STEPS {
        let angle = std::f32::consts::PI + std::f32::consts::FRAC_PI_2 * step as f32 / STEPS as f32;
        points.push(place((
            centre.0 + r * angle.cos(),
            centre.1 + r * angle.sin(),
        )));
    }
    points.push(place((w, cy)));
    points.dedup();
    Stroke {
        points,
        width: light,
    }
}

/// Generated from the Unicode names of U+2500–257F; sorted by character.
const BOX: &[(char, [u8; 4])] = &[
    ('\u{2500}', [0, 1, 0, 1]), // light horizontal
    ('\u{2501}', [0, 2, 0, 2]), // heavy horizontal
    ('\u{2502}', [1, 0, 1, 0]), // light vertical
    ('\u{2503}', [2, 0, 2, 0]), // heavy vertical
    ('\u{250c}', [0, 1, 1, 0]), // light down and right
    ('\u{250d}', [0, 2, 1, 0]), // down light and right heavy
    ('\u{250e}', [0, 1, 2, 0]), // down heavy and right light
    ('\u{250f}', [0, 2, 2, 0]), // heavy down and right
    ('\u{2510}', [0, 0, 1, 1]), // light down and left
    ('\u{2511}', [0, 0, 1, 2]), // down light and left heavy
    ('\u{2512}', [0, 0, 2, 1]), // down heavy and left light
    ('\u{2513}', [0, 0, 2, 2]), // heavy down and left
    ('\u{2514}', [1, 1, 0, 0]), // light up and right
    ('\u{2515}', [1, 2, 0, 0]), // up light and right heavy
    ('\u{2516}', [2, 1, 0, 0]), // up heavy and right light
    ('\u{2517}', [2, 2, 0, 0]), // heavy up and right
    ('\u{2518}', [1, 0, 0, 1]), // light up and left
    ('\u{2519}', [1, 0, 0, 2]), // up light and left heavy
    ('\u{251a}', [2, 0, 0, 1]), // up heavy and left light
    ('\u{251b}', [2, 0, 0, 2]), // heavy up and left
    ('\u{251c}', [1, 1, 1, 0]), // light vertical and right
    ('\u{251d}', [1, 2, 1, 0]), // vertical light and right heavy
    ('\u{251e}', [2, 1, 1, 0]), // up heavy and right down light
    ('\u{251f}', [1, 1, 2, 0]), // down heavy and right up light
    ('\u{2520}', [2, 1, 2, 0]), // vertical heavy and right light
    ('\u{2521}', [2, 2, 1, 0]), // down light and right up heavy
    ('\u{2522}', [1, 2, 2, 0]), // up light and right down heavy
    ('\u{2523}', [2, 2, 2, 0]), // heavy vertical and right
    ('\u{2524}', [1, 0, 1, 1]), // light vertical and left
    ('\u{2525}', [1, 0, 1, 2]), // vertical light and left heavy
    ('\u{2526}', [2, 0, 1, 1]), // up heavy and left down light
    ('\u{2527}', [1, 0, 2, 1]), // down heavy and left up light
    ('\u{2528}', [2, 0, 2, 1]), // vertical heavy and left light
    ('\u{2529}', [2, 0, 1, 2]), // down light and left up heavy
    ('\u{252a}', [1, 0, 2, 2]), // up light and left down heavy
    ('\u{252b}', [2, 0, 2, 2]), // heavy vertical and left
    ('\u{252c}', [0, 1, 1, 1]), // light down and horizontal
    ('\u{252d}', [0, 1, 1, 2]), // left heavy and right down light
    ('\u{252e}', [0, 2, 1, 1]), // right heavy and left down light
    ('\u{252f}', [0, 2, 1, 2]), // down light and horizontal heavy
    ('\u{2530}', [0, 1, 2, 1]), // down heavy and horizontal light
    ('\u{2531}', [0, 1, 2, 2]), // right light and left down heavy
    ('\u{2532}', [0, 2, 2, 1]), // left light and right down heavy
    ('\u{2533}', [0, 2, 2, 2]), // heavy down and horizontal
    ('\u{2534}', [1, 1, 0, 1]), // light up and horizontal
    ('\u{2535}', [1, 1, 0, 2]), // left heavy and right up light
    ('\u{2536}', [1, 2, 0, 1]), // right heavy and left up light
    ('\u{2537}', [1, 2, 0, 2]), // up light and horizontal heavy
    ('\u{2538}', [2, 1, 0, 1]), // up heavy and horizontal light
    ('\u{2539}', [2, 1, 0, 2]), // right light and left up heavy
    ('\u{253a}', [2, 2, 0, 1]), // left light and right up heavy
    ('\u{253b}', [2, 2, 0, 2]), // heavy up and horizontal
    ('\u{253c}', [1, 1, 1, 1]), // light vertical and horizontal
    ('\u{253d}', [1, 1, 1, 2]), // left heavy and right vertical light
    ('\u{253e}', [1, 2, 1, 1]), // right heavy and left vertical light
    ('\u{253f}', [1, 2, 1, 2]), // vertical light and horizontal heavy
    ('\u{2540}', [2, 1, 1, 1]), // up heavy and down horizontal light
    ('\u{2541}', [1, 1, 2, 1]), // down heavy and up horizontal light
    ('\u{2542}', [2, 1, 2, 1]), // vertical heavy and horizontal light
    ('\u{2543}', [2, 1, 1, 2]), // left up heavy and right down light
    ('\u{2544}', [2, 2, 1, 1]), // right up heavy and left down light
    ('\u{2545}', [1, 1, 2, 2]), // left down heavy and right up light
    ('\u{2546}', [1, 2, 2, 1]), // right down heavy and left up light
    ('\u{2547}', [2, 2, 1, 2]), // down light and up horizontal heavy
    ('\u{2548}', [1, 2, 2, 2]), // up light and down horizontal heavy
    ('\u{2549}', [2, 1, 2, 2]), // right light and left vertical heavy
    ('\u{254a}', [2, 2, 2, 1]), // left light and right vertical heavy
    ('\u{254b}', [2, 2, 2, 2]), // heavy vertical and horizontal
    ('\u{2550}', [0, 3, 0, 3]), // double horizontal
    ('\u{2551}', [3, 0, 3, 0]), // double vertical
    ('\u{2552}', [0, 3, 1, 0]), // down single and right double
    ('\u{2553}', [0, 1, 3, 0]), // down double and right single
    ('\u{2554}', [0, 3, 3, 0]), // double down and right
    ('\u{2555}', [0, 0, 1, 3]), // down single and left double
    ('\u{2556}', [0, 0, 3, 1]), // down double and left single
    ('\u{2557}', [0, 0, 3, 3]), // double down and left
    ('\u{2558}', [1, 3, 0, 0]), // up single and right double
    ('\u{2559}', [3, 1, 0, 0]), // up double and right single
    ('\u{255a}', [3, 3, 0, 0]), // double up and right
    ('\u{255b}', [1, 0, 0, 3]), // up single and left double
    ('\u{255c}', [3, 0, 0, 1]), // up double and left single
    ('\u{255d}', [3, 0, 0, 3]), // double up and left
    ('\u{255e}', [1, 3, 1, 0]), // vertical single and right double
    ('\u{255f}', [3, 1, 3, 0]), // vertical double and right single
    ('\u{2560}', [3, 3, 3, 0]), // double vertical and right
    ('\u{2561}', [1, 0, 1, 3]), // vertical single and left double
    ('\u{2562}', [3, 0, 3, 1]), // vertical double and left single
    ('\u{2563}', [3, 0, 3, 3]), // double vertical and left
    ('\u{2564}', [0, 3, 1, 3]), // down single and horizontal double
    ('\u{2565}', [0, 1, 3, 1]), // down double and horizontal single
    ('\u{2566}', [0, 3, 3, 3]), // double down and horizontal
    ('\u{2567}', [1, 3, 0, 3]), // up single and horizontal double
    ('\u{2568}', [3, 1, 0, 1]), // up double and horizontal single
    ('\u{2569}', [3, 3, 0, 3]), // double up and horizontal
    ('\u{256a}', [1, 3, 1, 3]), // vertical single and horizontal double
    ('\u{256b}', [3, 1, 3, 1]), // vertical double and horizontal single
    ('\u{256c}', [3, 3, 3, 3]), // double vertical and horizontal
    ('\u{2574}', [0, 0, 0, 1]), // light left
    ('\u{2575}', [1, 0, 0, 0]), // light up
    ('\u{2576}', [0, 1, 0, 0]), // light right
    ('\u{2577}', [0, 0, 1, 0]), // light down
    ('\u{2578}', [0, 0, 0, 2]), // heavy left
    ('\u{2579}', [2, 0, 0, 0]), // heavy up
    ('\u{257a}', [0, 2, 0, 0]), // heavy right
    ('\u{257b}', [0, 0, 2, 0]), // heavy down
    ('\u{257c}', [0, 2, 0, 1]), // light left and heavy right
    ('\u{257d}', [1, 0, 2, 0]), // light up and heavy down
    ('\u{257e}', [0, 1, 0, 2]), // heavy left and light right
    ('\u{257f}', [2, 0, 1, 0]), // heavy up and light down
];
