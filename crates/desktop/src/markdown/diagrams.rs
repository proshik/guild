//! Fenced diagram blocks remain native entities inside TextView's selection/scroll layout.
use crate::diagrams::{
    Engine,
    runtime::{DiagramView, Manager},
};
use gpui::{App, Entity, Window, prelude::*};
use gpui_component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

// Layout belongs to a source occurrence; Manager still deduplicates engine/source work.
pub(super) type Views = Arc<Mutex<HashMap<std::ops::Range<usize>, Entity<DiagramView>>>>;

pub(super) struct Diagrams {
    pub manager: Option<Entity<Manager>>,
    pub views: Views,
    pub width: Arc<std::sync::atomic::AtomicU32>,
    pub right: Arc<std::sync::atomic::AtomicU32>,
}
struct Block {
    span: std::ops::Range<usize>,
    engine: Engine,
    source: String,
}
impl MarkdownPlugin for Diagrams {
    fn name(&self) -> &str {
        "guild-diagram"
    }
    fn is_block(&self) -> bool {
        true
    }
    fn parse(&self, node: &Node, context: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Code(code) = node else { return None };
        let position = node.position()?;
        let span =
            (context.offset() + position.start.offset)..(context.offset() + position.end.offset);
        let engine = match code.lang.as_deref()?.to_ascii_lowercase().as_str() {
            "mermaid" => Engine::Mermaid,
            "plantuml" => Engine::PlantUml,
            _ => return None,
        };
        Some(
            MarkdownNode::new(
                self.name(),
                Block {
                    span,
                    engine,
                    source: code.value.clone(),
                },
            )
            .text(code.value.clone()),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let block = node.data::<Block>().expect("own diagram node");
        let mut row = gpui::div().w_full().min_w_0().flex().flex_col();
        if let Some(manager) = &self.manager {
            let mut views = self.views.lock().unwrap();
            let view = views.entry(block.span.clone()).or_insert_with(|| {
                DiagramView::new(manager.clone(), block.engine, &block.source, cx)
            });
            view.update(cx, |view, _| {
                view.constrain(self.width.clone(), self.right.clone())
            });
            row = row.child(view.clone());
        } else {
            row = row.child(
                "Diagram resources are unavailable. Reinstall Guild; source remains editable.",
            );
        }
        let source = block.source.clone();
        let copy = self
            .manager
            .as_ref()
            .map(|m| m.read(cx).ui.text(crate::i18n::Text::MenuCopy))
            .unwrap_or("Copy");
        row.child(
            gpui_kit::base::Button::new("diagram-copy-source")
                .child(copy)
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(source.clone()))
                }),
        )
        .into_any_element()
    }
}
