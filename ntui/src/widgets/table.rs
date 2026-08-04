use std::rc::Rc;

use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use crate::style::{Color, Weight};

/// Which slice of a table's rows to lay out.
///
/// Without one, a table builds and lays out every row it is given, whether
/// or not the row is on screen — fine for a dozen rows, and the dominant
/// cost of a frame once there are hundreds. With one, *building and laying
/// out* rows becomes proportional to `height` instead of to `rows.len()`.
///
/// The props comparison the reconciler performs is not: `TableProps`
/// derives `PartialEq` over `rows: Vec<Vec<String>>`, so a re-render of the
/// parent still compares every row. Wrap the source data in
/// [`Shared`](crate::Shared) on your side and slice into it if that
/// comparison shows up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    /// Index of the first row to render.
    pub offset: usize,
    /// How many rows to render.
    pub height: usize,
}

impl Viewport {
    pub fn new(offset: usize, height: usize) -> Self {
        Viewport { offset, height }
    }

    /// The row range this viewport covers, clamped to `len`.
    pub fn range(&self, len: usize) -> std::ops::Range<usize> {
        let start = self.offset.min(len);
        let end = start.saturating_add(self.height).min(len);
        start..end
    }
}

/// Where a cell sits, and what it holds — the input to a [`CellStyler`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellContext<'a> {
    /// Index into `rows`, not into the visible window.
    pub row: usize,
    pub column: usize,
    pub value: &'a str,
    /// Whether this cell's row is the selected one.
    pub selected: bool,
}

/// A per-cell style override. Fields left `None` keep the table's own choice
/// for that cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub weight: Option<Weight>,
}

impl CellStyle {
    pub fn color(color: Color) -> Self {
        CellStyle {
            color: Some(color),
            ..Default::default()
        }
    }

    pub fn background(background: Color) -> Self {
        CellStyle {
            background: Some(background),
            ..Default::default()
        }
    }

    pub fn bold(mut self) -> Self {
        self.weight = Some(Weight::Bold);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_background(mut self, background: Color) -> Self {
        self.background = Some(background);
        self
    }
}

/// Decides a cell's style from its position and contents.
///
/// A uniformly styled table cannot say "this number is alarming" or "this
/// row is a different kind of thing", which is most of what turns a grid of
/// strings into something readable at a glance.
///
/// Compared by pointer identity, like
/// [`Callback`](crate::widgets::Callback): a closure rebuilt inline on every
/// render compares unequal to the previous one, which costs this widget its
/// props-equality fast path but nothing else. Hoist it into a
/// [`use_memo`](crate::Hooks::use_memo) if that matters.
#[derive(Clone)]
pub struct CellStyler(Rc<dyn Fn(CellContext) -> CellStyle>);

impl CellStyler {
    pub fn new(f: impl Fn(CellContext) -> CellStyle + 'static) -> Self {
        CellStyler(Rc::new(f))
    }

    pub fn style(&self, cell: CellContext) -> CellStyle {
        (self.0)(cell)
    }
}

impl PartialEq for CellStyler {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for CellStyler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CellStyler(..)")
    }
}

/// A table.
///
/// Column widths default to the widest of that column's header and cell
/// contents, plus one cell of breathing room; override per-column via
/// `widths` (a `0` entry falls back to the default for that column). Rows
/// may be ragged — cells past `headers.len()` still get content-sized
/// columns (with no header above them).
///
/// Three optional props turn it from a static grid into a list a user can
/// work with:
///
/// - `selected` marks one row, drawn in the theme's accent.
/// - `viewport` renders only a slice, so a long table costs what is on
///   screen rather than what is in the list.
/// - `cell_style` overrides colors and weight per cell.
///
/// The table is not focusable and reads no input of its own: drive
/// `selected` from [`use_list_selection`](crate::Hooks::use_list_selection),
/// which owns the cursor and the scroll offset and handles the awkward edges
/// (empty list, viewport shorter than a row, list shrinking underneath).
///
/// With a `viewport` set, columns are measured from the *visible* rows, so
/// both their widths and — with ragged rows — their **count** can change as
/// the user scrolls: a row with an extra cell takes its column with it when
/// it leaves the window. Set `widths` explicitly to pin the widths, and
/// give every row the same number of cells to pin the count.
#[derive(Clone, PartialEq, Default)]
pub struct TableProps {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub widths: Vec<u16>,
    /// Index into `rows` of the selected row, if any.
    pub selected: Option<usize>,
    /// Which slice of `rows` to render. `None` renders all of them.
    pub viewport: Option<Viewport>,
    /// Per-cell style override.
    pub cell_style: Option<CellStyler>,
}

pub struct Table;
impl Component for Table {
    type Props = TableProps;
    fn render(props: &TableProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();

        let window = match props.viewport {
            Some(viewport) => viewport.range(props.rows.len()),
            None => 0..props.rows.len(),
        };
        let visible = &props.rows[window.clone()];

        // Cover the widest visible row too, not just `headers`, so ragged
        // rows get content-sized columns instead of an arbitrary fallback.
        let cols = visible
            .iter()
            .map(|r| r.len())
            .fold(props.headers.len(), usize::max);
        let col_widths: Vec<u16> = (0..cols)
            .map(|i| {
                if let Some(w) = props.widths.get(i).copied().filter(|w| *w > 0) {
                    return w;
                }
                let header_len = props.headers.get(i).map(|h| h.chars().count()).unwrap_or(0);
                let max_cell_len = visible
                    .iter()
                    .filter_map(|r| r.get(i))
                    .map(|c| c.chars().count())
                    .max()
                    .unwrap_or(0);
                // Cap before the u16 cast: a >65534-char cell must saturate,
                // not wrap (release) or overflow-panic on the +1 (debug).
                header_len.max(max_cell_len).min(u16::MAX as usize - 1) as u16 + 1
            })
            .collect();
        let col_width = |i: usize| -> u16 { col_widths[i] };

        let cell = |content: &str, width: u16, color: Color, weight: Weight, bg: Color| {
            Element::view(
                ViewProps {
                    width: Dimension::Cells(width),
                    height: Dimension::Cells(1),
                    background: bg,
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: content.to_string(),
                    color,
                    weight,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                })],
            )
        };

        let header_cells = props
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| cell(h, col_width(i), theme.accent, Weight::Bold, theme.surface))
            .collect();
        let mut children = Vec::with_capacity(1 + visible.len());
        children.push(Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
            header_cells,
        ));

        for (offset, row) in visible.iter().enumerate() {
            // Indices are into `rows`, not into the window, so `selected`
            // and a styler mean the same thing however the table is
            // scrolled.
            let r = window.start + offset;
            let selected = props.selected == Some(r);

            let base_bg = if selected {
                theme.accent
            } else if r % 2 == 0 {
                Color::Reset
            } else {
                theme.surface
            };
            let base_fg = if selected {
                theme.surface
            } else {
                theme.foreground
            };

            let cells = row
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let style = props
                        .cell_style
                        .as_ref()
                        .map(|styler| {
                            styler.style(CellContext {
                                row: r,
                                column: i,
                                value: v,
                                selected,
                            })
                        })
                        .unwrap_or_default();
                    cell(
                        v,
                        col_width(i),
                        style.color.unwrap_or(base_fg),
                        style.weight.unwrap_or(Weight::Normal),
                        style.background.unwrap_or(base_bg),
                    )
                })
                .collect();
            children.push(Element::view(
                ViewProps {
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                },
                cells,
            ));
        }

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            children,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Node;
    use crate::testing::{TestTerminal, render_once};

    fn rows(n: usize) -> Vec<Vec<String>> {
        (0..n).map(|i| vec![format!("row-{i:03}")]).collect()
    }

    #[tokio::test]
    async fn renders_headers_and_rows() {
        let t = TestTerminal::new(
            30,
            3,
            Element::component::<Table>(TableProps {
                headers: vec!["name".into(), "age".into()],
                rows: vec![
                    vec!["ada".into(), "36".into()],
                    vec!["alan".into(), "41".into()],
                ],
                ..Default::default()
            }),
        )
        .unwrap();
        let out = t.frame_text();
        assert!(out.contains("name"), "{out:?}");
        assert!(out.contains("ada"), "{out:?}");
        assert!(out.contains("alan"), "{out:?}");
    }

    #[tokio::test]
    async fn a_cell_wider_than_its_header_does_not_run_into_the_next_column() {
        let t = TestTerminal::new(
            30,
            2,
            Element::component::<Table>(TableProps {
                headers: vec!["widget".into(), "kind".into()],
                rows: vec![vec!["TextInput".into(), "focusable".into()]],
                ..Default::default()
            }),
        )
        .unwrap();
        let out = t.frame_text();
        assert!(out.contains("TextInput"), "{out:?}");
        assert!(
            !out.contains("TextInputfocusable") && !out.contains("TextInpfocusable"),
            "cell text ran into the next column: {out:?}"
        );
    }

    #[tokio::test]
    async fn ragged_row_cells_past_the_headers_still_size_to_content() {
        // One header, but the row has a second cell wider than the old
        // hardcoded fallback width of 8 — it must render in full, sized to
        // its own content, not truncated at an arbitrary width.
        let t = TestTerminal::new(
            40,
            2,
            Element::component::<Table>(TableProps {
                headers: vec!["a".into()],
                rows: vec![vec!["x".into(), "a-very-long-cell".into()]],
                ..Default::default()
            }),
        )
        .unwrap();
        let out = t.frame_text();
        assert!(out.contains("a-very-long-cell"), "{out:?}");
    }

    #[tokio::test]
    async fn explicit_width_overrides_the_header_default() {
        let (rt, _rx) = crate::hooks::RuntimeHandle::test_handle();
        let mut tree = crate::fiber::FiberTree::new();
        tree.mount_root(
            Element::component::<Table>(TableProps {
                headers: vec!["x".into()],
                rows: vec![vec!["y".into()]],
                widths: vec![10],
                ..Default::default()
            }),
            &rt,
        );
        crate::layout::compute_layout(&mut tree, 30, 3);
        // The header row's only child (the one column) should be 10 cells wide.
        let header_row = tree.get(tree.root.unwrap()).children[0];
        let col = tree.get(header_row).children[0];
        assert_eq!(tree.get(col).layout.width, 10);
    }

    // ---------- viewport ----------

    #[test]
    fn a_viewport_builds_only_the_rows_it_covers() {
        // The point of the prop: with 500 rows and room for 5, the cost of a
        // frame must track the window. Counted from the element tree because
        // a rendered frame cannot tell "never built" from "built and then
        // clipped", and only the first is the property here.
        let el = render_once::<Table>(&TableProps {
            headers: vec!["r".into()],
            rows: rows(500),
            viewport: Some(Viewport::new(0, 5)),
            ..Default::default()
        });

        let Node::View { children, .. } = el.node else {
            panic!("expected a view")
        };
        assert_eq!(children.len(), 6, "one header plus five visible rows");
    }

    #[tokio::test]
    async fn a_viewport_renders_the_slice_it_names() {
        let t = TestTerminal::new(
            20,
            4,
            Element::component::<Table>(TableProps {
                headers: vec!["r".into()],
                rows: rows(500),
                viewport: Some(Viewport::new(100, 3)),
                ..Default::default()
            }),
        )
        .unwrap();

        let out = t.frame_text();
        assert!(out.contains("row-100"), "{out:?}");
        assert!(out.contains("row-102"), "{out:?}");
        assert!(!out.contains("row-000"), "{out:?}");
    }

    #[test]
    fn a_viewport_past_the_end_renders_no_rows_rather_than_panicking() {
        let el = render_once::<Table>(&TableProps {
            headers: vec!["r".into()],
            rows: rows(3),
            viewport: Some(Viewport::new(99, 5)),
            ..Default::default()
        });

        let Node::View { children, .. } = el.node else {
            panic!("expected a view")
        };
        assert_eq!(children.len(), 1, "the header, and nothing else");
    }

    #[test]
    fn a_viewport_longer_than_the_list_stops_at_the_end() {
        let el = render_once::<Table>(&TableProps {
            headers: vec!["r".into()],
            rows: rows(3),
            viewport: Some(Viewport::new(0, 100)),
            ..Default::default()
        });

        let Node::View { children, .. } = el.node else {
            panic!("expected a view")
        };
        assert_eq!(children.len(), 4);
    }

    #[tokio::test]
    async fn ragged_rows_change_the_column_count_as_they_scroll() {
        // Documented consequence of measuring from the window: the extra
        // column exists only while the row that needs it is visible.
        let rows = vec![
            vec!["a".into()],
            vec!["b".into(), "extra-column".into()],
            vec!["c".into()],
        ];

        let with = TestTerminal::new(
            30,
            2,
            Element::component::<Table>(TableProps {
                headers: vec!["h".into()],
                rows: rows.clone(),
                viewport: Some(Viewport::new(1, 1)),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(with.frame_text().contains("extra-column"));

        let without = TestTerminal::new(
            30,
            2,
            Element::component::<Table>(TableProps {
                headers: vec!["h".into()],
                rows,
                viewport: Some(Viewport::new(2, 1)),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(!without.frame_text().contains("extra-column"));
    }

    #[test]
    fn viewport_range_clamps_to_the_list() {
        assert_eq!(Viewport::new(0, 5).range(3), 0..3);
        assert_eq!(Viewport::new(10, 5).range(3), 3..3);
        assert_eq!(Viewport::new(1, 2).range(10), 1..3);
        assert_eq!(Viewport::new(0, 0).range(10), 0..0);
    }

    // ---------- selection ----------

    #[tokio::test]
    async fn the_selected_row_is_drawn_in_the_accent() {
        let t = TestTerminal::new(
            12,
            3,
            Element::component::<Table>(TableProps {
                headers: vec!["r".into()],
                rows: rows(2),
                selected: Some(1),
                ..Default::default()
            }),
        )
        .unwrap();

        let theme = crate::widgets::Theme::default();
        // Terminal row 0 is the header, so data row 1 is terminal row 2.
        assert_eq!(t.cell(0, 2).unwrap().bg, theme.accent, "selected row");
        assert_ne!(t.cell(0, 1).unwrap().bg, theme.accent, "unselected row");
    }

    #[tokio::test]
    async fn selection_is_indexed_against_the_list_not_the_window() {
        // Scrolled down, "row 101" must still mean row 101.
        let t = TestTerminal::new(
            12,
            4,
            Element::component::<Table>(TableProps {
                headers: vec!["r".into()],
                rows: rows(500),
                viewport: Some(Viewport::new(100, 3)),
                selected: Some(101),
                ..Default::default()
            }),
        )
        .unwrap();

        let theme = crate::widgets::Theme::default();
        // header, row-100, row-101, row-102
        assert_eq!(t.cell(0, 2).unwrap().bg, theme.accent);
        assert_ne!(t.cell(0, 1).unwrap().bg, theme.accent);
    }

    #[tokio::test]
    async fn a_selection_outside_the_window_highlights_nothing() {
        let t = TestTerminal::new(
            12,
            4,
            Element::component::<Table>(TableProps {
                headers: vec!["r".into()],
                rows: rows(500),
                viewport: Some(Viewport::new(100, 3)),
                selected: Some(7),
                ..Default::default()
            }),
        )
        .unwrap();

        let theme = crate::widgets::Theme::default();
        for y in 1..4 {
            assert_ne!(t.cell(0, y).unwrap().bg, theme.accent, "row {y}");
        }
    }

    // ---------- per-cell styling ----------

    #[tokio::test]
    async fn a_styler_overrides_the_color_of_the_cells_it_names() {
        let t = TestTerminal::new(
            20,
            3,
            Element::component::<Table>(TableProps {
                headers: vec!["a".into(), "b".into()],
                rows: vec![vec!["x".into(), "y".into()]],
                cell_style: Some(CellStyler::new(|cell| {
                    if cell.column == 1 {
                        CellStyle::color(Color::Red)
                    } else {
                        CellStyle::default()
                    }
                })),
                ..Default::default()
            }),
        )
        .unwrap();

        let theme = crate::widgets::Theme::default();
        assert_eq!(
            t.cell(0, 1).unwrap().fg,
            theme.foreground,
            "column 0 keeps the default"
        );
        assert_eq!(t.cell(2, 1).unwrap().fg, Color::Red, "column 1 is styled");
    }

    #[tokio::test]
    async fn a_styler_sees_the_cells_value_and_selection() {
        let t = TestTerminal::new(
            20,
            3,
            Element::component::<Table>(TableProps {
                headers: vec!["n".into()],
                rows: vec![vec!["9".into()], vec!["1".into()]],
                selected: Some(0),
                cell_style: Some(CellStyler::new(|cell| {
                    // Both inputs must reach the styler: value and selection.
                    if cell.value == "9" && cell.selected {
                        CellStyle::color(Color::Magenta)
                    } else {
                        CellStyle::default()
                    }
                })),
                ..Default::default()
            }),
        )
        .unwrap();

        assert_eq!(t.cell(0, 1).unwrap().fg, Color::Magenta);
        assert_ne!(t.cell(0, 2).unwrap().fg, Color::Magenta);
    }

    #[tokio::test]
    async fn an_unset_style_field_keeps_the_tables_own_choice() {
        // A styler that sets only the color must not clear the selection
        // highlight behind it.
        let t = TestTerminal::new(
            20,
            2,
            Element::component::<Table>(TableProps {
                headers: vec!["n".into()],
                rows: vec![vec!["x".into()]],
                selected: Some(0),
                cell_style: Some(CellStyler::new(|_| CellStyle::color(Color::Red))),
                ..Default::default()
            }),
        )
        .unwrap();

        let theme = crate::widgets::Theme::default();
        let cell = t.cell(0, 1).unwrap();
        assert_eq!(cell.fg, Color::Red, "the styler's color");
        assert_eq!(cell.bg, theme.accent, "the table's selection background");
    }

    #[test]
    fn stylers_compare_by_identity() {
        let a = CellStyler::new(|_| CellStyle::default());
        let b = CellStyler::new(|_| CellStyle::default());

        assert_eq!(a, a.clone());
        assert_ne!(a, b);
    }
}
