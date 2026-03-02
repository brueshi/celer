use rustpython_parser::{self as parser, ast};
use tracing::debug;

use crate::convert_stmt::convert_stmt;
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
            match convert_stmt(stmt) {
                Ok(hir_stmt) => module.body.push(hir_stmt),
                Err(ParseError::UnsupportedFeature(feat)) => {
                    debug!("skipping unsupported feature: {feat}");
                }
                Err(e) => return Err(e),
            }
        }

        Ok(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Statement, TypeAnnotation};

    #[test]
    fn empty_source_produces_empty_module() {
        let module = Converter::convert_module("test", "test.py", "").unwrap();
        assert_eq!(module.name, "test");
        assert!(module.body.is_empty());
    }

    #[test]
    fn simple_assignment_parses() {
        let module = Converter::convert_module("test", "test.py", "x = 1\n").unwrap();
        assert_eq!(module.body.len(), 1);
        assert!(matches!(
            &module.body[0],
            Statement::Assign { target, .. } if target == "x"
        ));
    }

    #[test]
    fn full_fastapi_module() {
        let source = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def root() -> dict:
    return {"message": "hello"}

@app.get("/items/{item_id}")
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}
"#;
        let module = Converter::convert_module("basic", "basic.py", source).unwrap();

        // Should have: ImportFrom, Assign(app), FunctionDef(root), FunctionDef(get_item)
        assert_eq!(module.body.len(), 4);

        // Verify root function
        match &module.body[2] {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "root");
                assert!(f.params.is_empty());
                assert_eq!(
                    f.return_type,
                    TypeAnnotation::Dict(
                        Box::new(TypeAnnotation::Any),
                        Box::new(TypeAnnotation::Any)
                    )
                );
                assert_eq!(f.decorators, vec!["app.get(\"/\")"]);
                assert_eq!(f.body.len(), 1);
            }
            _ => panic!("expected FunctionDef for root"),
        }

        // Verify get_item function
        match &module.body[3] {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "get_item");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name, "item_id");
                assert_eq!(f.params[0].annotation, TypeAnnotation::Int);
                assert_eq!(
                    f.return_type,
                    TypeAnnotation::Dict(
                        Box::new(TypeAnnotation::Any),
                        Box::new(TypeAnnotation::Any)
                    )
                );
            }
            _ => panic!("expected FunctionDef for get_item"),
        }
    }
}
