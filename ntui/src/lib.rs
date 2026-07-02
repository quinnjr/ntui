//! `ntui` is an [Ink](https://github.com/vadimdemedes/ink)-style library for building
//! terminal UIs in Rust out of declarative components and hooks. Layouts are described
//! with `View`/`Text` elements (often via the [`element!`] macro) and arranged with a
//! flexbox model powered by [`taffy`]; components re-render in response to state and
//! effect hooks, similar to React. `render` drives the component tree fullscreen against
//! the real terminal using `crossterm`, while [`testing::TestTerminal`] drives the same
//! tree headlessly for tests.
//!
//! ```no_run
//! use ntui::{component, element, render};
//!
//! #[component]
//! fn Counter(hooks: &mut ntui::Hooks) -> ntui::Element {
//!     let count = hooks.use_state(|| 0i32);
//!     element! {
//!         View(padding: 1) {
//!             Text(content: format!("count: {}", count.get()))
//!         }
//!     }
//! }
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> Result<(), ntui::Error> {
//!     render(element!(Counter)).await
//! }
//! ```
//!
//! Core types:
//! - [`Element`] / [`Node`]: the tree of views, text, fragments, and components built
//!   each render, typically via [`element!`].
//! - [`Component`]: the trait implemented (usually via `#[component]`) to render props
//!   and hooks into an [`Element`].
//! - [`Hooks`]: per-fiber hook state, exposing `use_state`, `use_effect`, `use_input`,
//!   and friends during a component's render.
//! - [`render`]: runs a component tree fullscreen against the real terminal until the
//!   app exits.
//! - [`testing::TestTerminal`]: drives a component tree headlessly, frame by frame, for
//!   deterministic tests.
//!
//! # Semver note
//!
//! `ntui` re-exports `crossterm::event` key types (`KeyCode`, `KeyEvent`, `KeyEventKind`,
//! `KeyModifiers`) as part of its public API. A breaking change in a future major version
//! of `crossterm` would therefore surface as a breaking change in `ntui` even if no `ntui`
//! code changes.
pub mod backend;
pub mod buffer;
pub mod component;
pub mod element;
pub mod error;
pub(crate) mod fiber;
pub mod hooks;
pub(crate) mod layout;
pub(crate) mod paint;
pub mod props;
pub(crate) mod reconciler;
pub(crate) mod runtime;
pub mod style;
#[cfg(test)]
pub(crate) mod test_util;
pub mod testing;
pub(crate) mod text;

pub use backend::{Backend, FullscreenBackend, TestBackend};
pub use component::Component;
pub use element::{Element, Node};
pub use error::Error;
pub use hooks::Hooks;
pub use hooks::app::AppHandle;
pub use hooks::effect::Cleanup;
pub use hooks::input::{InputCtx, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub use hooks::state::State;
pub use ntui_macros::{component, element};
pub use props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
pub use runtime::render;
pub use style::{Attrs, BorderStyle, Color, Weight};

/// Internal, unstable surface for out-of-crate test tooling (benchmarks and
/// fuzz targets), enabled by the `bench` or `fuzz` feature. Not covered by
/// semver; do not use.
#[cfg(any(feature = "bench", feature = "fuzz"))]
#[doc(hidden)]
pub mod __private {
    /// See `crate::text::wrap_text`.
    pub fn wrap_text(content: &str, max_width: usize) -> Vec<String> {
        crate::text::wrap_text(content, max_width)
    }
    /// See `crate::text::truncate_line`.
    pub fn truncate_line(content: &str, max_width: usize) -> String {
        crate::text::truncate_line(content, max_width)
    }
}
