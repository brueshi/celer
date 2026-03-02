use rustpython_parser::ast;

/// Convert a decorator expression AST node to a string representation.
/// e.g. `app.get("/")` -> `"app.get(\"/\")"`
pub fn decorator_to_string(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Call(call) => {
            let func_str = expr_to_string(&call.func);
            let args: Vec<String> = call.args.iter().map(expr_to_string).collect();
            format!("{}({})", func_str, args.join(", "))
        }
        ast::Expr::Attribute(attr) => {
            format!("{}.{}", expr_to_string(&attr.value), attr.attr)
        }
        ast::Expr::Name(name) => name.id.to_string(),
        _ => format!("{expr:?}"),
    }
}

fn expr_to_string(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Name(name) => name.id.to_string(),
        ast::Expr::Attribute(attr) => {
            format!("{}.{}", expr_to_string(&attr.value), attr.attr)
        }
        ast::Expr::Constant(c) => match &c.value {
            ast::Constant::Str(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            ast::Constant::Int(i) => i.to_string(),
            ast::Constant::Float(f) => f.to_string(),
            ast::Constant::Bool(b) => b.to_string(),
            ast::Constant::None => "None".to_string(),
            _ => format!("{:?}", c.value),
        },
        ast::Expr::Call(call) => {
            let func_str = expr_to_string(&call.func);
            let args: Vec<String> = call.args.iter().map(expr_to_string).collect();
            format!("{}({})", func_str, args.join(", "))
        }
        _ => format!("{expr:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpython_parser as parser;

    fn parse_decorator(source: &str) -> ast::Expr {
        // Parse `@<source>\ndef f(): pass` and extract the decorator
        let code = format!("@{source}\ndef f(): pass\n");
        let parsed = parser::parse(&code, parser::Mode::Module, "<test>").unwrap();
        match parsed {
            ast::Mod::Module(m) => {
                if let ast::Stmt::FunctionDef(f) = &m.body[0] {
                    f.decorator_list[0].clone()
                } else {
                    panic!("expected FunctionDef");
                }
            }
            _ => panic!("expected Module"),
        }
    }

    #[test]
    fn simple_decorator_name() {
        let expr = parse_decorator("staticmethod");
        assert_eq!(decorator_to_string(&expr), "staticmethod");
    }

    #[test]
    fn fastapi_route_decorator() {
        let expr = parse_decorator("app.get(\"/\")");
        assert_eq!(decorator_to_string(&expr), "app.get(\"/\")");
    }

    #[test]
    fn fastapi_route_with_path() {
        let expr = parse_decorator("app.get(\"/items/{item_id}\")");
        assert_eq!(decorator_to_string(&expr), "app.get(\"/items/{item_id}\")");
    }
}
