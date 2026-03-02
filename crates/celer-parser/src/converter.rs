use rustpython_parser::{self as parser, ast};
use tracing::debug;

use crate::error::ParseError;

/// Converts a rustpython-parser AST into Celer HIR.
pub struct Converter;

impl Converter {
    /// Parse Python source and produce an HIR module.
    pub fn convert_module(
        name: &str,
        path: &str,
        source: &str,
    ) -> Result<celer_hir::Module, ParseError> {
        let parsed = parser::parse(source, parser::Mode::Module, "<module>")?;

        let mod_body = match parsed {
            ast::Mod::Module(m) => m.body,
            _ => {
                return Err(ParseError::ConversionError(
                    "expected Module, got different Mod variant".into(),
                ));
            }
        };

        let mut module = celer_hir::Module::new(name, path);

        for stmt in &mod_body {
            match Self::convert_stmt(stmt) {
                Ok(hir_stmt) => module.body.push(hir_stmt),
                Err(ParseError::UnsupportedFeature(feat)) => {
                    debug!("skipping unsupported feature: {feat}");
                }
                Err(e) => return Err(e),
            }
        }

        Ok(module)
    }

    // -- statement conversion stubs --

    fn convert_stmt(_stmt: &ast::Stmt) -> Result<celer_hir::Statement, ParseError> {
        // TODO: full statement conversion (match on Stmt variants, delegate to helpers)
        Err(ParseError::UnsupportedFeature(
            "full statement conversion not yet implemented".into(),
        ))
    }

    #[allow(dead_code)]
    fn convert_expr(_expr: &ast::Expr) -> Result<celer_hir::Expression, ParseError> {
        // TODO: full expression conversion
        Err(ParseError::UnsupportedFeature(
            "full expression conversion not yet implemented".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_produces_empty_module() {
        let module = Converter::convert_module("test", "test.py", "").unwrap();
        assert_eq!(module.name, "test");
        assert!(module.body.is_empty());
    }

    #[test]
    fn non_empty_source_parses_without_panic() {
        // Statements are skipped as UnsupportedFeature for now,
        // but the parse itself should succeed.
        let result = Converter::convert_module("test", "test.py", "x = 1\n");
        assert!(result.is_ok());
        // Body is empty because convert_stmt returns UnsupportedFeature
        assert!(result.unwrap().body.is_empty());
    }
}
