//! Keep document images at logical size instead of scaling them to heading text.
use gpui::{App, Entity, ImageCache, ObjectFit, StyledImage, Window, prelude::*, px};
use gpui_component::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
    markdown_ast::Node,
};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

#[derive(Clone)]
pub(super) struct InlineImages {
    pub images: Entity<super::Images>,
    pub width: Arc<AtomicU32>,
    pub right: Arc<AtomicU32>,
}
#[derive(Debug)]
pub(super) struct Picture {
    source: String,
    pub alt: String,
    width: Option<f32>,
    height: Option<f32>,
    available: Arc<AtomicU32>,
}
impl MarkdownPlugin for InlineImages {
    fn name(&self) -> &str {
        "guild-image"
    }
    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let picture = match node {
            Node::Image(image) if !image.url.starts_with("data:") => Picture {
                source: image.url.clone(),
                alt: image.alt.clone(),
                width: None,
                height: None,
                available: Arc::new(AtomicU32::new(f32::INFINITY.to_bits())),
            },
            Node::Html(html) => html_picture(&html.value)?,
            _ => return None,
        };
        let alt = picture.alt.clone();
        Some(
            MarkdownNode::new(self.name(), picture)
                .text(alt.clone())
                .accessibility_label(alt),
        )
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        _: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        self.render_picture(node.data::<Picture>()?, window, cx)
            .map(InlineElement::new)
    }
}
impl InlineImages {
    pub(super) fn render_picture(
        &self,
        picture: &Picture,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::Div> {
        let resource = gpui::Resource::Uri(picture.source.clone().into());
        let image = self
            .images
            .update(cx, |images, cx| images.load(&resource, window, cx))?
            .ok()?;
        // Img owns the raster-to-logical scale conversion; RenderImage::size is
        // device pixels and must not be used as a CSS/logical dimension.
        let intrinsic = gpui::img(image.clone()).into_any_element().layout_as_root(
            gpui::size(
                gpui::AvailableSpace::MaxContent,
                gpui::AvailableSpace::MaxContent,
            ),
            window,
            cx,
        );
        let mut width = picture.width.unwrap_or_else(|| {
            picture.height.map_or(f32::from(intrinsic.width), |height| {
                height * f32::from(intrinsic.width) / f32::from(intrinsic.height)
            })
        });
        let mut height = picture
            .height
            .unwrap_or(width * f32::from(intrinsic.height) / f32::from(intrinsic.width));
        let available = f32::from_bits(self.width.load(Ordering::Relaxed)).max(1.);
        let available = available.min(f32::from_bits(picture.available.load(Ordering::Relaxed)));
        let scale = (available / width).min(1.);
        width *= scale;
        height *= scale;
        let measured = picture.available.clone();
        let right = self.right.clone();
        Some(
            gpui::div()
                .relative()
                .child(
                    gpui::img(image)
                        .id("picture")
                        .debug_selector(|| "markdown-inline-image".into())
                        .w(px(width))
                        .h(px(height))
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    gpui::canvas(
                        move |bounds, window, _| {
                            // Inline custom objects are measured at MaxContent. Learn the
                            // visible right edge after ancestor indents/clips are applied.
                            let edge = window
                                .content_mask()
                                .bounds
                                .right()
                                .min(px(f32::from_bits(right.load(Ordering::Relaxed))));
                            let available = f32::from((edge - bounds.left()).max(px(1.)));
                            if measured.swap(available.to_bits(), Ordering::Relaxed)
                                != available.to_bits()
                            {
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
    }
}

// Reuse the already registered HTML grammar. Only a complete img tag is claimed;
// other HTML, including executable markup, remains the component's responsibility.
pub(super) fn html_picture(source: &str) -> Option<Picture> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() || root.named_child_count() != 1 {
        return None;
    }
    let element = root.named_child(0)?;
    let tag = if element.kind() == "element" {
        element.named_child(0)?
    } else {
        element
    };
    if !matches!(tag.kind(), "start_tag" | "self_closing_tag") {
        return None;
    }
    let name = tag.named_child(0)?;
    if !source[name.byte_range()].eq_ignore_ascii_case("img") {
        return None;
    }
    let mut picture = Picture {
        source: String::new(),
        alt: String::new(),
        width: None,
        height: None,
        available: Arc::new(AtomicU32::new(f32::INFINITY.to_bits())),
    };
    for attr in tag
        .named_children(&mut tag.walk())
        .filter(|n| n.kind() == "attribute")
    {
        let name = attr.named_child(0)?;
        let Some(mut value) = attr.named_child(1) else {
            continue;
        };
        if value.kind() == "quoted_attribute_value"
            && let Some(inner) = value.named_child(0)
        {
            value = inner;
        }
        let value = entities(source[value.byte_range()].trim_matches(['"', '\'']))?;
        match source[name.byte_range()].to_ascii_lowercase().as_str() {
            "src" => picture.source = value,
            "alt" => picture.alt = value,
            "width" => picture.width = dimension(&value),
            "height" => picture.height = dimension(&value),
            _ => {}
        }
    }
    (!picture.source.is_empty() && !picture.source.starts_with("data:")).then_some(picture)
}
fn dimension(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.)
}
pub(super) fn entities(value: &str) -> Option<String> {
    let mut result = String::new();
    let mut rest = value;
    while let Some((before, tail)) = rest.split_once('&') {
        result.push_str(before);
        let Some((entity, after)) = tail.split_once(';') else {
            result.push('&');
            result.push_str(tail);
            return Some(result);
        };
        let decoded = match entity {
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            "lt" => '<',
            "gt" => '>',
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                char::from_u32(u32::from_str_radix(&entity[2..], 16).ok()?)?
            }
            _ if entity.starts_with('#') => char::from_u32(entity[1..].parse().ok()?)?,
            // Leave unsupported named entities to the existing HTML renderer.
            _ => return None,
        };
        result.push(decoded);
        rest = after;
    }
    result.push_str(rest);
    Some(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_html_images_keep_the_component_decoder() {
        assert!(
            super::html_picture(r#"<img src="data:image/png;base64,AA==" alt="embedded">"#)
                .is_none()
        );
    }
}
