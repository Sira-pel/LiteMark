pub mod exporter;
pub mod image;
pub mod parser;
pub mod pdf_export;

pub use exporter::*;
pub use parser::*;
pub use pdf_export::*;

// Test-only dump of parse_document + visual_rows. The package is a binary,
// so an external harness cannot call those functions without this hook.
#[cfg(test)]
mod render_dump;
