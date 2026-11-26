//! Native Markdown image resolution through GPUI's public image-cache boundary.
use gpui::{
    App, AppContext, Entity, ImageCache, ImageCacheError, RenderImage, Resource,
    RetainAllImageCache, Window,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// A document's images keep their base directory even when another pane is active.
pub struct Images {
    directory: PathBuf,
    cache: Entity<RetainAllImageCache>,
    remote: std::collections::HashMap<String, remote::Remote>,
}
impl Images {
    /// Create a cache belonging to one Markdown document.
    pub fn new(path: &Path, cx: &mut App) -> Entity<Self> {
        let cache = RetainAllImageCache::new(cx);
        let images = cx.new(|_| Self {
            directory: path.parent().unwrap_or(Path::new("/")).to_owned(),
            cache,
            remote: Default::default(),
        });
        cx.observe_release(&images, |images, cx| {
            for remote in images.remote.values() {
                if let Some(Ok(image)) = remote.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        images
    }
}
impl ImageCache for Images {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        if let Resource::Uri(uri) = resource
            && (uri.as_ref().starts_with("http://") || uri.as_ref().starts_with("https://"))
        {
            if self
                .remote
                .get(uri.as_ref())
                .is_some_and(remote::Remote::retry)
            {
                self.remote.remove(uri.as_ref());
            }
            return self
                .remote
                .entry(uri.to_string())
                .or_insert_with(|| remote::Remote::start(uri.to_string(), window, cx))
                .get();
        }
        let resource = match resource {
            Resource::Uri(uri)
                if (!uri.as_ref().contains(":") && !uri.as_ref().starts_with("//"))
                    || uri.as_ref().starts_with("file:") =>
            {
                let path = gpui::http_client::Url::from_directory_path(&self.directory)
                    .ok()
                    .and_then(|base| base.join(uri.as_ref()).ok())
                    .and_then(|url| url.to_file_path().ok());
                match path {
                    Some(path) => path.into(),
                    None => {
                        return Some(Err(ImageCacheError::Asset(
                            "Invalid local image path".into(),
                        )));
                    }
                }
            }
            _ => resource.clone(),
        };
        self.cache
            .update(cx, |cache, cx| cache.load(&resource, window, cx))
    }
}

mod cached;
mod diagrams;
mod html_image;
mod inline_image;
mod pane;
pub use pane::{Controls, ModeChanged, PREVIEW_LIMIT, Pane, Presentation};

/// Conservative table/code extent used only to enforce the preview resource limit.
/// Uses Guild's existing block grammar on buffer invalidation, never on frames.
/// The component owns independent per-block scrolling and viewport layout.
pub fn minimum_columns(source: &str) -> usize {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .is_err()
    {
        return 0;
    }
    let Some(tree) = parser.parse(source, None) else {
        return 0;
    };
    let mut pending = vec![tree.root_node()];
    let mut columns = 0;
    while let Some(node) = pending.pop() {
        match node.kind() {
            "pipe_table" => {
                let mut widths: Vec<usize> = Vec::new();
                let mut weights: Vec<usize> = Vec::new();
                for row in node.named_children(&mut node.walk()) {
                    if !matches!(row.kind(), "pipe_table_header" | "pipe_table_row") {
                        continue;
                    }
                    let cells: Vec<_> = row
                        .named_children(&mut row.walk())
                        .filter(|n| n.kind() == "pipe_table_cell")
                        .collect();
                    widths.resize(widths.len().max(cells.len()), 0);
                    weights.resize(widths.len(), 5);
                    for (index, cell) in cells.into_iter().enumerate() {
                        let text = source[cell.byte_range()].trim();
                        widths[index] = widths[index].max(text.chars().count() + 4);
                        // TextView allocates column shares by UTF-8 length, capped
                        // at 150 (node.rs::render_table). Honor those shares while
                        // measuring content as characters, not bytes.
                        weights[index] = weights[index].max(text.len().clamp(5, 150));
                    }
                }
                let total: usize = weights.iter().sum();
                for (width, weight) in widths.into_iter().zip(weights) {
                    columns = columns.max(width.saturating_mul(total).div_ceil(weight));
                }
            }
            "code_fence_content" | "indented_code_block" => {
                columns = columns.max(
                    source[node.byte_range()]
                        .lines()
                        .map(|line| line.chars().count().saturating_add(4))
                        .max()
                        .unwrap_or(0),
                );
            }
            _ => pending.extend(node.named_children(&mut node.walk())),
        }
    }
    columns
}

mod remote;
