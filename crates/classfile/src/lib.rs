//! Java SE 21 class-file parsing. Parsing does not load, verify, or execute code.
//!
//! Unknown and currently unsupported standard attributes are retained explicitly
//! as opaque bytes. Successful parsing is not a bytecode-verification result.

mod annotation;
mod attribute;
mod constant_pool;
pub mod descriptor;
mod error;
mod model;
mod module;
mod parser;
mod reader;
#[cfg(test)]
mod tests;

pub use annotation::*;
pub use attribute::*;
pub use constant_pool::*;
pub use error::*;
pub use model::*;
pub use module::*;
pub use parser::{ParseOptions, parse, parse_with_options};
