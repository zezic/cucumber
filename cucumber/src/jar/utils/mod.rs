//! Utility modules for JAR manipulation
//!
//! This module contains debugging utilities, legacy compatibility,
//! and other helper functions for JAR processing.

pub mod debug;
pub mod legacy;

// Re-export commonly used utilities
pub use debug::debug_print_color;
pub use legacy::TimelineColorReference;
