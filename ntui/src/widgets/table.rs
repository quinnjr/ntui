use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use crate::style::{Color, Weight};

/// A static (non-focusable) table. Column widths default to the widest of
/// that column's header and cell contents, plus one cell of breathing room;
/// override per-column via `widths` (a `0` entry falls back to the default
/// for that column). Rows may be ragged — cells past `headers.len()` still
/// get content-sized columns (with no header above them).
#[derive(Clone, PartialEq, Default)]
pub struct TableProps {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub widths: Vec<u16>,
}

pub struct Table;
impl Component for Table {
    type Props = TableProps;
    fn render(props: &TableProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();

        // Cover the widest row too, not just `headers`, so ragged rows get
        // content-sized columns instead of an arbitrary fallback width.
        let cols = props
            .rows
            .iter()
            .map(|r| r.len())
            .fold(props.headers.len(), usize::max);
        let col_widths: Vec<u16> = (0..cols)
            .map(|i| {
                if let Some(w) = props.widths.get(i).copied().filter(|w| *w > 0) {
                    return w;
                }
                let header_len = props.headers.get(i).map(|h| h.chars().count()).unwrap_or(0);
                let max_cell_len = props
                    .rows
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
        let mut children = Vec::with_capacity(1 + props.rows.len());
        children.push(Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
            header_cells,
        ));

        for (r, row) in props.rows.iter().enumerate() {
            let bg = if r % 2 == 0 {
                Color::Reset
            } else {
                theme.surface
            };
            let cells = row
                .iter()
                .enumerate()
                .map(|(i, v)| cell(v, col_width(i), theme.foreground, Weight::Normal, bg))
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
    use crate::testing::TestTerminal;

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
                widths: vec![],
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
                widths: vec![],
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
                widths: vec![],
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
            }),
            &rt,
        );
        crate::layout::compute_layout(&mut tree, 30, 3);
        // The header row's only child (the one column) should be 10 cells wide.
        let header_row = tree.get(tree.root.unwrap()).children[0];
        let col = tree.get(header_row).children[0];
        assert_eq!(tree.get(col).layout.width, 10);
    }
}
