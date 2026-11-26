//! A restored Codex tab's local screen, kept separate from its live parser.

use std::io::{BufRead, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::color::{COUNT, Colors};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, Rgb, StdSyncHandler};
use serde::{Deserialize, Serialize};

use crate::terminal::emulator::GridSize;

const LIMIT: u64 = 4 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize)]
struct Screen {
    conversation: String,
    columns: usize,
    rows: usize,
    cells: Vec<Cell>,
    colors: Vec<Option<Rgb>>,
}

pub(super) struct Cache {
    path: PathBuf,
    pub conversation: Option<String>,
}

impl Cache {
    pub fn new(root: &Path, session: &str, conversation: Option<&str>) -> Option<Self> {
        // Never turn an external identifier into a path component.
        let session: guild_session::SessionId = session.parse().ok()?;
        Some(Self {
            path: root.join(format!("{session}.json")),
            conversation: conversation.map(str::to_owned),
        })
    }

    pub fn preview(&self, home: &Path, size: GridSize) -> Option<Preview> {
        let id = self.conversation.as_deref()?;
        let rollout = guild_session::codex_transcript::path(home, id);
        let messages = rollout.as_deref().map(messages).unwrap_or_default();
        let marker = messages
            .last()
            .map(|text| fingerprint(text))
            .unwrap_or_default();
        let screen = std::fs::File::open(&self.path).ok().and_then(|file| {
            let mut bytes = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
            if bytes.len() as u64 > LIMIT {
                return None;
            }
            let screen: Screen = serde_json::from_slice(&bytes).ok()?;
            (screen.conversation == id
                && screen.columns > 0
                && screen.columns <= 500
                && screen.rows > 0
                && screen.rows <= 300
                && screen.cells.len() == screen.columns * screen.rows
                && screen.colors.len() == COUNT)
                .then_some(screen)
        });
        let (grid, colors) = if let Some(screen) = screen {
            let mut grid = Grid::new(screen.rows, screen.columns, 0);
            for (i, cell) in screen.cells.into_iter().enumerate() {
                grid[Point::new(
                    Line((i / screen.columns) as i32),
                    Column(i % screen.columns),
                )] = cell;
            }
            let mut colors = Colors::default();
            for (i, color) in screen.colors.into_iter().enumerate() {
                colors[i] = color;
            }
            (grid, colors)
        } else {
            if messages.is_empty() {
                return None;
            }
            let mut term = Term::new(Config::default(), &size, Silent);
            let mut parser: Processor<StdSyncHandler> = Processor::new();
            for message in messages {
                // Rollouts are text, not terminal programs. In particular an
                // ESC in a tool's quoted output must never become an OSC.
                let text: String = message
                    .chars()
                    .map(|c| {
                        if c == '\n' || c == '\t' || !c.is_control() {
                            c
                        } else {
                            ' '
                        }
                    })
                    .collect();
                parser.advance(&mut term, text.replace('\n', "\r\n").as_bytes());
                parser.advance(&mut term, b"\r\n\r\n");
            }
            (term.grid().clone(), *term.colors())
        };
        Some(Preview {
            grid,
            colors,
            marker,
            began: std::cell::Cell::new(None),
        })
    }

    pub fn save<T: EventListener>(&self, term: &Term<T>) -> std::io::Result<()> {
        let Some(conversation) = &self.conversation else {
            return Ok(());
        };
        let grid = term.grid();
        let screen = Screen {
            conversation: conversation.clone(),
            columns: grid.columns(),
            rows: grid.screen_lines(),
            cells: grid
                .display_iter()
                .map(|indexed| indexed.cell.clone())
                .collect(),
            colors: (0..COUNT).map(|i| term.colors()[i]).collect(),
        };
        let bytes = serde_json::to_vec(&screen)?;
        if bytes.len() as u64 > LIMIT {
            return Ok(());
        }
        std::fs::create_dir_all(self.path.parent().unwrap())?;
        let draft = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&draft)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&draft, &self.path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(draft);
        }
        result
    }
}

pub(super) struct Preview {
    pub grid: Grid<Cell>,
    pub colors: Colors,
    marker: String,
    began: std::cell::Cell<Option<Instant>>,
}

impl Preview {
    pub fn shown(&self, now: Instant) {
        if self.began.get().is_none() {
            self.began.set(Some(now));
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.began.get().map(|began| began + DEADLINE)
    }

    pub fn expired(&self, now: Instant) -> bool {
        self.began
            .get()
            .is_some_and(|began| now.saturating_duration_since(began) >= DEADLINE)
    }

    pub fn ready<T: EventListener>(&self, term: &Term<T>, now: Instant, exited: bool) -> bool {
        if exited || self.expired(now) {
            return true;
        }
        if self.marker.is_empty() {
            return false;
        }
        let grid = term.grid();
        // The most recent reply is near the bottom. Bound work even when
        // the process has replayed thousands of scrollback lines.
        let first = -(grid.history_size().min(200) as i32);
        let mut text = String::new();
        for row in first..grid.screen_lines() as i32 {
            for column in 0..grid.columns() {
                let c = grid[Point::new(Line(row), Column(column))].c;
                if c.is_alphanumeric() {
                    text.push(c);
                }
            }
        }
        text.contains(&self.marker)
    }
}

fn fingerprint(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .rev()
        .take(96)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn messages(path: &Path) -> Vec<String> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut kept = std::collections::VecDeque::new();
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let p = &v["payload"];
        if v["type"] != "response_item"
            || p["type"] != "message"
            || !matches!(p["role"].as_str(), Some("user" | "assistant"))
        {
            continue;
        }
        let Some(parts) = p["content"].as_array() else {
            continue;
        };
        let text = parts
            .iter()
            .filter(|p| matches!(p["type"].as_str(), Some("input_text" | "output_text")))
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            continue;
        }
        // Codex stores launch context as role-user messages too; it is
        // absent from the TUI's discussion and from this preview.
        if p["role"] == "user"
            && [
                "<environment_context>",
                "<permissions instructions>",
                "<collaboration_mode>",
                "<user_instructions>",
                "<turn_aborted>",
                "# AGENTS.md instructions for ",
            ]
            .iter()
            .any(|prefix| text.trim_start().starts_with(prefix))
        {
            continue;
        }
        let text: String = text
            .chars()
            .rev()
            .take(32 * 1024)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        kept.push_back(text);
        if kept.len() > 8 {
            kept.pop_front();
        }
    }
    kept.into_iter().collect()
}

struct Silent;
impl EventListener for Silent {
    fn send_event(&self, _: Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    const OTHER: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    fn size() -> GridSize {
        GridSize { rows: 24, cols: 80 }
    }
    fn term(bytes: &[u8]) -> Term<Silent> {
        let mut term = Term::new(Config::default(), &size(), Silent);
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }
    fn rollout(home: &Path) {
        let dir = home.join("sessions/2026/10/05");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("rollout-today-{ID}.jsonl")), format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{ID}\"}}}}\n{}\n{}\n",
            serde_json::json!({"type":"response_item", "payload":{"type":"message", "role":"user", "content":[{"type":"input_text", "text":"<environment_context>Injected environment</environment_context>"}]}}),
            serde_json::json!({"type":"response_item", "payload":{"type":"message", "role":"assistant", "content":[{"type":"output_text", "text":"Previous discussion **recovered**."}]}})
        )).unwrap();
    }
    #[test]
    fn history_is_visible_before_live_replay_and_header_does_not_release_it() {
        let dir = tempfile::tempdir().unwrap();
        rollout(dir.path());
        let cache = Cache::new(dir.path(), ID, Some(ID)).unwrap();
        let now = Instant::now();
        let preview = cache.preview(dir.path(), size()).unwrap();
        assert!(
            !preview.expired(now + DEADLINE),
            "an unpainted background tab has no startup deadline yet"
        );
        assert_eq!(preview.deadline(), None);
        preview.shown(now);
        assert_eq!(preview.deadline(), Some(now + DEADLINE));
        assert!(preview.grid.display_iter().any(|c| c.cell.c == 'P'));
        let drawn: String = preview.grid.display_iter().map(|c| c.cell.c).collect();
        assert!(!drawn.contains("environment_context"));
        assert!(!preview.ready(&term(b""), now, false));
        assert!(!preview.ready(
            &term(b"Codex header\r\n>"),
            now + Duration::from_secs(2),
            false
        ));
        assert!(preview.ready(&term(b"Previous discussion recovered.\r\n>"), now, false));
        assert!(preview.ready(&term(b"error"), now, true));
        assert!(preview.ready(&term(b"trust this folder?"), now + DEADLINE, false));
    }
    #[test]
    fn a_screen_round_trip_keeps_styled_cells_but_never_crosses_conversations() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path(), ID, Some(ID)).unwrap();
        let source = term(
            "\x1b]4;1;rgb:12/34/56\x1b\\\x1b[31;1mPrevious discussion recovered. 日本語\x1b[0m"
                .as_bytes(),
        );
        cache.save(&source).unwrap();
        let preview = cache.preview(dir.path(), size()).unwrap();
        let before: Vec<_> = source
            .grid()
            .display_iter()
            .map(|c| c.cell.clone())
            .collect();
        let after: Vec<_> = preview
            .grid
            .display_iter()
            .map(|c| c.cell.clone())
            .collect();
        assert_eq!(before, after);
        assert_eq!(preview.colors[1], source.colors()[1]);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&cache.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let other = Cache::new(dir.path(), ID, Some(OTHER)).unwrap();
        assert!(other.preview(dir.path(), size()).is_none());
        assert!(Cache::new(dir.path(), "../../outside", Some(ID)).is_none());
        std::fs::write(&cache.path, "broken cache").unwrap();
        assert!(cache.preview(dir.path(), size()).is_none());
        rollout(dir.path());
        assert!(
            cache.preview(dir.path(), size()).is_some(),
            "a corrupt cache falls back to the exact rollout"
        );
    }

    #[test]
    fn restored_colors_follow_the_theme_except_for_program_overrides() {
        use crate::terminal::{paint, palette::Palette};
        use alacritty_terminal::term::cell::Flags;
        use alacritty_terminal::vte::ansi::{Color, NamedColor};

        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path(), ID, Some(ID)).unwrap();
        let overrides = b"\x1b]4;1;rgb:12/34/56\x1b\\\x1b]10;rgb:de/bc/9a\x1b\\\x1b]11;rgb:23/45/67\x1b\\\x1b]12;rgb:ab/cd/ef\x1b\\";
        let resets = b"\x1b]104\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\";
        let mut previous = None;
        for controls in [b"".as_slice(), overrides.as_slice(), resets.as_slice()] {
            let mut source = term(b"");
            let mut parser: Processor<StdSyncHandler> = Processor::new();
            parser.advance(&mut source, overrides);
            if controls.is_empty() {
                parser.advance(&mut source, resets);
            } else {
                parser.advance(&mut source, controls);
            }
            parser.advance(&mut source, b"N\x1b[2mD\x1b[7mI\x1b[0;31mA\x1b[38;5;67mX\x1b[38;2;11;22;33;48;2;44;55;66mR\x1b[0;2;4;58;2;77;88;99mU\x1b[0m");
            cache.save(&source).unwrap();
            let preview = cache.preview(dir.path(), size()).unwrap();
            assert_eq!(
                preview.grid[Point::new(Line(0), Column(1))].flags,
                Flags::DIM
            );
            assert_eq!(
                preview.grid[Point::new(Line(0), Column(2))].flags,
                Flags::DIM | Flags::INVERSE
            );
            for theme in crate::theme::Theme::all() {
                let palette = Palette::new(theme);
                let rendered = |grid: &Grid<Cell>, colors: &Colors| {
                    paint::runs_of_row(
                        (0..7).map(|column| {
                            (
                                &grid[Point::new(Line(0), Column(column))],
                                paint::Mark::default(),
                            )
                        }),
                        &palette,
                        colors,
                    )
                };
                let restored = rendered(&preview.grid, &preview.colors);
                assert_eq!(restored, rendered(source.grid(), source.colors()));
                let foreground =
                    palette.resolve(Color::Named(NamedColor::Foreground), &preview.colors);
                let background =
                    palette.resolve(Color::Named(NamedColor::Background), &preview.colors);
                let dimmed = crate::theme::blend(foreground, background, crate::theme::DIM_KEEP);
                assert_eq!(
                    restored[1].foreground, dimmed,
                    "DIM must use the effective terminal background under {:?}",
                    theme.kind
                );
                assert_eq!(restored[2].foreground, background);
                assert_eq!(restored[2].background, Some(dimmed));
                assert_eq!(
                    palette.resolve(Color::Named(NamedColor::DimForeground), &preview.colors),
                    dimmed
                );
                assert_eq!(
                    restored[6].underline_color,
                    Some(crate::theme::blend(
                        gpui::rgb(0x4d5863).into(),
                        background,
                        crate::theme::DIM_KEEP
                    ))
                );
                assert_eq!(
                    palette.answer_colour(258, &preview.colors),
                    source.colors()[258]
                        .unwrap_or_else(|| palette.answer_colour(258, &Colors::default()))
                );
                if controls == overrides {
                    assert_eq!(foreground, gpui::rgb(0xdebc9a).into());
                    assert_eq!(background, gpui::rgb(0x234567).into());
                    assert_eq!(restored[3].foreground, gpui::rgb(0x123456).into());
                } else {
                    assert_eq!(foreground, gpui::rgb(theme.text.primary).into());
                    assert_eq!(background, palette.background());
                    assert_eq!(
                        restored[3].foreground,
                        palette.resolve(Color::Named(NamedColor::Red), &Colors::default())
                    );
                }
                // Literal truecolor output stays literal after a theme switch.
                assert_eq!(restored[5].foreground, gpui::rgb(0x0b1621).into());
                assert_eq!(restored[5].background, Some(gpui::rgb(0x2c3742).into()));
            }
            if controls == resets {
                assert!(previous.is_some_and(|colors: Colors| colors[257].is_some()));
                assert!(
                    (0..COUNT).all(|i| preview.colors[i].is_none()),
                    "OSC resets must not inherit the previous screen's overrides"
                );
            }
            previous = Some(preview.colors);
        }
    }
}
