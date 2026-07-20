# Widgets

`ntui::widgets` is a first-party, batteries-included widget layer. Every
widget is an ordinary component built entirely out of ntui's five element
kinds — no sixth primitive, no special engine access — so anything a widget
does, your own components can do too.

Nothing here is required: the module is an opinionated layer on top of the
core primitives, not a dependency of them.

## The catalog

| Widget | What it does |
|---|---|
| `Spinner` | Braille-frame activity indicator with an optional label |
| `ProgressBar` | Solid or gradient fill; `animate: true` tweens toward new values |
| `Badge` | Small status pill, colored by `Tone` (neutral/info/success/warn/danger) |
| `Divider` | Horizontal rule, bare (full-width) or labeled |
| `GradientText` | Text with a color gradient across its characters |
| `Banner` | Large gradient headline block |
| `Button` | Focusable; Enter/Space fires `on_press` |
| `Checkbox` / `Toggle` | Focusable; Enter/Space flips and reports via `on_change` |
| `Select` | Focusable list; Up/Down moves the highlight, wrapping at the ends |
| `Table` | Static grid; columns auto-size to the widest header/cell |
| `Tabs` | Focusable tab strip; Left/Right moves the active tab |
| `TextInput` | Focusable single-line field with a blinking caret |
| `Modal` | Centered dialog over a full-screen backdrop; Esc calls `on_close` |
| `Toast` | Corner-anchored notification that auto-dismisses after a duration |
| `Tooltip` | Corner-anchored hint box |

`Modal`, `Toast`, and `Tooltip` render through `ViewProps::overlay` — an
`Anchor` that takes the view out of normal flow and paints it last, against
the whole viewport. That's a `View` prop, not a new node kind, so custom
overlay components get the same power.

## Composition pattern

Widgets read two pieces of shared context, both optional:

- **Theme** — `use_theme()` returns the nearest provided
  [`Theme`](hooks#use_theme--use_focus_scope--use_focusable) or the built-in
  default. Provide your own to recolor every widget at once.
- **Focus** — interactive widgets register with the nearest focus scope and
  only react to keys while focused. Create the scope once, near the root.

```rust
#[component]
fn App(hooks: &mut Hooks) -> Element {
    let scope = hooks.use_focus_scope(); // Tab / Shift-Tab cycling
    element! {
        ContextProvider(value: scope) {
            View(flex_direction: FlexDirection::Column, gap: 1) {
                TextInput(value: draft.get(), on_change: ..., on_submit: ...)
                Select(items: items.clone(), on_change: ...)
                Button(label: "Save".to_string(), on_press: ...)
            }
        }
    }
}
```

Without a scope, focusable widgets simply never report focus (and never
react to keys); without a theme provider, everything uses the default
palette. Both degrade quietly rather than requiring setup.

## Controlled state

Interactive widgets are *controlled*: they don't own their value. `TextInput`
renders whatever `value` you pass and reports edits through `on_change`;
`Checkbox`, `Select`, and `Tabs` follow the same shape. Callbacks are
`Callback<T>` — cheap to clone, created from any closure with
`Callback::new(...)`.

The `widgets_gallery` example (`cargo run --example widgets_gallery`) shows
every widget wired together under one scope and theme.
