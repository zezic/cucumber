//! Core JAR manipulation operations
//!
//! This module contains the fundamental operations for working with JAR files
//! and Java bytecode, including parsing, assembly, and bytecode manipulation.

pub mod assembly;
pub mod bytecode;

// Re-export commonly used core functionality
pub use assembly::{init_refprinter, reasm, ReasmError};
pub use bytecode::{IxToDouble, IxToFloat, IxToInt};
