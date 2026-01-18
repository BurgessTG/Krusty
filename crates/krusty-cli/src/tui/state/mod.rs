//! App State Components
//!
//! Centralized state management for the TUI.
//! Groups related state into logical modules.

mod blocks;
mod hover;
mod indices;
mod layout;
mod popups;
mod scroll;
mod selection;
mod ui_state;

pub use blocks::BlockManager;
pub use hover::{HoverState, HoveredLink};
pub use indices::BlockIndices;
pub use layout::LayoutState;
pub use popups::PopupState;
pub use scroll::ScrollState;
pub use selection::{
    BlockScrollbarDrag, DragTarget, EdgeScrollDirection, EdgeScrollState, SelectionArea,
    SelectionState,
};
pub use ui_state::{hash_content, BlockUiStates, ToolResultCache};
