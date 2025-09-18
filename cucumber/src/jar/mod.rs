//! JAR manipulation library for Bitwig Studio color theme modification
//!
//! This module provides a comprehensive set of tools for analyzing, modifying,
//! and rewriting JAR files to change color schemes in Bitwig Studio. The
//! functionality is organized into several sub-modules:
//!
//! - `analysis`: JAR file scanning, introspection, and data extraction
//! - `core`: Core JAR operations like parsing, assembly, and bytecode manipulation
//! - `modification`: Color replacement, patching, and JAR content modification
//! - `io`: JAR file reading and writing operations
//! - `types`: Type definitions for colors, methods, and metadata
//! - `utils`: Debug utilities and legacy compatibility
//!
//! # Example Usage
//!
//! ```no_run
//! use cucumber::jar::{analysis::extract_general_goodies, io::write_theme_to_jar};
//! use std::collections::BTreeMap;
//!
//! // Extract color information from a JAR file
//! let mut zip = /* open JAR file */;
//! let goodies = extract_general_goodies(&mut zip, |_| {})?;
//!
//! // Apply theme changes
//! let changed_colors = BTreeMap::new(); // populate with color changes
//! write_theme_to_jar("input.jar", "output.jar", changed_colors, |_| {})?;
//! ```

// Sub-modules
pub mod analysis;
pub mod core;
pub mod io;
pub mod modification;
pub mod types;
pub mod utils;

// Re-export the most commonly used functionality for convenience
pub use analysis::{extract_general_goodies, NamedColorGetterInvocation};
pub use core::{reasm, ReasmError};
pub use io::write_theme_to_jar;
pub use modification::{patch_class, replace_named_color};
pub use types::{
    ColorComponents, GeneralGoodies, MethodDescription, MethodSignatureKind, NamedColor,
    PaletteColorMethods, RawColorGoodies,
};
pub use utils::{debug_print_color, TimelineColorReference};

// Legacy re-exports for backward compatibility
// These will be deprecated in future versions
pub use analysis::extractor as analysis_impl;
pub use types::colors as goodies;
