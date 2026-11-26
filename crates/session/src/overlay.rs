//! Reusable observation surfaces. Visibility never controls an underlying job.
use crate::{PaneId, SessionId};
use serde::{Deserialize, Serialize};

/// The place an overlay covers, never an unrelated split sibling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target {
    /// Follow a session across tab moves.
    Session(SessionId),
    /// An arbitrary native pane, including files and other tools.
    Pane(PaneId),
}
/// A renderer's data, not executable markup or a command to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    /// A retained terminal job owns the process independently of this view.
    Terminal {
        /// Stable program identity, never shell text.
        job: crate::terminal_job::JobId,
    },
    /// Plain emitted text, useful for progress, logs and results.
    Text {
        /// Rendered literally, never as HTML or a shell command.
        text: String,
    },
}
/// One owned surface. Closing it drops only this presentation record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overlay {
    /// Caller-chosen identity, stable for updates.
    pub id: String,
    /// Session or native controller owning the surface.
    pub owner: String,
    /// Pane/session the content belongs to.
    pub target: Target,
    /// Human-readable heading.
    pub title: String,
    /// Native renderer data.
    pub content: Content,
    /// Whether the surface is expanded.
    pub visible: bool,
}
/// In-memory presentation state. Durable results belong to their producer.
#[derive(Debug, Default)]
pub struct Board {
    entries: Vec<Overlay>,
}
impl Board {
    /// Open or update an owned surface. Updates preserve its hidden state.
    pub fn open(
        &mut self,
        id: &str,
        owner: &str,
        target: Target,
        title: &str,
        content: Content,
    ) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 || title.len() > 512 {
            return Err("Overlay ID or title is invalid".into());
        }
        if let Content::Text { text } = &content
            && text.len() > 1024 * 1024
        {
            return Err("Overlay text exceeds 1 MiB".into());
        }
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == id) {
            if existing.owner != owner || existing.target != target {
                return Err("Overlay belongs to another owner or pane".into());
            }
            if !same_channel(&existing.content, &content) {
                return Err("Overlay content channel cannot be replaced".into());
            }
            existing.title = title.into();
            existing.content = content;
            return Ok(());
        }
        if self.entries.len() >= 64 {
            return Err("Close unused overlays before opening more".into());
        }
        for e in &mut self.entries {
            if e.target == target && same_channel(&e.content, &content) {
                e.visible = false;
            }
        }
        self.entries.push(Overlay {
            id: id.into(),
            owner: owner.into(),
            target,
            title: title.into(),
            content,
            visible: true,
        });
        Ok(())
    }
    fn owned(&mut self, id: &str, owner: &str) -> Result<&mut Overlay, String> {
        self.entries
            .iter_mut()
            .find(|e| e.id == id && e.owner == owner)
            .ok_or_else(|| "No overlay owned by this caller has that ID".into())
    }
    /// Show explicitly, hiding another surface only in the same content channel.
    pub fn show(&mut self, id: &str, owner: &str) -> Result<(), String> {
        let entry = self.owned(id, owner)?;
        let target = entry.target.clone();
        let content = entry.content.clone();
        for e in &mut self.entries {
            if e.target == target && same_channel(&e.content, &content) {
                e.visible = e.id == id;
            }
        }
        Ok(())
    }
    /// Minimize observation; the owner is responsible for its job's lifecycle.
    pub fn hide(&mut self, id: &str, owner: &str) -> Result<(), String> {
        self.owned(id, owner)?.visible = false;
        Ok(())
    }
    /// Remove a presentation record, without signalling its producer.
    pub fn close(&mut self, id: &str, owner: &str) -> Result<(), String> {
        self.owned(id, owner)?;
        self.entries.retain(|e| e.id != id);
        Ok(())
    }
    /// Drop presentation belonging to sessions no longer present in the window.
    pub fn prune_sessions(&mut self, live: &[SessionId]) {
        self.entries.retain(|entry| match entry.target {
            Target::Session(id) => live.contains(&id),
            Target::Pane(_) => true,
        });
    }
    /// Presentation lookup for native focus cleanup, without changing ownership.
    pub fn get(&self, id: &str) -> Option<&Overlay> {
        self.entries.iter().find(|entry| entry.id == id)
    }
    /// Expanded surface for one target.
    pub fn visible(&self, target: &Target) -> Option<&Overlay> {
        self.entries
            .iter()
            .find(|e| &e.target == target && e.visible)
    }
    /// Owned records, including minimized surfaces.
    pub fn list(&self, owner: &str) -> Vec<Overlay> {
        self.entries
            .iter()
            .filter(|e| e.owner == owner)
            .cloned()
            .collect()
    }
    /// All presentations at a target, for its native reopen controls.
    pub fn at(&self, target: &Target) -> Vec<Overlay> {
        self.entries
            .iter()
            .filter(|e| &e.target == target)
            .cloned()
            .collect()
    }
}

fn same_channel(a: &Content, b: &Content) -> bool {
    matches!(
        (a, b),
        (Content::Text { .. }, Content::Text { .. })
            | (Content::Terminal { .. }, Content::Terminal { .. })
    )
}
