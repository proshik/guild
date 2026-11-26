//! The five slots `gpui-component` leaves for a language server, filled.
//!
//! M8.5's decision 1: the component draws — the hover popover, the
//! completion menu, the underline under a definition — and we speak. Each
//! of these is one request to the server of the file's language and one
//! answer turned into the type the component asked for.
//!
//! **Every ask has a deadline.** A popup that waits for ever on a server
//! that will not answer is worse than one that says nothing: it holds the
//! keyboard's attention on a menu that will never open. Task 0 measured
//! hover at 2 ms and a completion at 588 ms with 355 KB of JSON, so the
//! deadline is seconds — long enough for the slowest real answer, short
//! enough to be over before a person tries again.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, anyhow};
use gpui::{App, AppContext as _, Task, WeakEntity, Window};
use gpui_component::input::{CompletionProvider, DefinitionProvider, HoverProvider, Rope};
use guild_session::ProjectId;
use lsp_types::{CompletionContext, CompletionResponse, Hover, LocationLink, Position};
use serde_json::{Value, json};

use crate::Root;

/// How long an ask may take before it is given up on.
const DEADLINE: Duration = Duration::from_secs(5);

/// How often the answer is looked for. A frame: the answer arrives on
/// the reader thread and there is nothing to be gained by asking faster
/// than the window redraws.
const LOOK: Duration = Duration::from_millis(16);

/// One file's line to a language server.
pub struct Provider {
    root: WeakEntity<Root>,
    project: ProjectId,
    /// The file, relative to the project — the same key everything else
    /// in the window uses.
    path: PathBuf,
    /// Whose server answers.
    language: &'static str,
}

impl Provider {
    /// The provider for a file, if its language has a server at all.
    pub fn of(
        root: WeakEntity<Root>,
        project: ProjectId,
        path: PathBuf,
        language: &'static str,
    ) -> Self {
        Self {
            root,
            project,
            path,
            language,
        }
    }

    /// Ask, wait for the answer, and give up when the deadline passes or
    /// the server goes.
    fn ask(&self, method: &'static str, params: Value, cx: &mut App) -> Task<Result<Value>> {
        let (root, project, language) = (self.root.clone(), self.project, self.language);
        let path = self.path.clone();
        let asked = root.update(cx, |root, _cx| {
            root.ask_server(project, language, method, params)
        });
        cx.spawn(async move |cx| {
            let Ok(Some(id)) = asked else {
                return Err(anyhow!("no server for {language}"));
            };
            let until = std::time::Instant::now() + DEADLINE;
            loop {
                let answer = cx.update(|cx| {
                    root.update(cx, |root, _cx| {
                        (
                            root.server_answer(project, language, &id),
                            root.server_alive(project, language)
                                && root.documents.contains_key(&(project, path.clone())),
                        )
                    })
                });
                match answer {
                    Ok((Some(Ok(result)), true)) => return Ok(result),
                    Ok((Some(Err(refused)), _)) => return Err(anyhow!(refused)),
                    // The server has gone, and the answer with it.
                    Ok((_, false)) => {
                        return Err(anyhow!("the document or {language}'s server has closed"));
                    }
                    Ok((None, true)) => {}
                    // The window has gone: there is nobody to answer to.
                    _ => return Err(anyhow!("the window is closing")),
                }
                if std::time::Instant::now() > until {
                    return Err(anyhow!("{language}'s server did not answer {method}"));
                }
                cx.background_executor().timer(LOOK).await;
            }
        })
    }

    /// What a request at a place in a file looks like.
    fn at(&self, position: Position, cx: &mut App) -> Option<Value> {
        let uri = self
            .root
            .update(cx, |root, _cx| root.uri_of(self.project, &self.path))
            .ok()
            .flatten()?;
        Some(json!({
            "textDocument": {"uri": uri},
            "position": position,
        }))
    }
}

impl HoverProvider for Provider {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Option<Hover>>> {
        let Some(params) = self.at(position_at(text, offset), cx) else {
            return Task::ready(Ok(None));
        };
        let asked = self.ask("textDocument/hover", params, cx);
        cx.background_spawn(async move {
            let answer = asked.await?;
            // `null` is an answer: there is nothing under the pointer.
            Ok(serde_json::from_value(answer).unwrap_or(None))
        })
    }
}

impl DefinitionProvider for Provider {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<LocationLink>>> {
        // **One copy of the rope for both countings**: the component
        // calls this on every `cmd`-move that reaches a new symbol, and
        // a large file copied twice per move is a copy for nothing.
        let (asked, here) = asked_and_held(&text.to_string(), offset);
        let Some(params) = self.at(asked, cx) else {
            return Task::ready(Ok(Vec::new()));
        };
        let asked = self.ask("textDocument/definition", params, cx);
        cx.background_spawn(async move { Ok(held_at_asked(links_in(asked.await?), here)) })
    }
}

impl CompletionProvider for Provider {
    fn is_completion_trigger(&self, _offset: usize, typed: &str, _cx: &mut App) -> bool {
        triggers(typed)
    }

    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let Some(mut params) = self.at(position_at(text, offset), cx) else {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        };
        if let Value::Object(map) = &mut params {
            map.insert(
                "context".to_owned(),
                serde_json::to_value(trigger).unwrap_or(Value::Null),
            );
        }
        let asked = self.ask("textDocument/completion", params, cx);
        cx.background_spawn(async move {
            let answer = asked.await?;
            Ok(serde_json::from_value(answer).unwrap_or(CompletionResponse::Array(Vec::new())))
        })
    }
}

/// What starts a completion.
///
/// **A letter, `.`, `::` and `->`, and nothing else.** The component asks
/// per keystroke and this decides; a rule that says yes to everything
/// makes a 588-millisecond request out of every space bar, which is the
/// number task 0 measured.
pub fn triggers(typed: &str) -> bool {
    typed.chars().last().is_some_and(|last| {
        last.is_alphanumeric() || last == '_' || last == '.' || last == ':' || last == '>'
    })
}

/// The links in a definition answer.
///
/// **Three shapes, and a server picks whichever it likes**: one
/// `Location`, an array of them, or an array of `LocationLink`. A client
/// that reads one of the three works until it meets a server that sends
/// another.
pub fn links_in(answer: Value) -> Vec<LocationLink> {
    if answer.is_null() {
        return Vec::new();
    }
    if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(answer.clone()) {
        return links;
    }
    let as_link = |location: lsp_types::Location| LocationLink {
        origin_selection_range: None,
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range,
    };
    if let Ok(locations) = serde_json::from_value::<Vec<lsp_types::Location>>(answer.clone()) {
        return locations.into_iter().map(as_link).collect();
    }
    serde_json::from_value::<lsp_types::Location>(answer)
        .map(|location| vec![as_link(location)])
        .unwrap_or_default()
}

/// Every definition link held at the place that was asked.
///
/// **The component's own jump is never where a definition is reached** —
/// M5.11's decision 10. Its `go_to_definition` moves the caret to a
/// link's `target_range` in the buffer it is in, whatever file the link
/// names, so a `cmd`-click on a name defined elsewhere put the caret on
/// that line of *this* file. Held here, its jump moves the caret where
/// the click already put it and selects nothing, and the file tab sends
/// Guild's own Go to Definition — `f12`'s path, which opens tabs.
///
/// **The same file's links are held too**: left alone the component
/// would move the caret to the target itself, Guild's jump would then
/// ask the server from the target, and Back after a `cmd`-click would
/// not return to the name clicked. `origin_selection_range` is left
/// alone because it is the underline under a `cmd`-hovered name, and
/// `target_uri` because the component reads it only to tell a web link.
pub fn held_at_asked(links: Vec<LocationLink>, asked: Position) -> Vec<LocationLink> {
    let here = lsp_types::Range::new(asked, asked);
    links
        .into_iter()
        .map(|link| LocationLink {
            target_range: here,
            target_selection_range: here,
            ..link
        })
        .collect()
}

/// Where a byte offset is, as the protocol counts.
///
/// The rope is copied into a string first, deliberately: `ropey`'s own
/// API is a pre-release that has renamed these calls once already, and
/// this crate does not depend on it — the type arrives through the
/// component's door. A completion's copy is tens of microseconds against
/// the 588 milliseconds task 0 measured for the answer.
pub fn position_at(text: &Rope, offset: usize) -> Position {
    position_in(&text.to_string(), offset)
}

/// The same, on a text — pure, and where the counting rule is checked.
///
/// **UTF-16 units, not characters and not bytes.** That is the protocol's
/// default and we negotiate nothing else, so a line with an emoji in it
/// would put every ask after that emoji one unit out — which reads as
/// "hover works except in that file".
pub fn position_in(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    Position::new(line as u32, utf16_len(&before[start..]))
}

/// The byte a protocol position names in a text — [`position_in`] turned
/// round, clamped to the line's end and to the text's. M8.15's balloon
/// finds the name it points at again with it.
pub fn offset_of(text: &str, at: Position) -> usize {
    let mut start = 0;
    for _ in 0..at.line {
        match text[start..].find('\n') {
            Some(newline) => start += newline + 1,
            None => return text.len(),
        }
    }
    let line = text[start..].split('\n').next().unwrap_or("");
    let mut units = 0;
    for (index, c) in line.char_indices() {
        if units >= at.character {
            return start + index;
        }
        units += c.len_utf16() as u32;
    }
    start + line.len()
}

/// Where a definition is asked, and where the component's caret is held,
/// from one offset.
///
/// **Two countings of one place**: the first is the protocol's, in UTF-16
/// units, and goes to the server; the second is the component's, in
/// characters, and is what [`held_at_asked`] writes into every link —
/// the component reads a link's range with `position_to_offset`, which
/// counts characters, and after an emoji a hold in UTF-16 units would move
/// the caret off the name before Guild's jump asked the server from it.
pub fn asked_and_held(text: &str, offset: usize) -> (Position, Position) {
    let asked = position_in(text, offset);
    let (line, character) = caret_for(text, asked);
    (asked, Position::new(line, character))
}

/// How many UTF-16 units a piece of text takes.
pub fn utf16_len(text: &str) -> u32 {
    text.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Where the **component** should put its caret for a place the protocol
/// named.
///
/// Two counting rules meet here: the protocol counts UTF-16 units and
/// `gpui-component`'s `Position` counts characters. They differ exactly
/// where an astral character is — an emoji in a comment — and the symptom
/// would be a jump that lands a column short in that one file.
pub fn caret_for(text: &str, at: Position) -> (u32, u32) {
    let line = text.lines().nth(at.line as usize).unwrap_or("");
    let mut units = 0;
    let mut characters = 0;
    for c in line.chars() {
        if units >= at.character {
            break;
        }
        units += c.len_utf16() as u32;
        characters += 1;
    }
    (at.line, characters)
}
