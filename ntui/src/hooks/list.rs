//! [`Hooks::use_list_selection`]: a cursor and viewport over a list.

use crate::hooks::Hooks;
use crate::hooks::state::State;

/// Where the cursor is in a list, and which slice of it is on screen.
///
/// Plain data with no dependency on the renderer, so the awkward cases —
/// an empty list, a viewport too short for any row, a list that shrinks
/// under the cursor — can be reasoned about and tested directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListSelection {
    /// Index of the selected item.
    pub index: usize,
    /// Index of the first visible item.
    pub offset: usize,
}

impl ListSelection {
    /// Move the cursor by `delta`, scrolling only as far as needed to keep
    /// it visible. Stops at both ends rather than wrapping.
    pub fn move_by(&mut self, delta: isize, len: usize, height: usize) {
        if len == 0 {
            *self = ListSelection::default();
            return;
        }
        let last = (len - 1) as isize;
        self.index = (self.index as isize).saturating_add(delta).clamp(0, last) as usize;
        self.scroll_into_view(len, height);
    }

    /// Jump to the first item.
    pub fn to_start(&mut self) {
        *self = ListSelection::default();
    }

    /// Jump to the last item.
    pub fn to_end(&mut self, len: usize, height: usize) {
        if len == 0 {
            *self = ListSelection::default();
            return;
        }
        self.index = len - 1;
        self.scroll_into_view(len, height);
    }

    /// Pull the cursor and viewport back into range after the list changed
    /// size beneath them.
    ///
    /// Lists that are refreshed from somewhere else — a process list, a log
    /// tail, search results — routinely shrink under a cursor that was valid
    /// a moment ago; left alone it points past the end and the viewport
    /// scrolls to blank space.
    pub fn clamp(&mut self, len: usize, height: usize) {
        if len == 0 {
            *self = ListSelection::default();
            return;
        }
        self.index = self.index.min(len - 1);
        self.scroll_into_view(len, height);
    }

    /// The range of items to render.
    ///
    /// Slice the list with this *before* building any row, so a frame costs
    /// what is on screen rather than what is in the list.
    pub fn visible(&self, len: usize, height: usize) -> std::ops::Range<usize> {
        let start = self.offset.min(len);
        let end = start.saturating_add(height).min(len);
        start..end
    }

    /// Move the viewport the minimum distance that puts `index` back on
    /// screen, so scrolling tracks the cursor instead of recentering it.
    fn scroll_into_view(&mut self, len: usize, height: usize) {
        if height == 0 {
            self.offset = 0;
            return;
        }
        // Never leave blank rows below a full list.
        let max_offset = len.saturating_sub(height);

        if self.index < self.offset {
            self.offset = self.index;
        } else if self.index >= self.offset.saturating_add(height) {
            self.offset = self.index + 1 - height;
        }
        self.offset = self.offset.min(max_offset);
    }
}

impl<'a> Hooks<'a> {
    /// A cursor and viewport over a list, persisted across renders.
    ///
    /// Returns a [`State<ListSelection>`]; move it with
    /// [`ListSelection::move_by`] and friends inside `use_input`, and read
    /// [`ListSelection::visible`] to slice the list for rendering.
    ///
    /// The list's length and the viewport height are passed per call rather
    /// than stored, because both change independently of the selection — the
    /// list is refreshed from elsewhere and the height follows the terminal.
    ///
    /// ```
    /// # use ntui::{Component, Element, Hooks, KeyCode, props::TextProps};
    /// # struct L; #[derive(Clone, PartialEq, Default)] struct LProps { items: Vec<String>, height: usize }
    /// # impl Component for L {
    /// #   type Props = LProps;
    /// fn render(props: &LProps, hooks: &mut Hooks) -> Element {
    ///     let selection = hooks.use_list_selection();
    ///     let (len, height) = (props.items.len(), props.height);
    ///
    ///     let s = selection.clone();
    ///     hooks.use_input(move |ev, _| match ev.code {
    ///         KeyCode::Down => s.update(|sel| sel.move_by(1, len, height)),
    ///         KeyCode::Up => s.update(|sel| sel.move_by(-1, len, height)),
    ///         _ => {}
    ///     });
    ///
    ///     let mut cursor = selection.get();
    ///     cursor.clamp(len, height);   // the list may have shrunk
    ///     let window = cursor.visible(len, height);
    ///     Element::fragment(
    ///         props.items[window].iter().map(|item| Element::text(TextProps {
    ///             content: item.clone(), ..Default::default()
    ///         })).collect(),
    ///     )
    /// }
    /// # }
    /// ```
    pub fn use_list_selection(&mut self) -> State<ListSelection> {
        self.use_state(ListSelection::default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHT: usize = 10;

    #[test]
    fn moves_within_the_viewport_without_scrolling() {
        let mut s = ListSelection::default();

        s.move_by(1, 100, HEIGHT);

        assert_eq!(s.index, 1);
        assert_eq!(s.offset, 0, "the row is already on screen");
    }

    #[test]
    fn scrolls_the_minimum_when_the_cursor_leaves_the_bottom() {
        let mut s = ListSelection::default();

        s.move_by(10, 100, HEIGHT);

        assert_eq!(s.index, 10);
        assert_eq!(s.offset, 1, "just enough to bring item 10 into view");
    }

    #[test]
    fn scrolls_back_when_the_cursor_leaves_the_top() {
        let mut s = ListSelection {
            index: 50,
            offset: 45,
        };

        s.move_by(-10, 100, HEIGHT);

        assert_eq!(s.index, 40);
        assert_eq!(s.offset, 40);
    }

    #[test]
    fn stops_at_both_ends_rather_than_wrapping() {
        let mut s = ListSelection::default();

        s.move_by(-5, 100, HEIGHT);
        assert_eq!((s.index, s.offset), (0, 0));

        s.move_by(500, 100, HEIGHT);
        assert_eq!((s.index, s.offset), (99, 90));
    }

    #[test]
    fn survives_an_empty_list() {
        let mut s = ListSelection::default();

        s.move_by(1, 0, HEIGHT);

        assert_eq!(s, ListSelection::default());
        assert_eq!(s.visible(0, HEIGHT), 0..0);
    }

    #[test]
    fn survives_a_viewport_too_short_for_any_row() {
        let mut s = ListSelection::default();

        s.move_by(5, 100, 0);

        assert_eq!(s.visible(100, 0), 0..0);
    }

    #[test]
    fn jumps_to_the_ends() {
        let mut s = ListSelection::default();

        s.to_end(100, HEIGHT);
        assert_eq!((s.index, s.offset), (99, 90));
        assert_eq!(s.visible(100, HEIGHT), 90..100);

        s.to_start();
        assert_eq!((s.index, s.offset), (0, 0));
    }

    #[test]
    fn keeps_the_cursor_on_screen_when_the_list_shrinks_under_it() {
        let mut s = ListSelection {
            index: 90,
            offset: 85,
        };

        s.clamp(20, HEIGHT);

        assert_eq!((s.index, s.offset), (19, 10));
        assert_eq!(s.visible(20, HEIGHT), 10..20);
    }

    #[test]
    fn survives_an_absurd_viewport_height() {
        // `index`/`offset` are public, so a caller can hand this any pair;
        // `offset + height` must not overflow the way `visible` already
        // guards against.
        let mut s = ListSelection {
            index: 1,
            offset: 1,
        };

        s.move_by(0, 10, usize::MAX);

        assert_eq!(s.visible(10, usize::MAX), 0..10);
    }

    #[test]
    fn shows_the_whole_list_when_it_is_shorter_than_the_viewport() {
        let s = ListSelection::default();

        assert_eq!(s.visible(3, HEIGHT), 0..3);
    }

    #[tokio::test]
    async fn the_hook_persists_the_selection_across_renders() {
        use crate::component::Component;
        use crate::element::Element;
        use crate::props::TextProps;
        use crate::testing::TestTerminal;
        use crate::{KeyCode, hooks::Hooks};

        struct L;
        #[derive(Clone, PartialEq, Default)]
        struct LProps;
        impl Component for L {
            type Props = LProps;
            fn render(_: &LProps, hooks: &mut Hooks) -> Element {
                let selection = hooks.use_list_selection();
                let s = selection.clone();
                hooks.use_input(move |ev, _| {
                    if ev.code == KeyCode::Down {
                        s.update(|sel| sel.move_by(1, 100, 10));
                    }
                });
                Element::text(TextProps {
                    content: format!("index={}", selection.get().index),
                    ..Default::default()
                })
            }
        }

        let mut t = TestTerminal::new(20, 1, Element::component::<L>(LProps)).unwrap();
        assert!(t.frame_text().contains("index=0"));

        t.send_key(KeyCode::Down).unwrap();
        t.send_key(KeyCode::Down).unwrap();

        assert!(t.frame_text().contains("index=2"), "{}", t.frame_text());
    }
}
