//! The picker card: every popup's list, drawn once.
//!
//! Section 6.32 of the design system. A popup answers with a [`Card`] —
//! its head, its rows, and where it is — and everything around those is
//! here: the card itself, occluding (*a hitbox does not hide what is
//! behind it by covering it*) and carrying its own move and release
//! handlers (*occluding takes the mouse away from the window
//! underneath*); the backdrop that closes it; the head a floating card is
//! dragged by and the edges it is resized from; the scrollbar; and, for a
//! card that hangs from a button, `deferred` and `anchored` (*an
//! absolutely placed popup is still painted in its own place in the
//! tree*).

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, Pixels, Point, Rems, Window, anchored, deferred,
    div, px, rgb, uniform_list,
};
use guild_session::{Grab, PickerPlace, Settings};

use crate::Root;
use crate::picker::Take;
use crate::root::picker::{Focus, Holds, Scroll};
use crate::theme;

/// Where a card is.
#[derive(Clone, Copy)]
pub(crate) enum Place {
    /// Hanging from a button, whose canvas recorded where it was drawn —
    /// the run list, the branch popup, the font list.
    Hanging {
        /// The button's bottom left, in window coordinates.
        from: Point<Pixels>,
        /// The card's width.
        width: Rems,
    },
    /// Over the middle of the window, moved by its head and remembered.
    Floating {
        /// The settings field a drag and a resize write — so the drop
        /// needs no `match` on which popup it was.
        remembered: fn(&mut Settings) -> &mut PickerPlace,
        /// How far down an unmoved card starts, in pixels.
        top: f32,
        /// `None` for a card a person resizes, whose size is remembered
        /// with its place; a width and a greatest height for one that is
        /// the design system's size and is only moved.
        size: Option<(Rems, Rems)>,
    },
}

/// What a popup answers when asked what it draws.
pub(crate) struct Card {
    /// The card's element id.
    pub id: &'static str,
    /// Where it is.
    pub place: Place,
    /// Whether anything behind it closes it: a dimmed layer for a floating
    /// card, an invisible one for a hanging card. A pinned card has none.
    pub backdrop: bool,
    /// The strip at the top — the field, and for a floating card the
    /// handle it is dragged by.
    pub head: AnyElement,
    /// The list, or the note that says why there is none.
    pub rows: AnyElement,
    /// What the lit row looks like, under the rows.
    pub preview: Option<AnyElement>,
    /// A line under everything.
    pub foot: Option<AnyElement>,
}

impl Root {
    /// The popup that is up, if one is.
    pub(crate) fn render_picker(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Root>,
    ) -> Option<AnyElement> {
        let open = self.picker.as_ref()?;
        let focus = open.focus.clone();
        let card = match open.holds {
            Holds::RecentProjects => self.recent_projects_card(cx),
            Holds::RecentFiles(_) => self.recent_files_card(cx),
            Holds::Run => self.run_card(cx),
            Holds::Branches(_) => self.branches_card(cx),
            Holds::Fonts(..) => self.fonts_card(cx),
            Holds::Everywhere(_) => self.everywhere_card(cx),
            Holds::FindInFiles(_) => self.find_in_files_card(cx),
        };
        if let Some(open) = self.picker.as_mut() {
            open.place = Some(card.place);
        }
        let theme = self.theme;

        let body = crate::view::popup::card_look(theme)
            .id(card.id)
            .track_focus(&focus)
            .key_context("picker")
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if root.picker_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .occlude()
            .flex()
            .flex_col()
            .overflow_hidden();

        match card.place {
            Place::Hanging { from, width } => {
                let size = self.viewport;
                let body = body
                    .w(width)
                    .max_w(size.width - px(16.0))
                    .max_h(size.height - px(16.0))
                    .child(card.head)
                    .child(card.rows)
                    .children(card.preview)
                    .children(card.foot);
                Some(
                    deferred(
                        // At the window's own corner: the layer is the
                        // whole window, and this element's place in the
                        // tree — after the status bar — is nowhere near it.
                        anchored().position(Point::default()).child(
                            div()
                                .w(size.width)
                                .h(size.height)
                                .when(card.backdrop, |layer| {
                                    layer.occlude().on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|root, _event, window, cx| {
                                            root.close_picker(Focus::Back, window, cx)
                                        }),
                                    )
                                })
                                .child(
                                    anchored()
                                        .anchor(gpui::Anchor::TopLeft)
                                        .position(from)
                                        .snap_to_window_with_margin(px(8.0))
                                        .child(body),
                                ),
                        ),
                    )
                    .with_priority(1)
                    .into_any_element(),
                )
            }
            Place::Floating { size, .. } => {
                let (x, y, width, height) = self.picker_frame(card.place)?;
                let held = self.picker_held();
                let body = body
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    // The scrolling rows fill this card. A maximum alone leaves
                    // their percentage height indefinite and collapses the list.
                    // picker_frame also clamps fixed cards to the current window.
                    .w(px(width))
                    .h(px(height))
                    // The card occludes the window, so it follows the hand
                    // itself — and hears the release, or letting go over
                    // it never ends the drag.
                    .on_mouse_move(
                        cx.listener(|root, event: &gpui::MouseMoveEvent, window, cx| {
                            root.follow_pointer(event, window, cx)
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|root, _event, _window, cx| root.drop_picker(cx)),
                    )
                    .child(
                        div()
                            .id("picker-head")
                            .debug_selector(|| "picker-head".into())
                            .flex_none()
                            .cursor(if held {
                                CursorStyle::ClosedHand
                            } else {
                                CursorStyle::OpenHand
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|root, event: &gpui::MouseDownEvent, _window, cx| {
                                    cx.stop_propagation();
                                    root.grab_picker(Grab::Move, event.position, cx);
                                }),
                            )
                            .child(card.head),
                    )
                    .child(card.rows)
                    .children(card.preview)
                    .children(card.foot)
                    .when(size.is_none(), |body| {
                        body.children(EDGES.map(|(grab, id)| self.render_picker_edge(grab, id, cx)))
                    });
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .when(card.backdrop, |layer| {
                            layer.child(
                                div()
                                    .id("picker-backdrop")
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .bg(dim(theme.surface.backdrop))
                                    .on_click(cx.listener(|root, _event, window, cx| {
                                        root.close_picker(Focus::Back, window, cx)
                                    })),
                            )
                        })
                        // A grab belongs to the window until release. Child inputs
                        // deliberately stop bubble events; occlusion also hides
                        // the root hitbox. Hear the active gesture in capture.
                        .child({
                            let root = cx.entity().downgrade();
                            gpui::canvas(
                                |_, _, _| {},
                                move |_, _, window, _| {
                                    let motion = root.clone();
                                    window.on_mouse_event(
                                        move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                            if phase != gpui::DispatchPhase::Capture {
                                                return;
                                            }
                                            let held = motion
                                                .update(cx, |root, cx| {
                                                    if !root.picker_held() {
                                                        return false;
                                                    }
                                                    if event.pressed_button
                                                        == Some(MouseButton::Left)
                                                    {
                                                        root.drag_picker(event.position, cx);
                                                    } else {
                                                        root.drop_picker(cx);
                                                    }
                                                    true
                                                })
                                                .unwrap_or(false);
                                            if held {
                                                cx.stop_propagation();
                                            }
                                        },
                                    );
                                    let release = root.clone();
                                    window.on_mouse_event(
                                        move |event: &gpui::MouseUpEvent, phase, _, cx| {
                                            if phase != gpui::DispatchPhase::Capture
                                                || event.button != MouseButton::Left
                                            {
                                                return;
                                            }
                                            let held = release
                                                .update(cx, |root, cx| {
                                                    if !root.picker_held() {
                                                        return false;
                                                    }
                                                    root.drag_picker(event.position, cx);
                                                    root.drop_picker(cx);
                                                    true
                                                })
                                                .unwrap_or(false);
                                            if held {
                                                cx.stop_propagation();
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_full()
                        })
                        .child(body)
                        .into_any_element(),
                )
            }
        }
    }

    /// A short list: a scrolling `div` of the rows given, with its
    /// scrollbar — decision 4's second shape.
    ///
    /// `most` bounds a card with no height of its own — a hanging one —
    /// and a floating card's list takes what is left of the card instead.
    /// Each row is made with [`Root::picker_item`].
    pub(crate) fn picker_children(
        &self,
        id: &'static str,
        rows: Vec<AnyElement>,
        most: Option<Rems>,
        _cx: &mut Context<Root>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(open) = self.picker.as_ref() else {
            return div().into_any_element();
        };
        let Scroll::Children(handle) = &open.scroll else {
            return div().into_any_element();
        };

        let list = div()
            .id(id)
            .overflow_y_scroll()
            .track_scroll(handle)
            .flex()
            .flex_col()
            .py(theme::space::MENU_PADDING)
            .children(rows);

        div()
            .relative()
            // **`min_h_0`, or nothing below matters**: a flex item's
            // automatic minimum size is its own content, and a list
            // taller than the card would push it open instead of
            // scrolling.
            .min_h_0()
            .map(|area| match most {
                Some(most) => area.child(list.max_h(most).min_h_0()),
                None => area.flex_1().child(list.size_full()),
            })
            .when_some(open.scroll.thumb(), |area, (top, length)| {
                area.child(
                    div()
                        .absolute()
                        .right(px(2.0))
                        .top(px(top))
                        .w(theme::space::SCROLLBAR)
                        .h(px(length))
                        .rounded_full()
                        .bg(rgb(theme.text.faint)),
                )
            })
            .into_any_element()
    }

    /// A row of a short list: the pointer lights it, a click takes it.
    ///
    /// No hover tint — the pointer moves the one highlight.
    pub(crate) fn picker_item(
        &self,
        row: usize,
        item: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        item.on_mouse_move(cx.listener(move |root, _event, _window, cx| {
            root.picker_point(row, cx);
        }))
        .on_click(cx.listener(move |root, _event, window, cx| {
            // A gesture aimed at the row: without this the backdrop
            // behind counts the click too.
            cx.stop_propagation();
            root.picker_take(row, Take::Click, window, cx);
        }))
    }

    /// A long list: one `uniform_list` of rows `row` high — decision 4's
    /// first shape, for lists of tens of thousands, which a `div` of
    /// children would lay out every frame.
    #[allow(
        dead_code,
        reason = "the first long list is Search Everywhere's or Find in Files', after the seams"
    )]
    pub(crate) fn picker_rows(
        &self,
        id: &'static str,
        count: usize,
        row: Rems,
        draw: impl Fn(&mut Root, usize, bool, &mut Window, &mut Context<Root>) -> AnyElement + 'static,
        cx: &mut Context<Root>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(open) = self.picker.as_ref() else {
            return div().into_any_element();
        };
        let Scroll::Uniform(handle) = &open.scroll else {
            return div().into_any_element();
        };

        let list = uniform_list(
            id,
            count,
            cx.processor(move |root, range: std::ops::Range<usize>, window, cx| {
                let lit = root.picker_lit_row();
                range
                    .map(|index| {
                        div()
                            .id((id, index))
                            .w_full()
                            .h(row)
                            .on_mouse_move(cx.listener(move |root, _event, _window, cx| {
                                root.picker_point(index, cx);
                            }))
                            .on_click(cx.listener(move |root, _event, window, cx| {
                                cx.stop_propagation();
                                root.picker_take(index, Take::Click, window, cx);
                            }))
                            .child(draw(root, index, lit == Some(index), window, cx))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(handle)
        .size_full()
        .py(theme::space::MENU_PADDING);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(list)
            .when_some(open.scroll.thumb(), |area, (top, length)| {
                area.child(
                    div()
                        .absolute()
                        .right(px(2.0))
                        .top(px(top))
                        .w(theme::space::SCROLLBAR)
                        .h(px(length))
                        .rounded_full()
                        .bg(rgb(theme.text.faint)),
                )
            })
            .into_any_element()
    }

    /// The field every popup's head holds: what has been typed, the caret,
    /// and — only while nothing has been — the placeholder.
    ///
    /// **The order is the point.** A placeholder is not text a person
    /// typed, so the caret does not stand after it: with the caret last it
    /// sat at the end of the hint and jumped left at the first keystroke.
    /// The caret is drawn in the ground's colour when it is off rather
    /// than taken away, or the line would twitch twice a second.
    pub(crate) fn picker_field(
        &self,
        placeholder: String,
        _ground: u32,
        cx: &mut Context<Root>,
    ) -> gpui::Div {
        let open = self.picker.as_ref().expect("visible picker");
        div().flex_1().min_w_0().child(
            crate::text_field::TextField::new(
                "picker-query",
                self.picker_query(),
                cx.listener(|root, value: &String, window, cx| {
                    if let Some(open) = root.picker.as_mut() {
                        open.picker.set_query(value.clone());
                    }
                    root.holds_edited(window, cx);
                    root.settle_picker();
                    cx.notify();
                }),
            )
            .placeholder(placeholder)
            .focus(&open.focus)
            .selected(open.picker.is_prefilled())
            .list()
            .on_key(cx.listener(|root, event, window, cx| {
                root.picker_key(event, window, cx);
            })),
        )
    }

    /// One edge or corner of a floating card, as something to take hold
    /// of. Absolute rather than part of the layout, or it would push the
    /// content in by its own width.
    fn render_picker_edge(
        &self,
        grab: Grab,
        id: &'static str,
        cx: &mut Context<Root>,
    ) -> AnyElement {
        let corner = matches!(grab, Grab::BottomLeft | Grab::BottomRight);
        let thick = if corner { CORNER } else { GRIP };

        let edge = div().id(id).absolute().cursor(match grab {
            Grab::Left | Grab::Right => CursorStyle::ResizeLeftRight,
            Grab::Bottom => CursorStyle::ResizeUpDown,
            Grab::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
            Grab::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            Grab::Move => CursorStyle::OpenHand,
        });

        let edge = match grab {
            Grab::Left => edge.left_0().top_0().w(px(thick)).h_full(),
            Grab::Right => edge.right_0().top_0().w(px(thick)).h_full(),
            Grab::Bottom => edge.bottom_0().left_0().h(px(thick)).w_full(),
            Grab::BottomLeft => edge.bottom_0().left_0().size(px(thick)),
            Grab::BottomRight => edge.bottom_0().right_0().size(px(thick)),
            Grab::Move => edge.top_0().left_0().w_full().h(px(thick)),
        };

        edge.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, event: &gpui::MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                root.grab_picker(grab, event.position, cx);
            }),
        )
        .into_any_element()
    }
}

/// Every part of a floating card a hand can take hold of to resize it.
/// The head is not here: it is the popup's own strip, wrapped.
const EDGES: [(Grab, &str); 5] = [
    (Grab::Left, "picker-left"),
    (Grab::Right, "picker-right"),
    (Grab::Bottom, "picker-bottom"),
    (Grab::BottomLeft, "picker-corner-left"),
    (Grab::BottomRight, "picker-corner-right"),
];

/// How wide an edge a person aims at is: the panel borders' five pixels.
const GRIP: f32 = 5.0;

/// A corner is bigger, because it is aimed at deliberately.
const CORNER: f32 = 14.0;

/// The backdrop, dimmed rather than opaque: the popup answers a question
/// about what is behind it, and hiding that would hide the context.
fn dim(colour: u32) -> gpui::Hsla {
    let mut shade: gpui::Hsla = gpui::rgb(colour).into();
    shade.a = 0.32;
    shade
}
