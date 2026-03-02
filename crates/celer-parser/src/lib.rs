pub mod convert_decorator;
pub mod convert_expr;
pub mod convert_stmt;
pub mod convert_type;
pub mod converter;
pub mod error;

pub use converter::Converter;
pub use error::ParseError;

/// Parse Python source into a HIR module.
pub fn parse_module(name: &str, path: &str, source: &str) -> Result<celer_hir::Module, ParseError> {
    Converter::convert_module(name, path, source)
}
