//! A small, first-party widgets library built entirely out of ntui's five
//! element kinds (`View`, `Text`, `Fragment`, `Provider`, `Component`) — no
//! new node kind is introduced anywhere in this module.
//!
//! Nothing here is required: `ntui::widgets` is an opinionated layer on top
//! of the core primitives, not a dependency of them.

// `pub`, not private: hook methods (`use_theme`, `use_focus_scope`,
// `use_focusable`) are inherent impls on `Hooks` defined inside these
// modules, and inherent-method visibility follows the defining module's own
// path — a `pub use` of the *type* below doesn't also expose the *methods*.
// Mirrors the same pattern in `hooks/mod.rs` (`pub mod state;`, etc.).
pub mod focus;
pub mod theme;

mod badge;
mod banner;
mod button;
mod callback;
mod checkbox;
mod divider;
mod gradient_text;
mod modal;
mod progress;
mod select;
mod spinner;
mod table;
mod tabs;
mod text_input;
mod toast;
mod tooltip;

pub use focus::{Focus, FocusScopeHandle};
pub use theme::Theme;

pub use badge::{Badge, BadgeProps, Tone};
pub use banner::{Banner, BannerProps};
pub use button::{Button, ButtonProps};
pub use callback::Callback;
pub use checkbox::{Checkbox, CheckboxProps, Toggle, ToggleProps};
pub use divider::{Divider, DividerProps};
pub use gradient_text::{GradientText, GradientTextProps};
pub use modal::{Modal, ModalProps};
pub use progress::{ProgressBar, ProgressBarProps};
pub use select::{Select, SelectProps};
pub use spinner::{Spinner, SpinnerProps};
pub use table::{Table, TableProps};
pub use tabs::{Tabs, TabsProps};
pub use text_input::{TextInput, TextInputProps};
pub use toast::{Toast, ToastProps};
pub use tooltip::{Tooltip, TooltipProps};
