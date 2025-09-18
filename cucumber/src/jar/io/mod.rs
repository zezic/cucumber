//! I/O modules for JAR file reading and writing operations
//!
//! This module contains functionality for reading from and writing to
//! JAR files, including theme extraction and application.

pub mod writer;

// Re-export commonly used I/O functionality
pub use writer::write_theme_to_jar;
