//! Analysis modules for JAR file introspection and data extraction
//!
//! This module contains functionality for scanning JAR files, extracting
//! color information, method signatures, and other metadata needed for
//! theme processing.

pub mod extractor;
pub mod introspection;
pub mod scanner;

// Re-export commonly used analysis functionality
pub use extractor::{
    extract_general_goodies, extract_palette_color_methods, extract_raw_color_goodies,
    scan_for_named_color_defs,
};
pub use introspection::{
    extract_named_color_getter_1, find_const_name, find_method_by_sig, find_method_description,
    find_named_color_getter_1_invocations, find_utf_ldc, NamedColorGetterInvocation,
};
pub use scanner::{
    detect_timeline_color_const, extract_release_metadata, has_any_double_in_constant_pool,
    has_any_string_in_constant_pool, is_useful_file, UsefulFileType,
};
