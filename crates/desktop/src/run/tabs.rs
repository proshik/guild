//! Runtime invocation identity is independent of selection and terminal lifetime.
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub cwd: PathBuf,
    pub line: String,
    pub task: Option<super::source::Identity>,
}
impl Invocation {
    /// Source captions use the captured executable and declaration, while the
    /// full command remains the invocation identity and diagnostic evidence.
    pub fn title(&self) -> String {
        let Some(task) = &self.task else {
            return self.line.clone();
        };
        let Ok(argv) = super::source::tokens(&self.line) else {
            return self.line.clone();
        };
        let Some(program) = argv.first() else {
            return self.line.clone();
        };
        if task.name.is_empty() {
            program.clone()
        } else {
            format!("{program} {}", task.name)
        }
    }

    pub fn details(&self) -> String {
        match &self.task {
            Some(task) => format!(
                "{}\n{}\n{}",
                task.source.display(),
                self.cwd.display(),
                self.line
            ),
            None => format!("{}\n{}", self.cwd.display(), self.line),
        }
    }
}
pub struct Entry<T> {
    pub id: u64,
    pub invocation: Invocation,
    pub value: T,
}
pub struct RunTabs<T> {
    entries: Vec<Entry<T>>,
    active: Option<u64>,
    next: u64,
}
impl<T> Default for RunTabs<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            active: None,
            next: 1,
        }
    }
}
impl<T> RunTabs<T> {
    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }
    pub fn entries_mut(&mut self) -> &mut [Entry<T>] {
        &mut self.entries
    }
    pub fn get(&self, id: u64) -> Option<&Entry<T>> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn active(&self) -> Option<&Entry<T>> {
        self.active.and_then(|id| self.get(id))
    }
    pub fn active_mut(&mut self) -> Option<&mut Entry<T>> {
        let id = self.active?;
        self.entries.iter_mut().find(|e| e.id == id)
    }
    pub fn select(&mut self, id: u64) {
        if self.get(id).is_some() {
            self.active = Some(id);
        }
    }
    pub fn find(&self, invocation: &Invocation) -> Option<&Entry<T>> {
        self.entries.iter().find(|e| &e.invocation == invocation)
    }
    /// The caller stops the old owner before constructing its replacement.
    pub fn insert(&mut self, invocation: Invocation, value: T) -> u64 {
        let id = if let Some(entry) = self.entries.iter_mut().find(|e| e.invocation == invocation) {
            entry.value = value;
            entry.id
        } else {
            let id = self.next;
            self.next += 1;
            self.entries.push(Entry {
                id,
                invocation,
                value,
            });
            id
        };
        self.active = Some(id);
        id
    }
    pub fn remove(&mut self, id: u64) -> Option<Entry<T>> {
        let at = self.entries.iter().position(|e| e.id == id)?;
        let removed = self.entries.remove(at);
        if self.active == Some(id) {
            self.active = self
                .entries
                .get(at.saturating_sub(1))
                .or_else(|| self.entries.first())
                .map(|e| e.id);
        }
        Some(removed)
    }
}
