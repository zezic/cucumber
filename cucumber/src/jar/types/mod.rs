//! Type definitions for JAR manipulation
//!
//! This module contains all the data structures used for representing
//! colors, methods, metadata, and other JAR-related information.

pub mod colors;
pub mod metadata;
pub mod methods;

// Re-export commonly used types for convenience
pub use colors::{
    ColorComponents, FloatToAddToConstantPool, NamedColor, RawColorConst, RawColorConstants,
};
pub use metadata::{GeneralGoodies, RawColorGoodies};
pub use methods::{MethodDescription, MethodSignatureKind, PaletteColorMethods, RawColorMethods};
