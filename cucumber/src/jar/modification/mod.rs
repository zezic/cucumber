//! Modification modules for JAR file manipulation and patching
//!
//! This module contains functionality for modifying JAR files, including
//! color replacement, general bytecode patching, and building modified
//! JAR content.

pub mod color_replacer;
pub mod patcher;

// Re-export commonly used modification functionality
pub use color_replacer::replace_named_color;
pub use patcher::patch_class;
