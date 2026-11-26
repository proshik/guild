//! Retain TextView itself, including its initialized parse and scroll/selection state.
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Window,
};
use gpui_component::text::{TextView, TextViewState};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(super) struct CachedTextView {
    view: Rc<RefCell<TextView>>,
    list: gpui::ListState,
    image_width: Arc<AtomicU32>,
    image_right: Arc<AtomicU32>,
    state: gpui::Entity<TextViewState>,
    selection: gpui::Hsla,
}
impl CachedTextView {
    pub(super) fn new(
        view: TextView,
        list: gpui::ListState,
        image_width: Arc<AtomicU32>,
        image_right: Arc<AtomicU32>,
        state: gpui::Entity<TextViewState>,
        selection: gpui::Hsla,
    ) -> Self {
        Self {
            view: Rc::new(RefCell::new(view)),
            list,
            image_width,
            image_right,
            state,
            selection,
        }
    }
}

impl IntoElement for CachedTextView {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for CachedTextView {
    type RequestLayoutState = <TextView as Element>::RequestLayoutState;
    type PrepaintState = <TextView as Element>::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.view.borrow().id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let top = self.list.logical_scroll_top();
        // The component facade resolves its rich-text palette here but has no
        // per-view selection setter. Scope that one public theme token to this
        // layout pass; the editor and sibling panes keep their original theme.
        let previous = gpui_component::Theme::global_mut(cx).colors.selection;
        gpui_component::Theme::global_mut(cx).colors.selection = self.selection;
        let result = self
            .view
            .borrow_mut()
            .request_layout(id, inspector, window, cx);
        gpui_component::Theme::global_mut(cx).colors.selection = previous;
        // TextView resets its list when a replacement changes the block count.
        // Restore the pane's reading anchor after that reset, before prepaint.
        if self.list.item_count() > 0 && (top.item_ix > 0 || top.offset_in_item > gpui::px(0.)) {
            self.list.scroll_to(gpui::ListOffset {
                item_ix: top.item_ix.min(self.list.item_count() - 1),
                offset_in_item: top.offset_in_item,
            });
        }
        result
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.image_right
            .store(f32::from(bounds.right()).to_bits(), Ordering::Relaxed);
        let width = f32::from(bounds.size.width).to_bits();
        if self.image_width.swap(width, Ordering::Relaxed) != width {
            let state = self.state.clone();
            window.defer(cx, move |_, cx| {
                state.update(cx, |state, cx| state.invalidate_inline_layout(cx))
            });
        }
        self.view
            .borrow_mut()
            .prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view
            .borrow_mut()
            .paint(id, inspector, bounds, layout, prepaint, window, cx);
    }
}
