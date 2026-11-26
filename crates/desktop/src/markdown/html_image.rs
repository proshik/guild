//! Image-only HTML blocks use the same sizing/cache as Markdown inline images.
use super::inline_image::{InlineImages, Picture, entities, html_picture};
use gpui::{App, Window, prelude::*};
use gpui_component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};

pub(super) struct HtmlImages(pub InlineImages);
struct Row {
    images: Vec<(Picture, Option<String>)>,
    align: String,
}
impl MarkdownPlugin for HtmlImages {
    fn name(&self) -> &str {
        "guild-html-images"
    }
    fn is_block(&self) -> bool {
        true
    }
    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Html(html) = node else {
            return None;
        };
        let row = parse_row(&html.value)?;
        let text = row
            .images
            .iter()
            .map(|(image, _)| image.alt.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Some(MarkdownNode::new(self.name(), row).text(text))
    }
    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let row = node.data::<Row>().expect("own HTML image node");
        gpui::div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .when(row.align == "center", |el| el.justify_center())
            .when(row.align == "right", |el| el.justify_end())
            .children(row.images.iter().enumerate().map(|(ix, (picture, link))| {
                let image = self
                    .0
                    .render_picture(picture, window, cx)
                    .unwrap_or_else(|| gpui::div().child(picture.alt.clone()));
                gpui::div()
                    .id(ix)
                    .child(image)
                    .when_some(link.clone(), |el, link| {
                        el.cursor_pointer()
                            .on_click(move |_, _, cx| cx.open_url(&link))
                    })
            }))
    }
}

fn parse_row(source: &str) -> Option<Row> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    let mut row = Row {
        images: Vec::new(),
        align: String::new(),
    };
    collect(tree.root_node(), source, None, &mut row)?;
    (!row.images.is_empty()).then_some(row)
}

// Claim only image rows. Mixed prose, unsupported tags and embedded data images
// stay with the component's HTML parser instead of losing content or decoding.
fn collect(
    node: tree_sitter::Node<'_>,
    source: &str,
    link: Option<String>,
    row: &mut Row,
) -> Option<()> {
    match node.kind() {
        "document" => {
            collect_children(
                node.named_children(&mut node.walk()).collect(),
                source,
                link,
                row,
            )?;
        }
        "element" => {
            let tag = node.named_child(0)?;
            let name = source[tag.named_child(0)?.byte_range()].to_ascii_lowercase();
            if name == "img" {
                row.images
                    .push((html_picture(&source[tag.byte_range()])?, link));
                return Some(());
            }
            if !matches!(name.as_str(), "p" | "div" | "a") {
                return None;
            }
            let link = if name == "a" {
                attribute(tag, source, "href").or(link)
            } else {
                link
            };
            if let Some(align) = attribute(tag, source, "align") {
                row.align = align.to_ascii_lowercase();
            }
            collect_children(
                node.named_children(&mut node.walk()).skip(1).collect(),
                source,
                link,
                row,
            )?;
        }
        "self_closing_tag" | "start_tag" => row
            .images
            .push((html_picture(&source[node.byte_range()])?, link)),
        "end_tag" => {}
        "text" if source[node.byte_range()].trim().is_empty() => {}
        _ => return None,
    }
    Some(())
}
// A row may have a chain of wrappers, but never flatten separate block rows.
fn collect_children(
    children: Vec<tree_sitter::Node<'_>>,
    source: &str,
    link: Option<String>,
    row: &mut Row,
) -> Option<()> {
    let content = children
        .into_iter()
        .filter(|node| {
            node.kind() != "end_tag"
                && !(node.kind() == "text" && source[node.byte_range()].trim().is_empty())
        })
        .collect::<Vec<_>>();
    let has_block = content.iter().any(|node| {
        node.kind() == "element"
            && node
                .named_child(0)
                .and_then(|tag| tag.named_child(0))
                .is_some_and(|name| {
                    matches!(
                        source[name.byte_range()].to_ascii_lowercase().as_str(),
                        "p" | "div"
                    )
                })
    });
    if has_block && content.len() != 1 {
        return None;
    }
    for child in content {
        collect(child, source, link.clone(), row)?;
    }
    Some(())
}

fn attribute(tag: tree_sitter::Node<'_>, source: &str, key: &str) -> Option<String> {
    for attr in tag
        .named_children(&mut tag.walk())
        .filter(|n| n.kind() == "attribute")
    {
        if !source[attr.named_child(0)?.byte_range()].eq_ignore_ascii_case(key) {
            continue;
        }
        let mut value = attr.named_child(1)?;
        if value.kind() == "quoted_attribute_value"
            && let Some(inner) = value.named_child(0)
        {
            value = inner;
        }
        return entities(source[value.byte_range()].trim_matches(['"', '\'']));
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_html_and_data_images_remain_with_the_component() {
        for source in [
            r#"<p>Text <img src="a.png"></p>"#,
            r#"<div><img src="a.png"><script>ignored()</script></div>"#,
            r#"<p><img src="data:image/png;base64,AA=="></p>"#,
            r#"<div><p><img src="a.png"></p><p><img src="b.png"></p></div>"#,
            r#"<p><img src="a.png"></p><img src="b.png">"#,
        ] {
            assert!(super::parse_row(source).is_none(), "{source}");
        }
    }
    #[test]
    fn image_row_preserves_links_and_alt_text() {
        let row = super::parse_row(r#"<p align="center"><a href="https://example.com/?a=1&amp;b=2"><img src="a.svg" width="100" height="20" alt="first"></a><img src="b.png" alt="second"></p>"#).unwrap();
        assert_eq!(row.align, "center");
        assert_eq!(row.images.len(), 2);
        assert_eq!(
            row.images[0].1.as_deref(),
            Some("https://example.com/?a=1&b=2")
        );
        assert_eq!(row.images[0].0.alt, "first");
        assert_eq!(row.images[1].0.alt, "second");
    }
}
