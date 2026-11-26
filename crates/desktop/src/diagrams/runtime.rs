//! The main window owns one serial worker and all retained decoded images.
use super::{Engine, Resources, transport};
use gpui::{App, AppContext, Context, Entity, Image, ImageFormat, RenderImage, Subscription, Task};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

/// Retained decoded pixels: CPU plus GPU <=32 MiB, leaving another 32 MiB for
/// one in-flight decode/upload. Encoded transport has its own independent limit.
pub const CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_REQUESTS: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    engine: Engine,
    source: Arc<str>,
}
impl Key {
    pub fn new(engine: Engine, source: &str) -> Self {
        Self {
            engine,
            source: source.into(),
        }
    }
}
/// Physical pixels own decode/cache accounting; logical points own layout.
#[derive(Clone)]
pub struct DiagramImage {
    pub image: Arc<RenderImage>,
    pub logical: gpui::Size<gpui::Pixels>,
}
impl DiagramImage {
    fn logical_size(reply: &serde_json::Value) -> anyhow::Result<gpui::Size<gpui::Pixels>> {
        let width = reply["logicalWidth"].as_f64().unwrap_or(0.);
        let height = reply["logicalHeight"].as_f64().unwrap_or(0.);
        anyhow::ensure!(
            width.is_finite()
                && height.is_finite()
                && width > 0.
                && height > 0.
                && width <= 4096.
                && height <= 4096.
                && width * height <= 1048576.,
            "Invalid logical diagram dimensions"
        );
        Ok(gpui::size(gpui::px(width as f32), gpui::px(height as f32)))
    }
}
#[derive(Clone)]
pub enum Output {
    Pending,
    Image(DiagramImage),
    Error(String),
}
struct Entry {
    users: usize,
    output: Output,
    bytes: usize,
    touched: u64,
}
enum Work {
    Render(Key, Arc<AtomicBool>),
}
enum Event {
    Render(Key, Result<DiagramImage, String>),
}

pub struct Manager {
    entries: HashMap<Key, Entry>,
    queue: VecDeque<Key>,
    active: Option<(Key, Arc<AtomicBool>)>,
    sender: Option<mpsc::SyncSender<Work>>,
    worker: Option<JoinHandle<()>>,
    _events: Option<Task<()>>,
    events: mpsc::Receiver<Event>,
    bytes: usize,
    clock: u64,
    pub theme: crate::theme::Theme,
    pub ui: crate::i18n::Ui,
}
impl Manager {
    pub fn new(resources: anyhow::Result<Resources>, cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (events, incoming) = mpsc::channel();
        let renderer = cx.svg_renderer();
        let resources = resources.map_err(|e| format!("{e:#}"));
        let worker = std::thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                let Work::Render(key, cancel) = work;
                let result = (|| -> anyhow::Result<_> {
                    let resources = resources.as_ref().map_err(|e| anyhow::anyhow!(e.clone()))?;
                    resources.verify(key.engine)?;
                    let reply = transport::render(
                        &resources.helper,
                        &resources.assets,
                        key.engine.id(),
                        &key.source,
                        &cancel,
                        Duration::from_secs(15),
                    )?;
                    if let Some(error) = reply.get("error").and_then(|e| e.as_str()) {
                        anyhow::bail!("{error}")
                    }
                    use base64::Engine as _;
                    let png = base64::engine::general_purpose::STANDARD.decode(
                        reply
                            .get("png")
                            .and_then(|s| s.as_str())
                            .ok_or_else(|| anyhow::anyhow!("Missing diagram image"))?,
                    )?;
                    let bytes = png_bytes(&png)?;
                    anyhow::ensure!(bytes <= CACHE_BYTES, "Diagram image exceeds memory budget");
                    anyhow::ensure!(!cancel.load(Ordering::Acquire), "Diagram cancelled");
                    let image =
                        Image::from_bytes(ImageFormat::Png, png).to_image_data(renderer.clone())?;
                    anyhow::ensure!(image.frame_count() == 1, "Unexpected animated image");
                    Ok(DiagramImage {
                        image,
                        logical: DiagramImage::logical_size(&reply)?,
                    })
                })()
                .map_err(|e| format!("{e:#}"));
                let event = Event::Render(key, result);
                if events.send(event).is_err() {
                    break;
                }
            }
        });
        cx.on_release(|this, cx| {
            this.shutdown(cx).detach();
        })
        .detach();
        Self {
            entries: HashMap::new(),
            queue: VecDeque::new(),
            active: None,
            sender: Some(sender),
            worker: Some(worker),
            _events: None,
            events: incoming,
            bytes: 0,
            clock: 0,
            theme: Default::default(),
            ui: crate::i18n::Ui(guild_session::Language::English),
        }
    }
    pub fn restyle(
        &mut self,
        theme: crate::theme::Theme,
        ui: crate::i18n::Ui,
        cx: &mut Context<Self>,
    ) {
        // The whole palette, not its kind: a tint changes the colours a
        // diagram is drawn in without changing which palette it is.
        if self.theme != theme || self.ui.0 != ui.0 {
            self.theme = theme;
            self.ui = ui;
            cx.notify();
        }
    }
    pub fn acquire(&mut self, key: Key, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.users += 1;
            return;
        }
        if self.entries.len() >= MAX_REQUESTS {
            self.evict_one(cx);
        }
        if self.entries.len() >= MAX_REQUESTS {
            return;
        }
        self.entries.insert(
            key.clone(),
            Entry {
                users: 1,
                output: Output::Pending,
                bytes: 0,
                touched: self.clock,
            },
        );
        self.queue.push_back(key);
        self.drive(cx);
    }
    pub fn release(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.users = entry.users.saturating_sub(1);
        }
        if self.entries.get(key).is_some_and(|e| e.users == 0) {
            self.queue.retain(|k| k != key);
            if let Some((active, cancel)) = &self.active
                && active == key
            {
                cancel.store(true, Ordering::Release);
            }
            self.remove(key, cx);
        }
    }
    pub fn output(&mut self, key: &Key) -> Output {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.touched = self.clock;
            entry.output.clone()
        } else {
            Output::Error("Diagram preview request limit reached".into())
        }
    }
    fn drive(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() || self.sender.is_none() {
            return;
        }
        if self._events.is_none() {
            self._events = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                    let keep = this.update(cx, |this, cx| {
                        loop {
                            match this.events.try_recv() {
                                Ok(event) => this.receive(event, cx),
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    this.sender.take();
                                    this.active = None;
                                    this.queue.clear();
                                    for entry in this.entries.values_mut() {
                                        if matches!(entry.output, Output::Pending) {
                                            entry.output =
                                                Output::Error("Diagram worker stopped".into());
                                        }
                                    }
                                    cx.notify();
                                    break;
                                }
                            }
                        }
                        if this.active.is_none() {
                            this._events = None;
                            false
                        } else {
                            true
                        }
                    });
                    if !matches!(keep, Ok(true)) {
                        break;
                    }
                }
            }));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let Some(key) = self.queue.pop_front() else {
            return;
        };
        self.active = Some((key.clone(), cancel.clone()));
        let work = Work::Render(key, cancel);
        if self.sender.as_ref().unwrap().send(work).is_err() {
            self.active = None;
            for entry in self.entries.values_mut() {
                if matches!(entry.output, Output::Pending) {
                    entry.output = Output::Error("Diagram worker stopped".into());
                }
            }
        }
    }
    fn receive(&mut self, event: Event, cx: &mut Context<Self>) {
        let cancelled = self
            .active
            .as_ref()
            .is_some_and(|(_, c)| c.load(Ordering::Acquire));
        self.active = None;
        match event {
            Event::Render(key, result) => {
                if !cancelled && self.entries.contains_key(&key) {
                    let output = match result {
                        Ok(image) => {
                            let size = image.image.size(0);
                            let bytes = usize::from(size.width) * usize::from(size.height) * 4;
                            while self.bytes + bytes > CACHE_BYTES {
                                if !self.evict_image(cx) {
                                    break;
                                }
                            }
                            self.bytes += bytes;
                            self.entries.get_mut(&key).unwrap().bytes = bytes;
                            Output::Image(image)
                        }
                        Err(error) => Output::Error(error),
                    };
                    self.entries.get_mut(&key).unwrap().output = output;
                } else if let Ok(image) = result {
                    discard(image, cx);
                }
            }
        }
        self.drive(cx);
        cx.notify();
    }
    fn evict_image(&mut self, cx: &mut Context<Self>) -> bool {
        let key = self
            .entries
            .iter()
            .filter(|(_, e)| e.bytes > 0)
            .min_by_key(|(_, e)| e.touched)
            .map(|(k, _)| k.clone());
        let Some(key) = key else { return false };
        if let Some(entry) = self.entries.get_mut(&key) {
            self.bytes -= entry.bytes;
            entry.bytes = 0;
            let old = std::mem::replace(
                &mut entry.output,
                Output::Error(
                    "Diagram preview memory limit reached. Reopen Preview to render again.".into(),
                ),
            );
            if let Output::Image(image) = old {
                discard(image, cx);
            }
        }
        true
    }
    fn evict_one(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self
            .entries
            .iter()
            .filter(|(_, e)| e.users == 0)
            .min_by_key(|(_, e)| e.touched)
            .map(|(k, _)| k.clone())
        {
            self.remove(&key, cx)
        }
    }
    fn remove(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.bytes;
            if let Output::Image(image) = entry.output {
                discard(image, cx)
            }
        }
    }
    /// Cancel first; root awaits joining this owner before application quit.
    pub fn shutdown(&mut self, cx: &mut App) -> Task<()> {
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Release)
        }
        self.sender.take();
        self._events.take();
        let (_, empty) = mpsc::channel();
        self.events = empty;
        for (_, entry) in self.entries.drain() {
            if let Output::Image(image) = entry.output {
                discard(image, cx)
            }
        }
        self.bytes = 0;
        let worker = self.worker.take();
        cx.background_executor().spawn(async move {
            if let Some(worker) = worker {
                let _ = worker.join();
            }
        })
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Release)
        }
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            std::thread::spawn(move || {
                let _ = worker.join();
            });
        }
    }
}
fn discard(frame: DiagramImage, cx: &mut App) {
    cx.defer(move |cx| {
        cx.drop_image(frame.image, None);
    });
}
/// PNG metadata preflight precedes GPUI decoding of exactly the same immutable bytes.
pub fn png_bytes(bytes: &[u8]) -> anyhow::Result<usize> {
    anyhow::ensure!(
        bytes.len() >= 33
            && bytes.len() <= 12 * 1024 * 1024
            && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
            && &bytes[12..16] == b"IHDR",
        "Invalid diagram PNG"
    );
    let width = u32::from_be_bytes(bytes[16..20].try_into()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into()?);
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= 8192
            && height <= 8192
            && u64::from(width) * u64::from(height) <= 4 * 1024 * 1024,
        "Diagram exceeds pixel budget"
    );
    Ok(width as usize * height as usize * 4)
}
/// A consumer owns its request through source changes, close and manager invalidation.
pub struct DiagramView {
    manager: Entity<Manager>,
    key: Key,
    acquired: bool,
    _observe: Subscription,
    available: Arc<std::sync::atomic::AtomicU32>,
    page_bounds: Option<(
        Arc<std::sync::atomic::AtomicU32>,
        Arc<std::sync::atomic::AtomicU32>,
    )>,
}
impl DiagramView {
    pub(crate) fn constrain(
        &mut self,
        width: Arc<std::sync::atomic::AtomicU32>,
        right: Arc<std::sync::atomic::AtomicU32>,
    ) {
        self.page_bounds = Some((width, right));
    }
    pub fn new(
        manager: Entity<Manager>,
        engine: Engine,
        source: &str,
        cx: &mut App,
    ) -> Entity<Self> {
        let key = Key::new(engine, source);
        let view = cx.new(|cx| Self {
            manager: manager.clone(),
            key: key.clone(),
            acquired: false,
            available: Arc::new(std::sync::atomic::AtomicU32::new(f32::INFINITY.to_bits())),
            page_bounds: None,
            _observe: cx.observe(&manager, |this: &mut Self, _, cx| {
                if !this.manager.read(cx).entries.contains_key(&this.key) {
                    this.acquired = false;
                }
                cx.notify();
            }),
        });
        cx.observe_release(&view, |this, cx| {
            if this.acquired {
                this.manager
                    .update(cx, |manager, cx| manager.release(&this.key, cx));
            }
        })
        .detach();
        view
    }
}
impl gpui::Render for DiagramView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        let theme = self.manager.read(cx).theme;
        let instance = cx.entity_id();
        let mut out = gpui::div()
            .debug_selector(move || format!("diagram-instance-{instance}"))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(crate::theme::space::TREE_PADDING);
        if !self.acquired {
            self.manager
                .update(cx, |manager, cx| manager.acquire(self.key.clone(), cx));
            self.acquired = true;
        }
        match self
            .manager
            .update(cx, |manager, _| manager.output(&self.key))
        {
            Output::Pending => {
                let engine = self.key.engine;
                out = out.debug_selector(move || format!("diagram-pending-{}", engine.id()));
                out = out.child(
                    self.manager
                        .read(cx)
                        .ui
                        .text(crate::i18n::Text::DiagramRendering),
                )
            }
            Output::Error(error) => {
                let engine = self.key.engine;
                out = out.debug_selector(move || format!("diagram-error-{}", engine.id()));
                out = out.child(
                    gpui::div()
                        .debug_selector(|| "diagram-error".into())
                        .text_color(gpui::rgb(theme.accent.rust))
                        .child(error),
                )
            }
            Output::Image(frame) => {
                let natural = frame.logical;
                let mut available = f32::from_bits(self.available.load(Ordering::Relaxed))
                    .min(f32::from(window.viewport_size().width));
                if let Some((width, _)) = &self.page_bounds {
                    available = available.min(f32::from_bits(width.load(Ordering::Relaxed)));
                }
                let scale = (available.max(1.) / f32::from(natural.width)).min(1.);
                let fitted = gpui::size(
                    gpui::px(f32::from(natural.width) * scale),
                    gpui::px(f32::from(natural.height) * scale),
                );
                let measured = self.available.clone();
                let page = self.page_bounds.clone();
                let view = cx.entity_id();
                let engine = self.key.engine;
                out = out.child(
                    gpui::div()
                        .relative()
                        .w_full()
                        .min_w_0()
                        .max_w_full()
                        .debug_selector(move || format!("diagram-image-{}", engine.id()))
                        .child(
                            gpui::img(frame.image)
                                .debug_selector(|| "diagram-image".into())
                                .w(fitted.width)
                                .h(fitted.height)
                                .object_fit(gpui::ObjectFit::Contain),
                        )
                        .child(
                            gpui::canvas(
                                move |bounds, window, cx| {
                                    let mut edge = window.content_mask().bounds.right();
                                    if let Some((_, right)) = &page {
                                        edge = edge.min(gpui::px(f32::from_bits(
                                            right.load(Ordering::Relaxed),
                                        )));
                                    }
                                    let width = f32::from(
                                        (edge - bounds.left())
                                            .min(bounds.size.width)
                                            .max(gpui::px(1.)),
                                    );
                                    if measured.swap(width.to_bits(), Ordering::Relaxed)
                                        != width.to_bits()
                                    {
                                        // Prepaint invalidation cannot dirty this frame in GPUI.
                                        // Wake the corrective frame after drawing has finished.
                                        window.defer(cx, move |window, cx| {
                                            cx.notify(view);
                                            window.refresh();
                                        });
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        ),
                );
            }
        }
        out.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Render, Window, prelude::*};
    struct Picture {
        manager: Entity<Manager>,
        key: Key,
        _watch: Subscription,
    }
    impl Render for Picture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            match self.manager.update(cx, |m, _| m.output(&self.key)) {
                Output::Image(image) => gpui::div()
                    .child(gpui::img(image.image).w(gpui::px(100.)).h(gpui::px(100.)))
                    .into_any_element(),
                _ => gpui::div().into_any_element(),
            }
        }
    }
    #[gpui::test]
    fn decoded_cache_evicts_pixels_and_actual_gpu_atlas(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let mut held = None;
        let a = Key::new(Engine::Mermaid, "first");
        let b = Key::new(Engine::Mermaid, "second");
        let first = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(2048, 2048),
        )]));
        let second = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(2048, 2048),
        )]));
        let (_, cx) = cx.add_window_view(|_, cx| {
            let manager = cx.new(|cx| Manager::new(Ok(Resources::from_directory(dir.path())), cx));
            manager.update(cx, |m, cx| {
                m.shutdown(cx).detach();
                for key in [&a, &b] {
                    m.entries.insert(
                        key.clone(),
                        Entry {
                            users: 1,
                            output: Output::Pending,
                            bytes: 0,
                            touched: 0,
                        },
                    );
                }
                m.receive(
                    Event::Render(
                        a.clone(),
                        Ok(DiagramImage {
                            image: first.clone(),
                            logical: gpui::size(gpui::px(1024.), gpui::px(1024.)),
                        }),
                    ),
                    cx,
                );
            });
            held = Some(manager.clone());
            Picture {
                manager: manager.clone(),
                key: a.clone(),
                _watch: cx.observe(&manager, |_, _, cx| cx.notify()),
            }
        });
        cx.run_until_parked();
        cx.update(|window, _| assert!(window.has_image_atlas_entry(&first)));
        let manager = held.unwrap();
        cx.update(|_, cx| {
            manager.update(cx, |m, cx| {
                m.receive(
                    Event::Render(
                        b.clone(),
                        Ok(DiagramImage {
                            image: second,
                            logical: gpui::size(gpui::px(1024.), gpui::px(1024.)),
                        }),
                    ),
                    cx,
                )
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(
                !window.has_image_atlas_entry(&first),
                "eviction releases actual platform atlas entries even when a CPU Arc remains"
            );
            assert_eq!(manager.read(cx).bytes, CACHE_BYTES);
            assert!(matches!(
                manager.read(cx).entries[&a].output,
                Output::Error(_)
            ));
        });
    }
    struct FrameContainer {
        view: Entity<DiagramView>,
        width: f32,
    }
    impl Render for FrameContainer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
                .w(gpui::px(self.width))
                .overflow_hidden()
                .child(self.view.clone())
        }
    }
    #[gpui::test]
    fn logical_geometry_is_scale_independent_and_fits_both_dimensions(
        cx: &mut gpui::TestAppContext,
    ) {
        for scale in [1, 2] {
            let state = tempfile::tempdir().unwrap();
            let (container, visual) = cx.add_window_view(|_, cx| {
                let manager =
                    cx.new(|cx| Manager::new(Ok(Resources::from_directory(state.path())), cx));
                manager.update(cx, |m, cx| {
                    m.shutdown(cx).detach();
                    let image = Arc::new(RenderImage::new([image::Frame::new(
                        image::RgbaImage::new(1024 * scale, 512 * scale),
                    )]));
                    m.entries.insert(
                        Key::new(Engine::Mermaid, "geometry"),
                        Entry {
                            users: 0,
                            output: Output::Image(DiagramImage {
                                image,
                                logical: gpui::size(gpui::px(1024.), gpui::px(512.)),
                            }),
                            bytes: 0,
                            touched: 0,
                        },
                    );
                });
                FrameContainer {
                    view: DiagramView::new(manager, Engine::Mermaid, "geometry", cx),
                    width: 400.,
                }
            });
            visual.run_until_parked();
            assert_eq!(
                visual.debug_bounds("diagram-image").unwrap().size,
                gpui::size(gpui::px(400.), gpui::px(200.)),
                "first appearance must settle without manual draws at {scale}x"
            );
            visual.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(1000.)));
            for (width, expected) in [
                (1400., (1024., 512.)),
                (400., (400., 200.)),
                (200., (200., 100.)),
            ] {
                visual.update(|_, cx| {
                    container.update(cx, |c, cx| {
                        c.width = width;
                        cx.notify();
                    })
                });
                visual.run_until_parked();
                assert_eq!(
                    visual.debug_bounds("diagram-image").unwrap().size,
                    gpui::size(gpui::px(expected.0), gpui::px(expected.1)),
                    "resize to {width} must schedule its own correction at {scale}x"
                );
            }
        }
    }
    #[test]
    fn png_canvas_budget_rejects_oversized_dimensions_before_decode() {
        let mut bytes = vec![0; 33];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&4096u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&4096u32.to_be_bytes());
        assert!(png_bytes(&bytes).is_err());
    }
}
