pub mod exporter;
pub mod parser;

pub use exporter::*;
pub use parser::*;

// Test-only dump of parse_document + visual_rows. The package is a binary,
// so an external harness cannot call those functions without this hook.
#[cfg(test)]
mod render_dump;
