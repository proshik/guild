//! Root owns one serialized workflow queue. Disk IO never runs on the UI thread.
mod correction;
mod handoff;
mod native;
mod recovery;
mod source;
use store::assignment::{Observed, Reserved};
mod store;
use crate::Root;
use gpui::Context;
use guild_session::{SessionId, command::Answer, workflow::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

fn workflow_cli_program() -> Option<std::path::PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("GUILD_WORKFLOW_FIXTURE_CLI") {
        return Some(path.into());
    }
    crate::paths::guild_dir().map(|p| p.join("guild"))
}
struct Job {
    /// Admission snapshot: toggling later does not revoke an already queued intent.
    entry_enabled: bool,
    trusted_navigation: bool,
    request: WorkflowCommand,
    presentation: crate::root::process::Presentation,
    caller: Option<SessionId>,
    origin: Option<OriginBinding>,
    roots: Vec<std::path::PathBuf>,
    review_host: Option<RevustHost>,
    recovery_records: Vec<guild_session::SessionRecord>,
    caller_pid: Option<u32>,
    answer: mpsc::Sender<Answer>,
}
enum Work {
    CorrectionPermitted {
        previous: Box<Completed>,
    },
    CorrectionSubmitted {
        previous: Box<Completed>,
        stage: String,
    },
    CorrectionObserved {
        effect: Box<store::correction::Pending>,
        observed: bool,
    },
    HandoffPermitted {
        previous: Box<Completed>,
    },
    HandoffSubmitted {
        previous: Box<Completed>,
        stage: String,
    },
    HandoffObserved {
        effect: Box<store::handoff::Reserved>,
        observed: bool,
    },
    Request(Box<Job>),
    Submitted(Box<OriginBinding>),
    ReviewRefresh(RunId),
    ReviewJob(Box<store::review::ReviewJob>),
    ReviewPermitted {
        previous: Box<Completed>,
        permission: Result<(), String>,
    },
    StartupObserved {
        effect: Box<store::recovery::Reserved>,
        observed: bool,
    },
    RecoveryInspected {
        previous: Box<Completed>,
        inspection: Result<bool, String>,
    },
    RecoveryObserved {
        previous: Box<Completed>,
        observation: serde_json::Value,
    },
    Observed {
        previous: Box<Completed>,
        observed: Observed,
    },
}
enum Effect {
    Correction(Box<store::correction::Pending>),
    Handoff(Box<store::handoff::Reserved>),
    Assignment(Box<Reserved>),
    Review(Box<store::review::PendingReview>),
    Recovery(Box<store::recovery::Reserved>),
}
impl Effect {
    fn run(&self) -> RunId {
        match self {
            Self::Handoff(e) => e.run,
            Self::Correction(e) => e.run,
            Self::Assignment(e) => e.plan.run,
            Self::Review(e) => e.run,
            Self::Recovery(e) => e.run,
        }
    }
}
struct Completed {
    reply: WorkflowReply,
    answer: mpsc::Sender<Answer>,
    effect: Option<Effect>,
    answered: bool,
    presentation: crate::root::process::Presentation,
}
struct PendingNative {
    previous: Completed,
    receive: native::Completion,
}
/// Root-owned channel lifetime releases the worker and all kernel run claims.
pub(crate) struct Queue {
    send: mpsc::SyncSender<Work>,
    observations: store::reconciliation::Observations,
    prompts: store::handoff::Prompts,
    startups: Vec<(store::recovery::Reserved, u32, mpsc::Receiver<Answer>)>,
    startup_outcomes: std::collections::VecDeque<(store::recovery::Reserved, bool)>,
    corrections: Vec<(store::correction::Pending, mpsc::Receiver<Answer>)>,
    correction_outcomes: std::collections::VecDeque<(store::correction::Pending, bool)>,
    handoffs: Vec<(store::handoff::Reserved, mpsc::Receiver<Answer>)>,
    handoff_outcomes: std::collections::VecDeque<(store::handoff::Reserved, bool)>,
    native: Option<PendingNative>,
    children: std::collections::HashSet<SessionId>,
    review_intents: std::collections::HashMap<std::path::PathBuf, store::review::PendingReview>,
    review_jobs:
        std::collections::HashMap<guild_session::terminal_job::JobId, store::review::ReviewJob>,
    review_observations: std::collections::VecDeque<store::review::ReviewJob>,
    submissions: std::collections::HashMap<SessionId, OriginBinding>,
    submission_sent: std::collections::HashSet<SessionId>,
    waiting: std::collections::VecDeque<Job>,
    in_flight: bool,
    last_review_poll: std::time::Instant,
    last_review_run: Option<RunId>,
    settled_reviews: std::collections::HashSet<RunId>,
    completed: mpsc::Receiver<Completed>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Queue {
    fn new(global_skills: Vec<std::path::PathBuf>) -> Result<Self, String> {
        let (send, receive) = mpsc::sync_channel::<Work>(64);
        let (completed, replies) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let observations = store::reconciliation::Observations::default();
        let native_observations = observations.clone();
        let prompts = store::handoff::Prompts::default();
        let native_prompts = prompts.clone();
        let worker = std::thread::Builder::new()
            .name("guild-workflow".into())
            .spawn(move || {
                let mut store = store::Runs::with_global_skills(global_skills);
                store.observations = native_observations;
                store.prompts = native_prompts;
                while let Ok(work) = receive.recv() {
                    if stopping.load(Ordering::Acquire) {
                        break;
                    }
                    let (mut completion, admit) = match work {
                        Work::CorrectionPermitted {mut previous}=>{
                            if let Some(Effect::Correction(e))=previous.effect.as_mut() {
                                match store.correction_preflight(e) {
                                    Ok(())=>e.verified=true,
                                    Err(reason)=>{tracing::warn!(%reason,"correction positively refused before input");let run=e.run;let _=store.correction_delivery(e,"refused");previous.effect=None;previous.reply=store.execute(&WorkflowCommand::Status {version:1,run},None,None,&[]);}
                                }
                            }
                            (*previous,false)
                        }
                        Work::CorrectionSubmitted {mut previous,stage}=>{
                            if let Some(Effect::Correction(e))=previous.effect.as_ref() {
                                let run=e.run;if let Err(error)=store.correction_delivery(e,&stage){tracing::error!(%error,"correction receipt unknown; no replay");}
                                previous.effect=None;previous.reply=store.execute(&WorkflowCommand::Status {version:1,run},None,None,&[]);
                            }
                            (*previous,false)
                        }
                        Work::CorrectionObserved {effect,observed}=>{
                            if let Err(error)=store.correction_delivery(&effect,if observed {"observed"} else {"unknown"}) {tracing::error!(%error,"correction turn witness not retained; no replay");}
                            let reply=store.execute(&WorkflowCommand::Status {version:1,run:effect.run},None,None,&[]);let (answer,_)=mpsc::channel();
                            (Completed {reply,answer,effect:None,answered:true,presentation:crate::root::process::Presentation::None},false)
                        }
                        Work::HandoffPermitted {mut previous} => {
                            if let Some(Effect::Handoff(effect))=previous.effect.as_mut() {
                                match store.handoff_preflight(effect) {
                                    Ok(())=>effect.verified=true,
                                    Err(reason)=>{
                                        let run=effect.run;
                                        let _=store.observe_handoff(effect,"refused",&reason);
                                        previous.effect=None;
                                        previous.reply=store.execute(&WorkflowCommand::Status {version:1,run},None,None,&[]);
                                    }
                                }
                            }
                            (*previous,false)
                        }
                        Work::HandoffSubmitted {mut previous,stage} => {
                            if let Some(Effect::Handoff(effect))=previous.effect.as_ref()
                                && let Err(error)=store.observe_handoff(effect,&stage,if stage=="submitted" {"native_input_submitted"} else {"positive_pre_send_refusal"}) {tracing::error!(%error,"handoff receipt unknown");}
                            let run=previous.effect.as_ref().map(Effect::run).unwrap();
                            previous.effect=None;
                            previous.reply=store.execute(&WorkflowCommand::Status {version:1,run},None,None,&[]);
                            (*previous,false)
                        }
                        Work::HandoffObserved {effect,observed} => {
                            if let Err(error)=store.settle_handoff(&effect,observed) {tracing::error!(%error,"handoff observation not retained; no replay");}
                            let reply=store.execute(&WorkflowCommand::Status {version:1,run:effect.run},None,None,&[]);
                            let (answer,_)=mpsc::channel();
                            (Completed {reply,answer,effect:None,answered:true,presentation:crate::root::process::Presentation::None},false)
                        }

                        Work::Request(job) => {
                            store.entry_enabled = job.entry_enabled;
                            store.trusted_navigation = job.trusted_navigation;
                            store.review_host = job.review_host;
                            store.recovery_records = job.recovery_records;
                            store.recovery_caller_pid = job.caller_pid;
                            let reply =
                                store.execute(&job.request, job.caller, job.origin, &job.roots);
                            let correction_effect=store.correction_effect.take().map(|e|Effect::Correction(Box::new(e)));
                            let review_effect = store
                                .review_effect
                                .take()
                                .map(|e| Effect::Review(Box::new(e)))
                                .or_else(|| {
                                    store
                                        .retry_effect
                                        .take()
                                        .map(|e| Effect::Assignment(Box::new(e)))
                                })
                                .or_else(|| {
                                    store
                                        .recovery_effect
                                        .take()
                                        .map(|e| Effect::Recovery(Box::new(e)))
                                });
                            let review_effect = review_effect.map(|mut effect| {
                                if let Effect::Recovery(r) = &mut effect {
                                    r.request_epoch = match job.presentation {
                                        crate::root::process::Presentation::Recover(epoch) => epoch,
                                        _ => 0,
                                    };
                                }
                                effect
                            });
                            let admit = store.take_effect_admission();
                            (
                                Completed {
                                    reply,
                                    answer: job.answer,
                                    effect: correction_effect.or(review_effect),
                                    answered: false,
                                    presentation: job.presentation,
                                },
                                admit,
                            )
                        }
                        Work::StartupObserved {effect,observed}=>{
                            let reply=match store.settle_startup(&effect,observed) {
                                Ok(())=>store.execute(&WorkflowCommand::Show {version:1,run:effect.run},None,None,&[]),
                                Err(message)=>WorkflowReply::Gap {run:Some(effect.run),revision:None,code:"workflow.handoff".into(),message},
                            };
                            {let (answer,_)=mpsc::channel(); (Completed {reply,answer,effect:None,answered:true,presentation:crate::root::process::Presentation::None},false)}
                        }
                        Work::RecoveryInspected {
                            mut previous,
                            inspection,
                        } => {
                            if let Some(Effect::Recovery(effect)) = previous.effect.as_mut() {
                                effect.stopped_qualified = matches!(inspection, Ok(false));
                                effect.inspection = Some(
                                    inspection
                                        .and_then(|live| {
                                            if live {
                                                Ok(())
                                            } else {
                                                recovery::inspect_origin(&effect.binding)
                                            }
                                        })
                                        .and_then(|_| store::Runs::recovery_preflight(effect))
                                        .and_then(|_| if effect.stopped_qualified {store.startup_preflight(effect)} else {Ok(())}),
                                );
                                effect.inspected = true;
                            }
                            (*previous, false)
                        }
                        Work::RecoveryObserved {
                            mut previous,
                            observation,
                        } => {
                            let run = previous.effect.as_ref().map(Effect::run);
                            let result = match previous.effect.as_ref() {
                                Some(Effect::Recovery(effect)) => {
                                    store.observe_recovery(effect, observation)
                                }
                                _ => Err("workflow.recovery: callback ownership mismatch".into()),
                            };
                            previous.effect = None;
                            previous.reply = match result {
                                Ok(()) => store.execute(
                                    &WorkflowCommand::Show {
                                        version: 1,
                                        run: run.expect("reserved recovery"),
                                    },
                                    None,
                                    None,
                                    &[],
                                ),
                                Err(message) => WorkflowReply::Gap {
                                    code: "recovery_observation".into(),
                                    message,
                                    run,
                                    revision: None,
                                },
                            };
                            (*previous, false)
                        }
                        Work::ReviewPermitted {
                            mut previous,
                            permission,
                        } => {
                            let run = previous.effect.as_ref().map(Effect::run);
                            let result = match previous.effect.as_ref() {
                                Some(Effect::Review(effect)) => {
                                    store.launch_review(effect, permission)
                                }
                                _ => {
                                    Err("workflow.review: matching reserved effect unavailable"
                                        .into())
                                }
                            };
                            previous.effect = None;
                            previous.reply = match result {
                                Ok(()) => store.execute(
                                    &WorkflowCommand::Status {
                                        version: 1,
                                        run: run.expect("reserved review"),
                                    },
                                    None,
                                    None,
                                    &[],
                                ),
                                Err(message) => WorkflowReply::Gap {
                                    code: "review_callback".into(),
                                    message,
                                    run,
                                    revision: None,
                                },
                            };
                            (*previous, false)
                        }
                        Work::ReviewRefresh(run) => {
                            let reply = store.execute(
                                &WorkflowCommand::Status { version: 1, run },
                                None,
                                None,
                                &[],
                            );
                            let admit = store.take_effect_admission();
                            let (answer, _receive) = mpsc::channel();
                            (
                                Completed {
                                    reply,
                                    answer,
                                    effect: None,
                                    answered: true,
                                    presentation: crate::root::process::Presentation::None,
                                },
                                admit,
                            )
                        }
                        Work::ReviewJob(observed) => {
                            let result = store.observe_review_job(&observed);
                            let reply = match result {
                                Ok(()) => store.execute(
                                    &WorkflowCommand::Status {
                                        version: 1,
                                        run: observed.run,
                                    },
                                    None,
                                    None,
                                    &[],
                                ),
                                Err(message) => WorkflowReply::Gap {
                                    code: "review_job_observation".into(),
                                    message,
                                    run: Some(observed.run),
                                    revision: None,
                                },
                            };
                            let (answer, _receive) = mpsc::channel();
                            let admit = store.take_effect_admission();
                            (
                                Completed {
                                    reply,
                                    answer,
                                    effect: None,
                                    answered: true,
                                    presentation: crate::root::process::Presentation::None,
                                },
                                admit,
                            )
                        }
                        Work::Submitted(binding) => {
                            let reply =
                                store.record_submission(&binding).unwrap_or_else(|message| {
                                    WorkflowReply::Gap {
                                        code: "delivery_observation".into(),
                                        message,
                                        run: None,
                                        revision: None,
                                    }
                                });
                            let (answer, _receive) = mpsc::channel();
                            (
                                Completed {
                                    reply,
                                    answer,
                                    effect: None,
                                    answered: true,
                                    presentation: crate::root::process::Presentation::None,
                                },
                                false,
                            )
                        }
                        Work::Observed {
                            mut previous,
                            observed,
                        } => {
                            let result = previous
                                .effect
                                .as_ref()
                                .ok_or("workflow.assignment: native reservation missing".into())
                                .and_then(|effect| match effect {
                                    Effect::Assignment(effect) => {
                                        store.observe_assignment(effect, observed)
                                    }
                                    _ => Err("workflow: mismatched native callback".into()),
                                });
                            let run = previous.effect.as_ref().map(Effect::run);
                            previous.effect = None;
                            previous.reply = match result {
                                Ok(()) => store.execute(
                                    &WorkflowCommand::Status {
                                        version: 1,
                                        run: run.expect("reserved effect"),
                                    },
                                    None,
                                    None,
                                    &[],
                                ),
                                Err(message) => WorkflowReply::Gap {
                                    code: "assignment_observation".into(),
                                    message,
                                    run,
                                    revision: None,
                                },
                            };
                            (*previous, true)
                        }
                    };
                    if admit
                        && completion.effect.is_none()
                        && !matches!(completion.reply, WorkflowReply::Gap { .. })
                    {
                        let run = match &completion.reply {
                            WorkflowReply::Receipt { run, .. }
                            | WorkflowReply::Snapshot { run, .. } => Some(*run),
                            _ => None,
                        };
                        if let Some(run) = run {
                            match store.reserve_assignment(run) {
                                Ok(effect) => {
                                    completion.effect =
                                        effect.map(|e| Effect::Assignment(Box::new(e)))
                                }
                                Err(message) => {
                                    completion.reply = WorkflowReply::Gap {
                                        code: "assignment_admission".into(),
                                        message,
                                        run: Some(run),
                                        revision: None,
                                    }
                                }
                            }
                        }
                    }
                    if completion.effect.is_none() {
                        let run=match &completion.reply { WorkflowReply::Snapshot{run,..}|WorkflowReply::Receipt{run,..}=>Some(*run),_=>None};
                        if let Some(run)=run {
                            match store.prepare_handoff(run) {
                                Ok(Some(effect))=>{
                                    match store.handoff_preflight(&effect) {
                                        Ok(())=>completion.effect=Some(Effect::Handoff(Box::new(effect))),
                                        Err(reason)=>{let _=store.observe_handoff(&effect,"refused",&reason);}
                                    }
                                }
                                Ok(None)=>{},
                                Err(error)=>tracing::warn!(%error,"workflow handoff unavailable"),
                            }
                        }
                    }
                    if stopping.load(Ordering::Acquire) {
                        break;
                    }
                    if completed.send(completion).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("workflow.queue: {e}"))?;
        Ok(Self {
            send,
            observations,
            prompts,
            startups: vec![],
            startup_outcomes: Default::default(),
            corrections: vec![],
            correction_outcomes: Default::default(),
            handoffs: vec![],
            handoff_outcomes: Default::default(),
            native: None,
            children: Default::default(),
            review_intents: Default::default(),
            review_jobs: Default::default(),
            review_observations: Default::default(),
            submissions: Default::default(),
            submission_sent: Default::default(),
            waiting: Default::default(),
            in_flight: false,
            last_review_poll: std::time::Instant::now(),
            last_review_run: None,
            settled_reviews: Default::default(),
            completed: replies,
            stop,
            worker: Some(worker),
        })
    }
    fn enqueue(&mut self, job: Job) -> Result<(), String> {
        if self.waiting.len() + usize::from(self.in_flight) >= 64 {
            return Err("workflow.queue: busy; retry the same request identity".into());
        }
        self.waiting.push_back(job);
        self.pump();
        Ok(())
    }
    fn review_progress(&mut self, reply: &WorkflowReply) {
        let (run, snapshot) = match reply {
            WorkflowReply::Snapshot { run, snapshot, .. }
            | WorkflowReply::Receipt { run, snapshot, .. } => (*run, snapshot),
            _ => return,
        };
        let Some(reviews) = snapshot["reviews"].as_array().filter(|v| !v.is_empty()) else {
            return;
        };
        // Poll intents, including failures before job creation. Once both
        // owners have ended and result/uncertainty is retained, history needs
        // no background disk walk until another explicit effect is admitted.
        if reviews.iter().all(|r| {
            r["observation"]["observer"]["pid"].is_number()
                && (r.get("job").is_none()
                    || r.get("result").is_some()
                        && serde_json::from_value::<guild_session::terminal_job::JobReceipt>(
                            r["job"].clone(),
                        )
                        .is_ok_and(|j| j.state.is_terminal()))
        }) {
            self.settled_reviews.insert(run);
        }
    }
    fn acknowledged(&mut self) {
        self.in_flight = false;
    }
    fn pump(&mut self) {
        if self.in_flight {
            return;
        }
        if let Some((effect, observed)) = self.correction_outcomes.pop_front() {
            match self.send.try_send(Work::CorrectionObserved {
                effect: Box::new(effect),
                observed,
            }) {
                Ok(()) => self.in_flight = true,
                Err(mpsc::TrySendError::Full(Work::CorrectionObserved { effect, observed })) => {
                    self.correction_outcomes.push_front((*effect, observed))
                }
                Err(_) => tracing::error!("correction observation lost; no replay"),
            }
            return;
        }
        if let Some((effect, observed)) = self.startup_outcomes.pop_front() {
            match self.send.try_send(Work::StartupObserved {
                effect: Box::new(effect),
                observed,
            }) {
                Ok(()) => self.in_flight = true,
                Err(mpsc::TrySendError::Full(Work::StartupObserved { effect, observed })) => {
                    self.startup_outcomes.push_front((*effect, observed))
                }
                Err(_) => {
                    tracing::error!("startup observation lost; retained submission remains unknown")
                }
            }
            return;
        }
        if let Some((effect, observed)) = self.handoff_outcomes.pop_front() {
            match self.send.try_send(Work::HandoffObserved {
                effect: Box::new(effect),
                observed,
            }) {
                Ok(()) => self.in_flight = true,
                Err(mpsc::TrySendError::Full(Work::HandoffObserved { effect, observed })) => {
                    self.handoff_outcomes.push_front((*effect, observed))
                }
                Err(_) => {
                    tracing::error!("handoff observation lost; submitted state remains unknown")
                }
            }
            return;
        }
        if let Some(observed) = self.review_observations.pop_front() {
            match self.send.try_send(Work::ReviewJob(Box::new(observed))) {
                Ok(()) => self.in_flight = true,
                Err(mpsc::TrySendError::Full(Work::ReviewJob(observed))) => {
                    self.review_observations.push_front(*observed)
                }
                Err(_) => tracing::error!(
                    "workflow actual review job observation could not reach writer; history remains unconfirmed"
                ),
            }
            return;
        }
        if let Some(binding) = self
            .submissions
            .iter()
            .find(|(s, _)| self.children.contains(s) && !self.submission_sent.contains(s))
            .map(|(_, b)| b.clone())
        {
            let session = binding.session;
            match self.send.try_send(Work::Submitted(Box::new(binding))) {
                Ok(()) => {
                    self.in_flight = true;
                    self.submission_sent.insert(session);
                }
                Err(_) => tracing::error!(
                    "workflow delivery observation could not reach its writer; delivery remains unconfirmed"
                ),
            }
            return;
        }
        let Some(job) = self.waiting.pop_front() else {
            if self.last_review_poll.elapsed() >= std::time::Duration::from_millis(500) {
                let runs: std::collections::BTreeSet<_> = self
                    .review_intents
                    .values()
                    .map(|j| j.run)
                    .filter(|run| !self.settled_reviews.contains(run))
                    .collect();
                if let Some(run) = runs
                    .iter()
                    .find(|r| self.last_review_run.is_none_or(|last| **r > last))
                    .or_else(|| runs.first())
                    .copied()
                    && self.send.try_send(Work::ReviewRefresh(run)).is_ok()
                {
                    self.in_flight = true;
                    self.last_review_poll = std::time::Instant::now();
                    self.last_review_run = Some(run);
                }
            }
            return;
        };
        match self.send.try_send(Work::Request(Box::new(job))) {
            Ok(()) => self.in_flight = true,
            Err(mpsc::TrySendError::Full(Work::Request(job))) => self.waiting.push_front(*job),
            Err(mpsc::TrySendError::Disconnected(Work::Request(job))) => {
                for job in std::iter::once(*job).chain(self.waiting.drain(..)) {
                    let _=job.answer.send(Answer::Workflow {reply:WorkflowReply::Gap {code:"coordinator_stopped".into(),message:"workflow queue stopped; inspect retained receipts before retrying the same request identity".into(),run:job.request.run(),revision:None}});
                }
            }
            Err(_) => unreachable!("pump only sends public requests"),
        }
    }
}
impl Drop for Queue {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Do not block the UI on disk IO. The worker stops after its current
        // atomic operation, without admitting queued requests after Root closes.
        self.worker.take();
    }
}
impl Root {
    fn refresh_reconciliation_observations(&self, cx: &gpui::App) {
        let Some(queue) = self.workflows.as_ref() else {
            return;
        };
        let mut rows = std::collections::BTreeMap::new();
        for record in self.desk.sessions() {
            let Ok(mut binding) = self.workflow_binding(record.id) else {
                continue;
            };
            binding.area = None;
            let mut holders = vec![];
            let mut qualified = !self.desk.sessions().any(|other| {
                other.id != record.id
                    && other.runs.harness() == Some(binding.harness)
                    && other.harness_session.is_some()
                    && other.harness_session == binding.conversation
            });
            for body in self
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .filter(|b| b.session() == Some(record.id))
            {
                match body {
                    crate::Body::Terminal { view, facts, .. } => {
                        if facts.resumed().is_some_and(|(kind, id)| {
                            kind != binding.harness || Some(id) != binding.conversation
                        }) {
                            qualified = false;
                        }
                        if !view.read(cx).exited() {
                            match view.read(cx).process_id() {
                                Some(pid) => holders.push(pid),
                                None => qualified = false,
                            }
                        }
                    }
                    _ => qualified = false,
                }
            }
            holders.sort_unstable();
            holders.dedup();
            rows.insert(
                record.id.to_string(),
                store::reconciliation::Native {
                    binding,
                    task: record.task,
                    selection: record.launch_selection.clone(),
                    permission: record.launch_permission,
                    status: format!("{:?}", self.status_of(record.id)),
                    holders,
                    identity_qualified: qualified,
                },
            );
        }
        *queue
            .observations
            .write()
            .unwrap_or_else(|e| e.into_inner()) = rows;
    }
    fn reconciliation_native_permit(
        &self,
        run: RunId,
        permit: Option<&serde_json::Value>,
        cx: &gpui::App,
    ) -> Result<(), String> {
        self.refresh_reconciliation_observations(cx);
        let Some(expected) = permit else {
            if self
                .processes
                .runs
                .get(&run)
                .and_then(|r| r.value.as_ref())
                .is_some_and(|value| {
                    value["recovery"]["state"]["parent_attention_required"] == true
                })
            {
                return Err(
                    "workflow.reconcile: parent reconciliation required before effect".into(),
                );
            }
            return Ok(());
        };
        let Some(queue) = self.workflows.as_ref() else {
            return Err("workflow.reconcile: native owner unavailable".into());
        };
        let current = queue.observations.read().unwrap_or_else(|e| e.into_inner());
        store::reconciliation::check_native_permit(expected, &current)
    }
    pub(super) fn workflow_binding(&self, id: SessionId) -> Result<OriginBinding, String> {
        let record = self
            .desk
            .session(id)
            .ok_or("workflow.binding: session is not owned by this window")?;
        let harness = record
            .runs
            .harness()
            .ok_or("workflow.binding: an agent conversation is required")?;
        Ok(OriginBinding {
            session: id,
            project: record.project,
            workspace: record.workspace,
            area: self
                .tabs
                .iter()
                .find(|t| t.sessions().any(|s| s == id))
                .map(|t| t.area),
            harness,
            conversation: if harness == guild_session::HarnessKind::Claude {
                Some(id.to_string())
            } else {
                record.harness_session.clone()
            },
            cwd: record.cwd.clone(),
        })
    }
    pub(super) fn workflow_review_job_started(
        &mut self,
        spec: &guild_session::terminal_job::LaunchSpec,
        receipt: &guild_session::terminal_job::JobReceipt,
    ) {
        let Some(queue) = self.workflows.as_mut() else {
            return;
        };
        let Some(effect) = queue.review_intents.values().find(|e| {
            spec.revmux_archive
                .as_ref()
                .is_some_and(|a| a.directory == e.round.archive)
        }) else {
            return;
        };
        let observed = store::review::ReviewJob {
            run: effect.run,
            attempt: effect.attempt,
            round_sha256: effect.round.sha256.clone(),
            receipt: receipt.clone(),
        };
        if queue
            .review_jobs
            .get(&receipt.id)
            .is_some_and(|old| old.receipt == *receipt)
        {
            return;
        }
        queue.review_jobs.insert(receipt.id, observed.clone());
        queue.review_observations.push_back(observed);
    }
    fn workflow_review_job_changes(&mut self) {
        let Some(queue) = self.workflows.as_mut() else {
            return;
        };
        for observed in queue.review_jobs.values_mut() {
            if let Some(receipt) = self.terminal_jobs.receipt(&observed.receipt.id)
                && receipt != observed.receipt
            {
                observed.receipt = receipt;
                queue.review_observations.push_back(observed.clone());
            }
        }
        queue.pump();
    }
    pub(super) fn workflow_review_terminal_guard(
        &self,
        spec: &guild_session::terminal_job::LaunchSpec,
        caller: SessionId,
        cx: &gpui::App,
    ) -> Result<(), String> {
        let Some(effect) = self.workflows.as_ref().and_then(|q| {
            q.review_intents.values().find(|e| {
                spec.cwd == e.round.configuration.directory
                    || spec
                        .revmux_archive
                        .as_ref()
                        .is_some_and(|a| a.directory == e.round.archive)
            })
        }) else {
            return Ok(());
        };
        self.reconciliation_native_permit(effect.run, effect.native.as_ref(), cx)?;
        self.workflow_parent_binding(&effect.origin, cx)?;
        let expected = effect
            .round
            .package
            .root
            .join("bin/revust")
            .to_string_lossy()
            .into_owned();
        let helper = effect
            .round
            .package
            .root
            .join("scripts/guild-run.sh")
            .to_string_lossy()
            .into_owned();
        let linked = caller == effect.origin.session
            && spec.cwd == effect.round.configuration.directory
            && spec.program == "/bin/bash"
            && spec.args.len() == effect.round.args.len() + 4
            && spec.args.first() == Some(&helper)
            && spec.args.get(3) == Some(&expected)
            && spec.args[4..] == effect.round.args
            && spec.revmux_archive.as_ref().is_some_and(|a| {
                a.directory == effect.round.archive
                    && a.program == effect.round.package.root.join("bin/revust")
            });
        if !linked {
            return Err("workflow.review: native job descriptor differs from reserved launcher/source/configuration".into());
        }
        Ok(())
    }
    /// The session's harness conversation is now known: a turn start held
    /// for it is sent (review r1, finding 2).
    pub(super) fn conversation_admitted(&mut self, session: SessionId) {
        if self.unbound_starts.remove(&session) {
            self.workflow_prompt_submitted(session);
        }
    }
    pub(super) fn workflow_prompt_submitted(&mut self, session: SessionId) {
        if self.desk.session(session).is_some_and(|r| r.task)
            && let Ok(binding) = self.workflow_binding(session)
            && let Some(queue) = self.workflows.as_mut()
        {
            // A turn can begin (OSC 7501 `working`) before the hook naming a
            // Codex or OpenCode conversation is admitted; the store rejects an
            // unbound start for good, so it waits for the identity instead.
            if binding.conversation.as_deref().is_none_or(str::is_empty) {
                self.unbound_starts.insert(session);
                return;
            }
            queue.submissions.insert(session, binding);
            queue.pump();
        }
    }
    pub(super) fn command_workflow(
        &mut self,
        request: &WorkflowCommand,
        caller: Option<&str>,
        answer: mpsc::Sender<Answer>,
        cx: &mut Context<Self>,
    ) {
        self.command_workflow_with_navigation(request, caller, answer, false, cx);
    }
    /// Only an actual native gesture reaches this entry. No wire field can grant it.
    pub(super) fn navigate_workflow_recovery(
        &mut self,
        request: &WorkflowCommand,
        session: SessionId,
        answer: mpsc::Sender<Answer>,
        cx: &mut Context<Self>,
    ) {
        let eligible = request
            .run()
            .is_some_and(|run| self.process_for_session(session).contains(&run));
        if !eligible || !matches!(request, WorkflowCommand::Recover { .. }) {
            return;
        }
        self.command_workflow_with_navigation(
            request,
            Some(&session.to_string()),
            answer,
            true,
            cx,
        );
    }
    fn command_workflow_with_navigation(
        &mut self,
        request: &WorkflowCommand,
        caller: Option<&str>,
        answer: mpsc::Sender<Answer>,
        trusted_navigation: bool,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            request.validate()?;
            let caller = caller
                .map(|raw| {
                    raw.parse::<SessionId>()
                        .map_err(|_| "workflow.caller: invalid session".to_owned())
                })
                .transpose()?
                .map(|id| crate::binding::resolve(id, &self.started_as));
            if caller.is_some_and(|id| self.desk.session(id).is_none()) {
                return Err("workflow.caller: not owned by this window".into());
            }
            let origin = if let WorkflowCommand::Start { spec } = request {
                let id = crate::binding::resolve(spec.origin, &self.started_as);
                let record = self
                    .desk
                    .session(id)
                    .ok_or("workflow.origin: not owned by this window")?;
                if caller.is_some_and(|caller| caller != id) || record.task {
                    return Err("workflow.authority: a managed task cannot initiate or steer another workflow".into());
                }
                let area = self
                    .tabs
                    .iter()
                    .find(|t| t.sessions().any(|s| s == id))
                    .map(|t| t.area);
                if !record.open || area.is_none() {
                    return Err(
                        "workflow.origin: initiation requires its actual open native session"
                            .into(),
                    );
                }
                let harness = record
                    .runs
                    .harness()
                    .ok_or("workflow.origin: an agent conversation is required")?;
                Some(OriginBinding {
                    session: id,
                    project: record.project,
                    workspace: record.workspace,
                    area,
                    harness,
                    conversation: if harness == guild_session::HarnessKind::Claude {
                        Some(id.to_string())
                    } else {
                        record.harness_session.clone()
                    },
                    cwd: record.cwd.clone(),
                })
            } else if matches!(request, WorkflowCommand::Capabilities { .. }) {
                None
            } else {
                caller.map(|id| self.workflow_binding(id)).transpose()?
            };
            let caller_pid = caller.and_then(|caller| {
                self.tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|body| match body {
                        crate::Body::Terminal { session, view, .. }
                            if *session == caller && !view.read(cx).exited() =>
                        {
                            view.read(cx).process_id()
                        }
                        _ => None,
                    })
            });
            if matches!(request, WorkflowCommand::Recover { .. }) {
                let session = caller.ok_or("workflow.recovery: actual caller required")?;
                if !self.tabs.iter().flat_map(crate::Tab::bodies).any(|b|matches!(b,crate::Body::Terminal {session:s,view,..} if *s==session && !view.read(cx).exited() && view.read(cx).process_id().is_some())) {return Err("workflow.recovery: caller has no live owned native runtime".into());}
            }
            let review_host = if matches!(request, WorkflowCommand::Review { .. }) {
                let session = caller.ok_or("workflow.review: actual parent caller required")?;
                let record = self
                    .desk
                    .session(session)
                    .ok_or("workflow.review: parent unavailable")?;
                let live=!record.task && record.open && self.tabs.iter().flat_map(crate::Tab::bodies).any(|body|matches!(body,crate::Body::Terminal {session:actual,view,..} if *actual==session && !view.read(cx).exited() && view.read(cx).process_id().is_some()));
                if !live {
                    return Err("workflow.review: original parent must own a live native runtime; no managed-child review".into());
                }
                let socket = self
                    .hook_launch()
                    .ok_or("workflow.review: owning native socket unavailable")?
                    .socket;
                let cli=workflow_cli_program().ok_or("workflow.review: this build's public Guild CLI is unavailable beside the desktop")?;
                Some(RevustHost {
                    session,
                    socket,
                    cli,
                })
            } else {
                None
            };
            let roots = self.desk.projects().map(|p| p.path.clone()).collect();
            if self.workflows.is_none() {
                self.workflows = Some(Queue::new(if self.integrations.external() {
                    store::global_skill_roots()
                } else {
                    vec![]
                })?);
            }
            self.refresh_reconciliation_observations(cx);
            self.workflows
                .as_mut()
                .expect("initialized queue")
                .enqueue(Job {
                    entry_enabled: self.settings.development_workflows,
                    trusted_navigation,
                    request: request.clone(),
                    presentation: match request {
                        WorkflowCommand::Start { .. } => {
                            crate::root::process::Presentation::Background
                        }
                        WorkflowCommand::Show { .. } => {
                            crate::root::process::Presentation::Select(self.processes.epoch)
                        }
                        WorkflowCommand::Recover { .. } => {
                            crate::root::process::Presentation::Recover(self.processes.epoch)
                        }
                        _ => crate::root::process::Presentation::None,
                    },
                    caller,
                    origin,
                    roots,
                    review_host,
                    recovery_records: self.desk.sessions().cloned().collect(),
                    caller_pid,
                    answer: answer.clone(),
                })?;
            Ok::<_, String>(())
        })();
        if let Err(message) = result {
            let reply = WorkflowReply::Gap {
                code: "admission".into(),
                message,
                run: request.run(),
                revision: None,
            };
            self.observe_process_reply(&reply, crate::root::process::Presentation::None, cx);
            let _ = answer.send(Answer::Workflow { reply });
        }
        cx.notify();
    }
    pub(crate) fn inspect_process(&mut self, run: RunId) -> Result<(), String> {
        if self.workflows.is_none() {
            self.workflows = Some(Queue::new(if self.integrations.external() {
                store::global_skill_roots()
            } else {
                vec![]
            })?);
        }
        let (answer, _) = mpsc::channel();
        self.workflows.as_mut().unwrap().enqueue(Job {
            entry_enabled: self.settings.development_workflows,
            trusted_navigation: false,
            request: WorkflowCommand::Show { version: 1, run },
            presentation: crate::root::process::Presentation::None,
            caller: None,
            origin: None,
            review_host: None,
            recovery_records: vec![],
            caller_pid: None,
            roots: self.desk.projects().map(|p| p.path.clone()).collect(),
            answer,
        })
    }
    pub(super) fn poll_workflows(&mut self, cx: &mut Context<Self>) {
        self.refresh_reconciliation_observations(cx);
        self.refresh_handoff_prompts(cx);
        self.poll_handoff_observations(cx);
        self.refresh_process_views(cx);
        self.workflow_review_job_changes();
        if let Some(pending) = self.workflows.as_mut().and_then(|q| q.native.take()) {
            let observed = match pending.receive.try_recv() {
                Ok(answer) => Some(native::observe(
                    match pending
                        .previous
                        .effect
                        .as_ref()
                        .expect("native reservation")
                    {
                        Effect::Assignment(effect) => effect,
                        Effect::Review(_)
                        | Effect::Recovery(_)
                        | Effect::Handoff(_)
                        | Effect::Correction(_) => {
                            unreachable!("effect uses its own native permission callback")
                        }
                    },
                    answer,
                    self,
                    cx,
                )),
                Err(mpsc::TryRecvError::Disconnected) => Some(Observed::Refused {
                    reason: "native callback disconnected; delivery remains unconfirmed".into(),
                }),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(observed) = observed {
                if let Observed::Launched {
                    session,
                    bound: true,
                    pid: Some(_),
                    ..
                } = &observed
                    && let Some(queue) = self.workflows.as_mut()
                {
                    queue.children.insert(*session);
                }
                if let Some(queue) = self.workflows.as_mut()
                    && queue
                        .send
                        .try_send(Work::Observed {
                            previous: Box::new(pending.previous),
                            observed,
                        })
                        .is_err()
                {
                    tracing::error!(
                        "workflow native observation could not reach its writer; reservation remains uncertain"
                    );
                }
            } else if let Some(queue) = self.workflows.as_mut() {
                queue.native = Some(pending)
            }
        }
        let completion = self
            .workflows
            .as_ref()
            .and_then(|q| q.completed.try_recv().ok());
        if let Some(mut completion) = completion {
            self.observe_process_reply(&completion.reply, completion.presentation, cx);
            if let WorkflowReply::Snapshot { run, snapshot, .. }
            | WorkflowReply::Receipt { run, snapshot, .. } = &completion.reply
                && snapshot["nodes"]
                    .as_array()
                    .is_some_and(|nodes| nodes.iter().any(|n| n["state"] == "cancelled"))
            {
                let jobs = self
                    .workflows
                    .as_ref()
                    .into_iter()
                    .flat_map(|q| q.review_jobs.values())
                    .filter(|j| j.run == *run)
                    .filter_map(|j| self.terminal_jobs.receipt(&j.receipt.id))
                    .filter(|j| j.state == guild_session::terminal_job::JobState::Running)
                    .collect::<Vec<_>>();
                for job in jobs {
                    let answer = self.command_terminal(
                        &guild_session::terminal_job::TerminalCommand::Cancel { id: job.id },
                        Some(&job.owner.to_string()),
                        cx,
                    );
                    if let Answer::Refused { because } = answer {
                        tracing::warn!(%because,"cancelled workflow review owner could not be stopped; retain lease/history");
                    }
                }
            }

            completion.presentation = crate::root::process::Presentation::None;
            if let Some(queue) = self.workflows.as_mut() {
                queue.review_progress(&completion.reply);
            }
            if !completion.answered {
                let _ = completion.answer.send(Answer::Workflow {
                    reply: completion.reply.clone(),
                });
                completion.answered = true;
            } else if let WorkflowReply::Gap { message, .. } = &completion.reply {
                tracing::warn!("workflow effect reconciliation: {message}");
            }
            if let Some(Effect::Correction(e)) = &completion.effect {
                let e = *e.clone();
                self.perform_workflow_correction(e, completion, cx);
                return;
            }
            if let Some(Effect::Handoff(effect)) = &completion.effect {
                let effect = *effect.clone();
                self.perform_workflow_handoff(effect, completion, cx);
                return;
            }
            if let Some(Effect::Recovery(effect)) = &completion.effect {
                let effect = effect.clone();
                self.perform_workflow_recovery(*effect, completion, cx);
                return;
            }
            if let Some(Effect::Review(effect)) = &completion.effect {
                let permission = self
                    .reconciliation_native_permit(effect.run, effect.native.as_ref(), cx)
                    .and_then(|_| self.workflow_parent_binding(&effect.origin, cx));
                if permission.is_ok()
                    && let Some(queue) = self.workflows.as_mut()
                {
                    queue.settled_reviews.remove(&effect.run);
                    queue
                        .review_intents
                        .insert(effect.round.archive.clone(), *effect.clone());
                }
                if let Some(queue) = self.workflows.as_mut()
                    && queue
                        .send
                        .try_send(Work::ReviewPermitted {
                            previous: Box::new(completion),
                            permission,
                        })
                        .is_err()
                {
                    tracing::error!(
                        "workflow review callback could not reach writer; reservation remains charged"
                    );
                }
                return;
            }
            if let Some(Effect::Assignment(effect)) = &completion.effect {
                let receive = self.perform_workflow_assignment(effect, cx);
                if let Some(queue) = self.workflows.as_mut() {
                    queue.native = Some(PendingNative {
                        previous: completion,
                        receive,
                    });
                }
            } else if let Some(queue) = self.workflows.as_mut() {
                queue.acknowledged();
                queue.pump();
            }
            cx.notify();
        }
    }
}

pub(super) fn continuation_process_snapshot() -> Result<String, String> {
    recovery::process_snapshot()
}

/// Shared exact-store/holder inspection; task authority is checked by its caller.
pub(super) fn inspect_task(
    record: &guild_session::SessionRecord,
    conversation: &str,
) -> Result<(), String> {
    recovery::inspect_origin(&store::recovery::Binding {
        origin: OriginBinding {
            session: record.id,
            project: record.project,
            workspace: record.workspace,
            area: None,
            harness: record.runs.harness().ok_or("Task harness missing")?,
            conversation: Some(conversation.into()),
            cwd: record.cwd.clone(),
        },
        selection: record.launch_selection.clone(),
        permission: record.launch_permission,
    })
}
/// No coordinator is initialized by read-only task admission.
pub(super) fn continuation_membership(record: &guild_session::SessionRecord) -> Result<(), String> {
    store::continuation_membership(record)
}
impl Root {
    pub(super) fn workflow_holds_task(&self, id: SessionId) -> bool {
        self.workflows
            .as_ref()
            .is_some_and(|q| q.children.contains(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Body;
    use gpui::AppContext;
    use guild_session::{Desk, Runs, SessionMode, SessionRecord};
    /// **A turn that begins before the conversation is known waits for it**
    /// — review r1, finding 2. A Codex worker can report its first `working`
    /// before the hook carrying its conversation id is admitted; the start
    /// is then held, not sent unbound (which the store rejects for good), and
    /// is sent once the identity arrives.
    #[gpui::test]
    fn a_start_before_the_conversation_is_held_until_it_is_known(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let session = desk.add_session(SessionRecord::new(
            "worker".into(),
            guild_session::Seat { workspace, project },
            dir.path().into(),
            Runs::Codex(SessionMode::Terminal),
        ));
        desk.mark_task(session);
        let entity = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        entity.update(cx, |root, _| {
            root.workflows = Some(Queue::new(vec![]).unwrap());
            root.prompt_began(session);
            assert!(
                !root
                    .workflows
                    .as_ref()
                    .unwrap()
                    .submissions
                    .contains_key(&session),
                "a start without its conversation is not sent"
            );
            root.desk
                .record_harness_session(session, "codex-conversation".into());
            root.conversation_admitted(session);
            let sent = &root.workflows.as_ref().unwrap().submissions[&session];
            assert_eq!(sent.conversation.as_deref(), Some("codex-conversation"));
            root.conversation_admitted(session);
            assert_eq!(
                root.workflows.as_ref().unwrap().submissions.len(),
                1,
                "once"
            );
        });
    }
    #[gpui::test]
    fn missing_persisted_run_is_a_local_gap_and_keeps_ordinary_tabs(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let run = RunId::new();
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        guild_session::project_state::save(
            dir.path(),
            &guild_session::ProjectState {
                tabs: vec![
                    guild_session::OpenTab::showing(guild_session::Shows::Settings),
                    guild_session::OpenTab::showing(guild_session::Shows::Process { run }),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        guild_session::flush();
        let entity = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        entity.update(cx, |root, cx| root.restore_tabs(cx));
        let parent = entity.update(cx, |root, cx| {
            let parent = root.adopt(guild_session::Seat {workspace, project}, project,
                Body::Broken {session: SessionId::new(), reason: "Controlled ordinary session".into()}, cx);
            // Hold the existing serialized queue and fill its actual admission bound.
            root.workflows = Some(Queue::new(vec![]).unwrap());
            root.workflows.as_mut().unwrap().in_flight = true;
            for _ in 0..63 { root.inspect_process(RunId::new()).unwrap(); }
            root.refresh_process_views(cx);
            assert!(root.processes.runs[&run].gap.as_ref().is_some_and(|gap| gap.contains("queue: busy")));
            assert!(root.process_move_group(parent).is_err(), "a local queue refusal is not an orphan observation");
            let (answer, reply) = mpsc::channel();
            root.command_workflow(&WorkflowCommand::Show {version: 1, run}, None, answer, cx);
            assert!(matches!(reply.recv().unwrap(), Answer::Workflow {reply: WorkflowReply::Gap {code, ..}} if code == "admission"));
            assert!(root.process_move_group(parent).is_err(), "public queue refusal is not an inspection reply either");
            let queue = root.workflows.as_mut().unwrap();
            queue.waiting.clear();
            queue.in_flight = false;
            root.inspect_process(run).unwrap();
            assert!(root.process_move_group(parent).is_err(), "retry without its reply must retain the guard");
            parent
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let done = entity.update(cx, |root, cx| {
                root.poll_workflows(cx);
                root.processes.runs.get(&run).is_some_and(|s| {
                    s.gap
                        .as_ref()
                        .is_some_and(|gap| !gap.contains("queue: busy"))
                })
            });
            if done {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "missing run inspection did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        entity.update(cx, |root, cx| {
            assert_eq!(root.tabs.len(), 3);
            assert!(
                root.process_move_group(parent).unwrap().is_none(),
                "confirmed missing run releases unrelated navigation"
            );
            assert!(
                root.tabs
                    .iter()
                    .any(|t| matches!(t.body(), Some(Body::Settings { .. })))
            );
            assert!(
                root.tabs
                    .iter()
                    .any(|t| matches!(t.body(),Some(Body::Process { run:r }) if *r==run))
            );
            assert!(root.processes.runs[&run].value.is_none());
            assert!(root.desk.project(project).is_some());
            let pane = root
                .tabs
                .iter()
                .flat_map(|tab| tab.panes.bodies())
                .find_map(|(pane, body)| {
                    matches!(body,crate::Body::Process {run:r} if *r==run).then_some(pane)
                })
                .unwrap();
            let _ = root.render_process(run, pane, cx);
        });
        let worker = entity.update(cx, |root, _| {
            root.workflows.take().and_then(|mut q| q.worker.take())
        });
        worker.unwrap().join().unwrap();
    }

    #[gpui::test]
    fn owning_handler_rejects_nested_tasks_and_returns_durable_start_without_focus_theft(
        cx: &mut gpui::TestAppContext,
    ) {
        // This fixture also renders the native Process controls, whose palette
        // comes from the ordinary component initialization.
        cx.update(gpui_component::init);
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("task.md"),
            "Draft requested work; preparing design",
        )
        .unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "task.md"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-qm",
                "Entry fixture",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(directory.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }
        assert!(!directory.path().join(".gitignore").exists());
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), directory.path());
        let mut record = SessionRecord::new(
            "fixture origin".into(),
            project,
            directory.path().into(),
            Runs::Claude(SessionMode::Terminal),
        );
        record.had_turn = true;
        let origin = desk.add_session(record);
        let mut child = SessionRecord::new(
            "managed fixture".into(),
            project,
            directory.path().into(),
            Runs::Claude(SessionMode::Terminal),
        );
        child.task = true;
        child.had_turn = true;
        let child = desk.add_session(child);
        let entity = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        let request = WorkflowCommand::Start {
            spec: StartRequest {
                version: 1,
                request_id: "native-fixture".into(),
                expected_revision: 0,
                origin,
                target: directory.path().into(),
                selection: Some("task.md".into()),
                definition: None,
                overrides: Some(
                    serde_json::json!({"label":"Long configured fixture process · Процесс без провайдера"}),
                ),
                task: "Controlled handler fixture; no harness or provider launch".into(),
                architecture_only: false,
                checks: vec![],
            },
        };
        let (send, receive) = mpsc::channel();
        entity.update(cx, |root, cx| {
            root.attach_test_hooks(state.path());
            let seat = guild_session::Seat {
                project,
                workspace: root.desk.active_workspace(),
            };
            root.adopt(
                seat,
                project,
                crate::Body::Broken {
                    session: origin,
                    reason: "controlled body fixture".into(),
                },
                cx,
            );
            root.take_command(
                crate::hooks::server::Asked {
                    command: guild_session::command::Command::Workflow {
                        request: request.clone(),
                        caller: Some(child.to_string()),
                    },
                    answer: send.clone(),
                },
                cx,
            );
            assert!(
                root.workflows.is_none(),
                "unauthorized child must never reach execution queue"
            );
        });
        let denied = receive.try_recv().unwrap();
        assert!(
            matches!(denied,Answer::Workflow {reply:WorkflowReply::Gap {message,..}} if message.contains("authority"))
        );
        // Real public admission: default-off capabilities and Start have no effects.
        for command in [
            WorkflowCommand::Capabilities { version: 1 },
            request.clone(),
        ] {
            let (answer, receive) = mpsc::channel();
            entity.update(cx, |root, cx| {
                root.command_workflow(&command, Some(&origin.to_string()), answer, cx)
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let reply = loop {
                entity.update(cx, |root, cx| root.poll_workflows(cx));
                if let Ok(reply) = receive.try_recv() {
                    break reply;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            match command {
                WorkflowCommand::Capabilities { .. } => assert!(
                    matches!(
                        reply,
                        Answer::Workflow {
                            reply: WorkflowReply::Capabilities {
                                entry_before_design: false,
                                ..
                            }
                        }
                    ),
                    "{reply:?}"
                ),
                _ => assert!(
                    matches!(reply, Answer::Workflow { reply: WorkflowReply::Gap { ref message, .. } } if message.contains("Settings > Experimental")),
                    "{reply:?}"
                ),
            }
            assert!(!directory.path().join(".guild/workflow-runs").exists());
        }
        entity.update(cx, |root, _| {
            root.settings =
                serde_json::from_value(serde_json::json!({"development_workflows":true})).unwrap();
        });
        entity.update(cx, |root, cx| {
            root.take_command(
                crate::hooks::server::Asked {
                    command: guild_session::command::Command::Workflow {
                        request: request.clone(),
                        caller: Some(origin.to_string()),
                    },
                    answer: send,
                },
                cx,
            )
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let reply = loop {
            entity.update(cx, |root, cx| root.poll_workflows(cx));
            if let Ok(reply) = receive.try_recv() {
                break reply;
            }
            assert!(
                std::time::Instant::now() < until,
                "workflow worker did not answer"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(
            matches!(
                reply,
                Answer::Workflow {
                    reply: WorkflowReply::Receipt { revision: 2, .. }
                }
            ),
            "{reply:?}"
        );
        entity.update(cx, |root, _| {
            assert!(root.active_tab.is_none());
            assert_eq!(
                root.tabs.len(),
                2,
                "start must add its native Process view without selecting it"
            );
            assert!(root.tabs.iter().all(|t| {
                t.panes
                    .bodies()
                    .into_iter()
                    .all(|(_, b)| !matches!(b, crate::Body::Terminal { .. }))
            }));
        });
        let (run, revision, snapshot) = match reply {
            Answer::Workflow {
                reply:
                    WorkflowReply::Receipt {
                        run,
                        revision,
                        snapshot,
                        ..
                    },
            } => (run, revision, snapshot),
            _ => unreachable!(),
        };
        entity.update(cx, |root, _| root.settings.development_workflows = false);
        let mut distinct = request.clone();
        if let WorkflowCommand::Start { spec } = &mut distinct {
            spec.request_id = "disabled-new-intent".into();
        }
        for command in [
            WorkflowCommand::Capabilities { version: 1 },
            request.clone(),
            request.clone(),
            distinct,
        ] {
            let (answer, receive) = mpsc::channel();
            entity.update(cx, |root, cx| {
                root.command_workflow(&command, Some(&origin.to_string()), answer, cx)
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let reply = loop {
                entity.update(cx, |root, cx| root.poll_workflows(cx));
                if let Ok(reply) = receive.try_recv() {
                    break reply;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            match &command {
                WorkflowCommand::Capabilities { .. } => assert!(
                    matches!(reply, Answer::Workflow { reply: WorkflowReply::Capabilities { entry_before_design: false, ref operations, .. } } if !operations.iter().any(|op| op == "start")),
                    "{reply:?}"
                ),
                WorkflowCommand::Start { spec } if spec.request_id == "disabled-new-intent" => {
                    assert!(
                        matches!(reply, Answer::Workflow {reply: WorkflowReply::Gap { ref message, .. }} if message.contains("Settings > Experimental")),
                        "{reply:?}"
                    )
                }
                _ => assert!(
                    matches!(reply, Answer::Workflow {reply: WorkflowReply::Receipt {run:r,revision:2,..}} if r == run),
                    "{reply:?}"
                ),
            }
        }
        // All subsequent Show/checkpoint operations in this fixture run while off.
        entity.update(cx, |root, cx| {
            assert_eq!(root.process_for_origin(origin), vec![run]);
            assert_eq!(snapshot["nodes"][0]["state"], "active");
            assert!(snapshot["accepted_contract"].is_null());
            assert_eq!(snapshot["assignments"], serde_json::json!([]));
            assert_eq!(snapshot["reviews"], serde_json::json!([]));
            let cached = &root.processes.runs[&run];
            assert_eq!(cached.value.as_ref(), Some(&snapshot));
            assert_eq!(
                cached.value.as_ref().unwrap()["graph"]["definition"]["label"],
                "Long configured fixture process · Процесс без провайдера"
            );
            let process = root
                .tabs
                .iter()
                .find(|t| matches!(t.body(),Some(crate::Body::Process { run: r }) if *r == run))
                .unwrap();
            assert_eq!(process.session(), None);
            assert_eq!(process.area, root.tabs[0].area);
            let process_id = process.id;
            root.close_tab(process_id, cx);
            assert_eq!(root.tabs.len(), 1);
            assert!(root.desk.session(origin).unwrap().open);
            assert!(root.processes.runs.contains_key(&run));
            assert_eq!(
                root.process_for_origin(origin),
                vec![run],
                "closed Process keeps its reopen icon link"
            );
            assert!(root.process_tab_tip(run).contains(&origin.to_string()));
        });
        for _ in 0..2 {
            let (answer, receive) = mpsc::channel();
            entity.update(cx, |root, cx| {
                root.command_workflow(
                    &WorkflowCommand::Show { version: 1, run },
                    Some(&origin.to_string()),
                    answer,
                    cx,
                )
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let shown = loop {
                entity.update(cx, |root, cx| root.poll_workflows(cx));
                if let Ok(reply) = receive.try_recv() {
                    break reply;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert!(
                matches!(shown, Answer::Workflow { reply:WorkflowReply::Snapshot { revision:r,..} } if r == revision)
            );
            entity.update(cx, |root, cx| {
                assert_eq!(root.tabs.len(), 2);
                assert!(
                    matches!(root.active_body(),Some(crate::Body::Process { run:r }) if *r == run)
                );
                assert!(root.focusing_front);
                let pane = root
                    .tabs
                    .iter()
                    .flat_map(|tab| tab.panes.bodies())
                    .find_map(|(pane, body)| {
                        matches!(body,crate::Body::Process {run:r} if *r==run).then_some(pane)
                    })
                    .unwrap();
                let _ = root.render_process(run, pane, cx);
            });
        }
        // Complete preparation through the same public owning handler/queue,
        // after the selected draft has changed. No direct graph manipulation.
        std::fs::write(
            directory.path().join("task.md"),
            "Final prepared scope and observable cases",
        )
        .unwrap();
        let mut exchange = |request: WorkflowCommand| {
            let (answer, receive) = mpsc::channel();
            entity.update(cx, |root, cx| {
                root.take_command(
                    crate::hooks::server::Asked {
                        command: guild_session::command::Command::Workflow {
                            request,
                            caller: Some(origin.to_string()),
                        },
                        answer,
                    },
                    cx,
                )
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                entity.update(cx, |root, cx| root.poll_workflows(cx));
                if let Ok(Answer::Workflow { reply }) = receive.try_recv() {
                    break reply;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        let prepared = exchange(WorkflowCommand::Status { version: 1, run });
        let WorkflowReply::Snapshot {
            snapshot, revision, ..
        } = prepared
        else {
            panic!("{prepared:?}")
        };
        let context = &snapshot["preparation"]["context"];
        let output = directory
            .path()
            .join(".guild/workflow-runs")
            .join(run.to_string())
            .join("artifacts/note-check.json");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        let check = std::process::Command::new("/bin/test")
            .arg("-s")
            .arg(directory.path().join("task.md"))
            .status()
            .unwrap();
        assert!(check.success());
        std::fs::write(&output, "actual /bin/test -s exited 0").unwrap();
        let evidence: Evidence = serde_json::from_value(serde_json::json!({
            "id":uuid::Uuid::new_v4(),"producer":origin,"contract":context["contract"]["sha256"],"source":context["source"],
            "scenario":"contract","step":"contract","tool":{"program":"/bin/test","version":"POSIX test -s","argv":["-s","task.md"]},
            "observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"actual /bin/test -s exited 0")}],
            "observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}
        })).unwrap();
        let final_reply = exchange(WorkflowCommand::Checkpoint {
            run,
            spec: CheckpointRequest {
                checks: None,
                version: 1,
                request_id: "final-parent-design".into(),
                expected_revision: revision,
                step: "contract".into(),
                outcome: "contract".into(),
                evidence: vec![evidence],
            },
        });
        let WorkflowReply::Receipt { snapshot, .. } = final_reply else {
            panic!("{final_reply:?}")
        };
        assert_eq!(snapshot["nodes"][0]["state"], "finished");
        assert_eq!(snapshot["nodes"][1]["state"], "ready");
        assert_eq!(snapshot["assignments"], serde_json::json!([]));
        assert_eq!(snapshot["reviews"], serde_json::json!([]));
        let accepted: ContractSnapshot =
            serde_json::from_value(snapshot["accepted_contract"].clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(accepted.directory.join("task.md")).unwrap(),
            "Final prepared scope and observable cases"
        );
        let worker = entity.update(cx, |root, _| {
            root.workflows
                .take()
                .and_then(|mut queue| queue.worker.take())
        });
        worker.expect("owned fixture worker").join().unwrap();
    }
    #[test]
    fn next_request_waits_for_the_native_owner_to_acknowledge_the_previous_reply() {
        let mut queue = Queue::new(vec![]).unwrap();
        let (first, one) = mpsc::channel();
        let (second, two) = mpsc::channel();
        for (entry_enabled, answer) in [(true, first), (false, second)] {
            queue
                .enqueue(Job {
                    entry_enabled,
                    trusted_navigation: false,
                    request: WorkflowCommand::Capabilities { version: 1 },
                    presentation: crate::root::process::Presentation::None,
                    caller: None,
                    origin: None,
                    roots: vec![],
                    review_host: None,
                    recovery_records: vec![],
                    caller_pid: None,
                    answer,
                })
                .unwrap();
        }
        let reply = queue
            .completed
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert!(
            queue.completed.try_recv().is_err(),
            "next job must not execute before Root applies its predecessor"
        );
        reply
            .answer
            .send(Answer::Workflow { reply: reply.reply })
            .unwrap();
        assert!(matches!(
            one.try_recv().unwrap(),
            Answer::Workflow {
                reply: WorkflowReply::Capabilities {
                    entry_before_design: true,
                    ..
                }
            }
        ));
        assert!(two.try_recv().is_err());
        queue.acknowledged();
        queue.pump();
        let reply = queue
            .completed
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        reply
            .answer
            .send(Answer::Workflow { reply: reply.reply })
            .unwrap();
        assert!(matches!(
            two.try_recv().unwrap(),
            Answer::Workflow {
                reply: WorkflowReply::Capabilities {
                    entry_before_design: false,
                    ..
                }
            }
        ));
        let worker = queue.worker.take().unwrap();
        drop(queue);
        worker.join().unwrap();
    }
}
