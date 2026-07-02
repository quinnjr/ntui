// modules are added task by task
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
