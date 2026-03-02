use rustpython_parser::ast;

use crate::convert_decorator::decorator_to_string;
use crate::convert_expr::convert_expr;
use crate::convert_type::convert_annotation;
use crate::error::ParseError;
use celer_hir::{Function, Parameter, Statement, TypeAnnotation};

/// Convert a rustpython-parser statement AST node to a HIR Statement.
pub fn convert_stmt(stmt: &ast::Stmt) -> Result<Statement, ParseError> {
    match stmt {
        ast::Stmt::FunctionDef(f) => convert_function_def(f, false),
        ast::Stmt::AsyncFunctionDef(f) => convert_async_function_def(f),
        ast::Stmt::Return(r) => convert_return(r),
        ast::Stmt::Assign(a) => convert_assign(a),
        ast::Stmt::ImportFrom(i) => convert_import_from(i),
        ast::Stmt::Import(i) => convert_import(i),
        ast::Stmt::Expr(e) => {
            let expr = convert_expr(&e.value)?;
            Ok(Statement::Expr(expr))
        }
        ast::Stmt::Pass(_) => Ok(Statement::Pass),
        ast::Stmt::Break(_) => Ok(Statement::Break),
        ast::Stmt::Continue(_) => Ok(Statement::Continue),
        ast::Stmt::If(i) => convert_if(i),
        ast::Stmt::While(w) => convert_while(w),
        ast::Stmt::For(f) => convert_for(f),
        ast::Stmt::Raise(r) => convert_raise(r),
        ast::Stmt::Assert(a) => convert_assert(a),
        _ => Err(ParseError::UnsupportedFeature(format!(
            "statement: {stmt:?}"
        ))),
    }
}

fn convert_function_def(f: &ast::StmtFunctionDef, is_async: bool) -> Result<Statement, ParseError> {
    let name = f.name.to_string();

    let decorators: Vec<String> = f.decorator_list.iter().map(decorator_to_string).collect();

    let params = convert_arguments(&f.args)?;

    let return_type = match &f.returns {
        Some(ret_expr) => convert_annotation(ret_expr)?,
        None => TypeAnnotation::Unknown,
    };

    let body: Result<Vec<_>, _> = f.body.iter().map(convert_stmt).collect();

    Ok(Statement::FunctionDef(Function {
        name,
        params,
        return_type,
        body: body?,
        decorators,
        is_async,
    }))
}

fn convert_async_function_def(f: &ast::StmtAsyncFunctionDef) -> Result<Statement, ParseError> {
    let name = f.name.to_string();

    let decorators: Vec<String> = f.decorator_list.iter().map(decorator_to_string).collect();

    let params = convert_arguments(&f.args)?;

    let return_type = match &f.returns {
        Some(ret_expr) => convert_annotation(ret_expr)?,
        None => TypeAnnotation::Unknown,
    };

    let body: Result<Vec<_>, _> = f.body.iter().map(convert_stmt).collect();

    Ok(Statement::FunctionDef(Function {
        name,
        params,
        return_type,
        body: body?,
        decorators,
        is_async: true,
    }))
}

fn convert_arguments(args: &ast::Arguments) -> Result<Vec<Parameter>, ParseError> {
    let mut params = Vec::new();

    // positional-only args
    for awd in &args.posonlyargs {
        params.push(convert_arg_with_default(awd)?);
    }

    // regular args
    for awd in &args.args {
        params.push(convert_arg_with_default(awd)?);
    }

    // keyword-only args
    for awd in &args.kwonlyargs {
        params.push(convert_arg_with_default(awd)?);
    }

    Ok(params)
}

fn convert_arg_with_default(awd: &ast::ArgWithDefault) -> Result<Parameter, ParseError> {
    let name = awd.def.arg.to_string();
    let annotation = match &awd.def.annotation {
        Some(ann) => convert_annotation(ann)?,
        None => TypeAnnotation::Unknown,
    };
    let default = match &awd.default {
        Some(expr) => Some(convert_expr(expr)?),
        None => None,
    };

    Ok(Parameter {
        name,
        annotation,
        default,
    })
}

fn convert_return(r: &ast::StmtReturn) -> Result<Statement, ParseError> {
    let value = match &r.value {
        Some(expr) => Some(convert_expr(expr)?),
        None => None,
    };
    Ok(Statement::Return { value })
}

fn convert_assign(a: &ast::StmtAssign) -> Result<Statement, ParseError> {
    // Take the first target only (multi-target assignment not supported)
    let target = a
        .targets
        .first()
        .ok_or_else(|| ParseError::ConversionError("assign with no targets".into()))?;

    let target_name = match target {
        ast::Expr::Name(n) => n.id.to_string(),
        _ => {
            return Err(ParseError::UnsupportedFeature(
                "non-name assignment target".into(),
            ));
        }
    };

    let value = convert_expr(&a.value)?;

    Ok(Statement::Assign {
        target: target_name,
        annotation: None,
        value,
    })
}

fn convert_import(i: &ast::StmtImport) -> Result<Statement, ParseError> {
    let names: Vec<(String, Option<String>)> = i
        .names
        .iter()
        .map(|alias| {
            (
                alias.name.to_string(),
                alias.asname.as_ref().map(|a| a.to_string()),
            )
        })
        .collect();

    // Use the first module name as the import module
    let module = names.first().map(|(n, _)| n.clone()).unwrap_or_default();

    Ok(Statement::Import { module, names })
}

fn convert_import_from(i: &ast::StmtImportFrom) -> Result<Statement, ParseError> {
    let module = i.module.as_ref().map(|m| m.to_string()).unwrap_or_default();

    let names: Vec<(String, Option<String>)> = i
        .names
        .iter()
        .map(|alias| {
            (
                alias.name.to_string(),
                alias.asname.as_ref().map(|a| a.to_string()),
            )
        })
        .collect();

    Ok(Statement::ImportFrom { module, names })
}

fn convert_if(i: &ast::StmtIf) -> Result<Statement, ParseError> {
    let test = convert_expr(&i.test)?;
    let body: Result<Vec<_>, _> = i.body.iter().map(convert_stmt).collect();
    let orelse: Result<Vec<_>, _> = i.orelse.iter().map(convert_stmt).collect();
    Ok(Statement::If {
        test,
        body: body?,
        orelse: orelse?,
    })
}

fn convert_while(w: &ast::StmtWhile) -> Result<Statement, ParseError> {
    let test = convert_expr(&w.test)?;
    let body: Result<Vec<_>, _> = w.body.iter().map(convert_stmt).collect();
    Ok(Statement::While { test, body: body? })
}

fn convert_for(f: &ast::StmtFor) -> Result<Statement, ParseError> {
    let target = match &*f.target {
        ast::Expr::Name(n) => n.id.to_string(),
        _ => {
            return Err(ParseError::UnsupportedFeature(
                "non-name for-loop target".into(),
            ));
        }
    };
    let iter = convert_expr(&f.iter)?;
    let body: Result<Vec<_>, _> = f.body.iter().map(convert_stmt).collect();
    Ok(Statement::For {
        target,
        iter,
        body: body?,
    })
}

fn convert_raise(r: &ast::StmtRaise) -> Result<Statement, ParseError> {
    let value = match &r.exc {
        Some(expr) => Some(convert_expr(expr)?),
        None => None,
    };
    Ok(Statement::Raise { value })
}

fn convert_assert(a: &ast::StmtAssert) -> Result<Statement, ParseError> {
    let test = convert_expr(&a.test)?;
    let msg = match &a.msg {
        Some(m) => Some(convert_expr(m)?),
        None => None,
    };
    Ok(Statement::Assert { test, msg })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpython_parser as parser;

    fn parse_stmts(source: &str) -> Vec<ast::Stmt> {
        let parsed = parser::parse(source, parser::Mode::Module, "<test>").unwrap();
        match parsed {
            ast::Mod::Module(m) => m.body,
            _ => panic!("expected Module"),
        }
    }

    #[test]
    fn function_def_basic() {
        let stmts = parse_stmts("def foo():\n    pass\n");
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "foo");
                assert!(f.params.is_empty());
                assert!(!f.is_async);
                assert_eq!(f.return_type, TypeAnnotation::Unknown);
            }
            _ => panic!("expected FunctionDef"),
        }
    }

    #[test]
    fn function_with_typed_params_and_return() {
        let stmts =
            parse_stmts("def get_item(item_id: int) -> dict:\n    return {\"item_id\": item_id}\n");
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
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
            _ => panic!("expected FunctionDef"),
        }
    }

    #[test]
    fn function_with_decorator() {
        let src = "@app.get(\"/\")\ndef root() -> dict:\n    return {\"message\": \"hello\"}\n";
        let stmts = parse_stmts(src);
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "root");
                assert_eq!(f.decorators.len(), 1);
                assert_eq!(f.decorators[0], "app.get(\"/\")");
            }
            _ => panic!("expected FunctionDef"),
        }
    }

    #[test]
    fn return_dict() {
        let stmts = parse_stmts("return {\"key\": \"val\"}\n");
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
            Statement::Return { value: Some(expr) } => {
                assert!(matches!(expr, celer_hir::Expression::Dict { .. }));
            }
            _ => panic!("expected Return with dict"),
        }
    }

    #[test]
    fn import_from() {
        let stmts = parse_stmts("from fastapi import FastAPI\n");
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
            Statement::ImportFrom { module, names } => {
                assert_eq!(module, "fastapi");
                assert_eq!(names.len(), 1);
                assert_eq!(names[0].0, "FastAPI");
            }
            _ => panic!("expected ImportFrom"),
        }
    }

    #[test]
    fn assign_simple() {
        let stmts = parse_stmts("x = 42\n");
        let hir = convert_stmt(&stmts[0]).unwrap();
        match hir {
            Statement::Assign { target, value, .. } => {
                assert_eq!(target, "x");
                assert_eq!(value, celer_hir::Expression::IntLiteral(42));
            }
            _ => panic!("expected Assign"),
        }
    }

    #[test]
    fn parse_target_patterns() {
        // Both target patterns from the plan
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def root() -> dict:
    return {"message": "hello"}

@app.get("/items/{item_id}")
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}
"#;
        let stmts = parse_stmts(src);
        let hir_stmts: Vec<_> = stmts.iter().map(convert_stmt).collect();

        // All should parse successfully
        for (i, result) in hir_stmts.iter().enumerate() {
            assert!(
                result.is_ok(),
                "statement {i} failed: {:?}",
                result.as_ref().err()
            );
        }

        // Verify we got: ImportFrom, Assign, FunctionDef(root), FunctionDef(get_item)
        assert!(matches!(
            hir_stmts[0].as_ref().unwrap(),
            Statement::ImportFrom { .. }
        ));
        assert!(matches!(
            hir_stmts[1].as_ref().unwrap(),
            Statement::Assign { .. }
        ));

        // root function
        match hir_stmts[2].as_ref().unwrap() {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "root");
                assert!(f.params.is_empty());
                assert_eq!(f.decorators[0], "app.get(\"/\")");
            }
            _ => panic!("expected root FunctionDef"),
        }

        // get_item function
        match hir_stmts[3].as_ref().unwrap() {
            Statement::FunctionDef(f) => {
                assert_eq!(f.name, "get_item");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name, "item_id");
                assert_eq!(f.params[0].annotation, TypeAnnotation::Int);
                assert_eq!(f.decorators[0], "app.get(\"/items/{item_id}\")");
            }
            _ => panic!("expected get_item FunctionDef"),
        }
    }
}
