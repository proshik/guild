//! Group qualification and body transfer; placement stays in root::areas.
use super::*;
use crate::i18n::Text;
type MoveGroup = (SessionId, HashMap<TabId, Vec<PaneId>>);
impl Root {
    pub(in crate::root) fn process_move_group(
        &self,
        tab: TabId,
    ) -> Result<Option<MoveGroup>, Text> {
        let Some(origin) = self.process_move_origin(tab)? else {
            return Ok(None);
        };
        let seat = self
            .tabs
            .iter()
            .find(|t| t.id == tab)
            .expect("qualified tab")
            .seat;
        let group = self.process_group_panes(origin, seat)?;
        if self
            .tabs
            .iter()
            .any(|t| group.contains_key(&t.id) && t.seat != seat)
        {
            return Err(Text::ProcessMoveSeat);
        }
        Ok(Some((origin, group)))
    }
    fn process_move_origin(&self, tab: TabId) -> Result<Option<SessionId>, Text> {
        let Some(tab) = self.tabs.iter().find(|t| t.id == tab) else {
            return Ok(None);
        };
        // Until the first legacy inspection reply, an un-attributed view
        // could belong to this origin even if another member is already loaded.
        // Missing/orphan replies release this temporary guard; other seats and
        // ordinary non-session bodies do not wait for these observations.
        let pending = self
            .tabs
            .iter()
            .filter(|t| t.seat == tab.seat)
            .flat_map(crate::Tab::bodies)
            .any(|body| match body {
                Body::Process { run } => self.processes.runs.get(run).is_some_and(|s| {
                    s.value.is_none() && s.origin_hint.is_none() && !s.reply_received
                }),
                _ => false,
            });
        if pending
            && tab
                .bodies()
                .any(|b| b.session().is_some() || matches!(b, Body::Process { .. }))
        {
            return Err(Text::ProcessMoveUnavailable);
        }
        let open_snapshots = || {
            self.tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .filter_map(|body| match body {
                    Body::Process { run } => self.processes.runs.get(run),
                    _ => None,
                })
        };
        let origin = |body: &Body| -> Result<Option<SessionId>, Text> {
            if let Body::Process { run } = body {
                return self
                    .processes
                    .runs
                    .get(run)
                    .and_then(|s| s.value.as_ref())
                    .and_then(|s| s["origin"]["session"].as_str())
                    .and_then(|s| s.parse().ok())
                    .map(Some)
                    .ok_or(Text::ProcessMoveUnavailable);
            }
            Ok(body.session().filter(|session| {
                open_snapshots()
                    .filter_map(|s| s.value.as_ref())
                    .any(|s| s["origin"]["session"] == session.to_string())
            }))
        };
        if let Some(body) = tab.panes.body()
            && let Some(owner) = origin(body)?
        {
            return Ok(Some(owner));
        }
        let mut candidates = HashSet::new();
        for body in tab.bodies() {
            if let Some(owner) = origin(body)? {
                candidates.insert(owner);
            }
        }
        match candidates.len() {
            0 if tab.sessions().any(|session| {
                open_snapshots().any(|s| s.value.is_none() && s.origin_hint == Some(session))
            }) =>
            {
                Err(Text::ProcessMoveUnavailable)
            }
            0 => Ok(None),
            1 => Ok(candidates.into_iter().next()),
            _ => Err(Text::ProcessMoveSelectPane),
        }
    }
    fn process_group_panes(
        &self,
        origin: SessionId,
        seat: Seat,
    ) -> Result<HashMap<TabId, Vec<PaneId>>, Text> {
        let mut group = HashMap::new();
        for tab in &self.tabs {
            let mut panes = vec![];
            for (pane, body) in tab.panes.bodies() {
                let matches = match body {
                    Body::Process { run } => self
                        .processes
                        .runs
                        .get(run)
                        .and_then(|s| s.value.as_ref())
                        .and_then(|s| s["origin"]["session"].as_str())
                        .map(|s| s == origin.to_string())
                        .or_else(|| {
                            let hint = self.processes.runs.get(run).and_then(|s| s.origin_hint);
                            (tab.seat != seat || hint != Some(origin)).then_some(false)
                        })
                        .ok_or(Text::ProcessMoveUnavailable)?,
                    _ => body.session() == Some(origin),
                };
                if matches {
                    panes.push(pane);
                }
            }
            if !panes.is_empty() {
                group.insert(tab.id, panes);
            }
        }
        Ok(group)
    }
    /// Split only mixed containers. No close hook or terminal teardown runs:
    /// the same bodies and pane identities transfer through the one adopter.
    pub(in crate::root) fn detach_process_group(
        &mut self,
        group: &HashMap<TabId, Vec<PaneId>>,
        dragged: TabId,
        cx: &mut Context<Self>,
    ) -> (TabId, Vec<TabId>) {
        let original: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| group.contains_key(&t.id))
            .map(|t| t.id)
            .collect();
        let mut active = dragged;
        let mut tabs = vec![];
        for id in original {
            let t = self.tabs.iter_mut().find(|t| t.id == id).unwrap();
            let (seat, project, area) = (t.seat, t.project, t.area);
            let moved = if group[&id].len() < t.panes.showing() {
                let panes = t.panes.extract(&group[&id]).expect("qualified mixed group");
                let new = self.adopt_panes(seat, project, panes, cx);
                self.tabs.iter_mut().find(|t| t.id == new).unwrap().area = area;
                new
            } else {
                id
            };
            if id == dragged {
                active = moved;
            }
            tabs.push(moved);
        }
        (active, tabs)
    }
}
