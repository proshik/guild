//! Stable runtime tabs for one project's Terminal tool. Hiding a view does not own its process.

pub struct Entry<T> {
    pub id: u64,
    pub value: T,
}

pub struct TerminalTabs<T> {
    entries: Vec<Entry<T>>,
    active: Option<u64>,
    next: u64,
}

impl<T> Default for TerminalTabs<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            active: None,
            next: 1,
        }
    }
}

impl<T> TerminalTabs<T> {
    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }
    pub fn get(&self, id: u64) -> Option<&Entry<T>> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn active(&self) -> Option<&Entry<T>> {
        self.active.and_then(|id| self.get(id))
    }
    pub fn add(&mut self, value: T) -> u64 {
        let id = self.next;
        self.next += 1;
        self.entries.push(Entry { id, value });
        self.active = Some(id);
        id
    }
    pub fn select(&mut self, id: u64) {
        if self.get(id).is_some() {
            self.active = Some(id);
        }
    }
    pub fn remove(&mut self, id: u64) -> Option<Entry<T>> {
        let at = self.entries.iter().position(|e| e.id == id)?;
        let removed = self.entries.remove(at);
        if self.active == Some(id) {
            self.active = self
                .entries
                .get(at)
                .or_else(|| self.entries.last())
                .map(|e| e.id);
        }
        Some(removed)
    }
}
