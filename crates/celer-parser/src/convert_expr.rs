use num_traits::ToPrimitive;
use rustpython_parser::ast;

use crate::error::ParseError;
use celer_hir::{Comprehension, Expression, FStringPart, Keyword, TypeAnnotation};

/// Convert a rustpython-parser expression AST node to a HIR Expression.
pub fn convert_expr(expr: &ast::Expr) -> Result<Expression, ParseError> {
    match expr {
        ast::Expr::Constant(c) => convert_constant(c),
        ast::Expr::Name(name) => Ok(Expression::Name {
            id: name.id.to_string(),
            ty: TypeAnnotation::Unknown,
        }),
        ast::Expr::Dict(dict) => convert_dict(dict),
        ast::Expr::Call(call) => convert_call(call),
        ast::Expr::Attribute(attr) => convert_attribute(attr),
        ast::Expr::Subscript(sub) => convert_subscript(sub),
        ast::Expr::List(list) => convert_list(list),
        ast::Expr::Tuple(tuple) => convert_tuple(tuple),
        ast::Expr::BoolOp(boolop) => convert_boolop(boolop),
        ast::Expr::Compare(cmp) => convert_compare(cmp),
        ast::Expr::BinOp(binop) => convert_binop(binop),
        ast::Expr::UnaryOp(unary) => convert_unaryop(unary),
        ast::Expr::Await(a) => convert_await(a),
        ast::Expr::JoinedStr(j) => convert_fstring(j),
        ast::Expr::ListComp(lc) => convert_list_comp(lc),
        ast::Expr::DictComp(dc) => convert_dict_comp(dc),
        _ => Err(ParseError::UnsupportedFeature(format!(
            "expression: {expr:?}"
        ))),
    }
}

fn convert_constant(c: &ast::ExprConstant) -> Result<Expression, ParseError> {
    match &c.value {
        ast::Constant::Int(i) => {
            let val = i
                .to_i64()
                .ok_or_else(|| ParseError::ConversionError(format!("integer overflow: {i}")))?;
            Ok(Expression::IntLiteral(val))
        }
        ast::Constant::Float(f) => Ok(Expression::FloatLiteral(*f)),
        ast::Constant::Str(s) => Ok(Expression::StringLiteral(s.clone())),
        ast::Constant::Bool(b) => Ok(Expression::BoolLiteral(*b)),
        ast::Constant::None => Ok(Expression::NoneLiteral),
        _ => Err(ParseError::UnsupportedFeature(format!(
            "constant: {:?}",
            c.value
        ))),
    }
}

fn convert_dict(dict: &ast::ExprDict) -> Result<Expression, ParseError> {
    let mut keys = Vec::with_capacity(dict.keys.len());
    let mut values = Vec::with_capacity(dict.values.len());

    for (key_opt, val) in dict.keys.iter().zip(dict.values.iter()) {
        match key_opt {
            Some(k) => {
                keys.push(convert_expr(k)?);
                values.push(convert_expr(val)?);
            }
            None => {
                // ** unpacking -- skip for now
                return Err(ParseError::UnsupportedFeature("dict unpacking (**)".into()));
            }
        }
    }

    Ok(Expression::Dict {
        keys,
        values,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_call(call: &ast::ExprCall) -> Result<Expression, ParseError> {
    let func = convert_expr(&call.func)?;
    let args: Result<Vec<_>, _> = call.args.iter().map(convert_expr).collect();
    let mut keywords = Vec::new();
    for kw in &call.keywords {
        keywords.push(Keyword {
            name: kw.arg.as_ref().map(|a| a.to_string()),
            value: convert_expr(&kw.value)?,
        });
    }

    Ok(Expression::Call {
        func: Box::new(func),
        args: args?,
        keywords,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_attribute(attr: &ast::ExprAttribute) -> Result<Expression, ParseError> {
    let value = convert_expr(&attr.value)?;
    Ok(Expression::Attribute {
        value: Box::new(value),
        attr: attr.attr.to_string(),
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_subscript(sub: &ast::ExprSubscript) -> Result<Expression, ParseError> {
    let value = convert_expr(&sub.value)?;
    let index = convert_expr(&sub.slice)?;
    Ok(Expression::Subscript {
        value: Box::new(value),
        index: Box::new(index),
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_list(list: &ast::ExprList) -> Result<Expression, ParseError> {
    let elements: Result<Vec<_>, _> = list.elts.iter().map(convert_expr).collect();
    Ok(Expression::List {
        elements: elements?,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_tuple(tuple: &ast::ExprTuple) -> Result<Expression, ParseError> {
    let elements: Result<Vec<_>, _> = tuple.elts.iter().map(convert_expr).collect();
    Ok(Expression::Tuple {
        elements: elements?,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_binop(binop: &ast::ExprBinOp) -> Result<Expression, ParseError> {
    let left = convert_expr(&binop.left)?;
    let right = convert_expr(&binop.right)?;
    let op = convert_operator(&binop.op)?;
    Ok(Expression::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_operator(op: &ast::Operator) -> Result<celer_hir::BinaryOp, ParseError> {
    match op {
        ast::Operator::Add => Ok(celer_hir::BinaryOp::Add),
        ast::Operator::Sub => Ok(celer_hir::BinaryOp::Sub),
        ast::Operator::Mult => Ok(celer_hir::BinaryOp::Mul),
        ast::Operator::Div => Ok(celer_hir::BinaryOp::Div),
        ast::Operator::FloorDiv => Ok(celer_hir::BinaryOp::FloorDiv),
        ast::Operator::Mod => Ok(celer_hir::BinaryOp::Mod),
        ast::Operator::Pow => Ok(celer_hir::BinaryOp::Pow),
        ast::Operator::BitAnd => Ok(celer_hir::BinaryOp::BitAnd),
        ast::Operator::BitOr => Ok(celer_hir::BinaryOp::BitOr),
        ast::Operator::BitXor => Ok(celer_hir::BinaryOp::BitXor),
        ast::Operator::LShift => Ok(celer_hir::BinaryOp::LShift),
        ast::Operator::RShift => Ok(celer_hir::BinaryOp::RShift),
        _ => Err(ParseError::UnsupportedFeature(format!("operator: {op:?}"))),
    }
}

fn convert_unaryop(unary: &ast::ExprUnaryOp) -> Result<Expression, ParseError> {
    let operand = convert_expr(&unary.operand)?;
    let op = match &unary.op {
        ast::UnaryOp::USub => celer_hir::UnaryOp::Neg,
        ast::UnaryOp::Not => celer_hir::UnaryOp::Not,
        ast::UnaryOp::Invert => celer_hir::UnaryOp::BitNot,
        ast::UnaryOp::UAdd => celer_hir::UnaryOp::Pos,
    };
    Ok(Expression::UnaryOp {
        op,
        operand: Box::new(operand),
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_boolop(boolop: &ast::ExprBoolOp) -> Result<Expression, ParseError> {
    // Chain bool ops into nested binary ops
    if boolop.values.len() < 2 {
        return Err(ParseError::ConversionError(
            "BoolOp with fewer than 2 values".into(),
        ));
    }

    let op = match &boolop.op {
        ast::BoolOp::And => celer_hir::BinaryOp::And,
        ast::BoolOp::Or => celer_hir::BinaryOp::Or,
    };

    let mut result = convert_expr(&boolop.values[0])?;
    for val in &boolop.values[1..] {
        let right = convert_expr(val)?;
        result = Expression::BinaryOp {
            op: op.clone(),
            left: Box::new(result),
            right: Box::new(right),
            ty: TypeAnnotation::Unknown,
        };
    }
    Ok(result)
}

fn convert_await(a: &ast::ExprAwait) -> Result<Expression, ParseError> {
    let value = convert_expr(&a.value)?;
    Ok(Expression::Await {
        value: Box::new(value),
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_fstring(j: &ast::ExprJoinedStr) -> Result<Expression, ParseError> {
    let mut parts = Vec::new();
    for value in &j.values {
        match value {
            ast::Expr::Constant(c) => {
                if let ast::Constant::Str(s) = &c.value {
                    parts.push(FStringPart::Literal(s.clone()));
                }
            }
            ast::Expr::FormattedValue(fv) => {
                let expr = convert_expr(&fv.value)?;
                parts.push(FStringPart::Expression(Box::new(expr)));
            }
            other => {
                let expr = convert_expr(other)?;
                parts.push(FStringPart::Expression(Box::new(expr)));
            }
        }
    }
    Ok(Expression::FString {
        parts,
        ty: TypeAnnotation::Str,
    })
}

fn convert_list_comp(lc: &ast::ExprListComp) -> Result<Expression, ParseError> {
    let element = convert_expr(&lc.elt)?;
    let generators = convert_comprehensions(&lc.generators)?;
    Ok(Expression::ListComp {
        element: Box::new(element),
        generators,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_dict_comp(dc: &ast::ExprDictComp) -> Result<Expression, ParseError> {
    let key = convert_expr(&dc.key)?;
    let value = convert_expr(&dc.value)?;
    let generators = convert_comprehensions(&dc.generators)?;
    Ok(Expression::DictComp {
        key: Box::new(key),
        value: Box::new(value),
        generators,
        ty: TypeAnnotation::Unknown,
    })
}

fn convert_comprehensions(
    generators: &[ast::Comprehension],
) -> Result<Vec<Comprehension>, ParseError> {
    generators
        .iter()
        .map(|comp| {
            let target = convert_expr(&comp.target)?;
            let iter = convert_expr(&comp.iter)?;
            let conditions: Result<Vec<_>, _> = comp.ifs.iter().map(convert_expr).collect();
            Ok(Comprehension {
                target: Box::new(target),
                iter: Box::new(iter),
                conditions: conditions?,
            })
        })
        .collect()
}

fn convert_compare(cmp: &ast::ExprCompare) -> Result<Expression, ParseError> {
    // Chain comparisons into nested binary ops
    let mut left = convert_expr(&cmp.left)?;

    for (op, comparator) in cmp.ops.iter().zip(cmp.comparators.iter()) {
        let right = convert_expr(comparator)?;
        let hir_op = match op {
            ast::CmpOp::Eq => celer_hir::BinaryOp::Eq,
            ast::CmpOp::NotEq => celer_hir::BinaryOp::NotEq,
            ast::CmpOp::Lt => celer_hir::BinaryOp::Lt,
            ast::CmpOp::LtE => celer_hir::BinaryOp::LtEq,
            ast::CmpOp::Gt => celer_hir::BinaryOp::Gt,
            ast::CmpOp::GtE => celer_hir::BinaryOp::GtEq,
            _ => {
                return Err(ParseError::UnsupportedFeature(format!(
                    "comparison op: {op:?}"
                )));
            }
        };
        left = Expression::BinaryOp {
            op: hir_op,
            left: Box::new(left),
            right: Box::new(right),
            ty: TypeAnnotation::Unknown,
        };
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpython_parser as parser;

    fn parse_expr(source: &str) -> ast::Expr {
        let parsed = parser::parse(source, parser::Mode::Expression, "<test>").unwrap();
        match parsed {
            ast::Mod::Expression(e) => *e.body,
            _ => panic!("expected Expression"),
        }
    }

    #[test]
    fn int_literal() {
        let expr = parse_expr("42");
        let hir = convert_expr(&expr).unwrap();
        assert_eq!(hir, Expression::IntLiteral(42));
    }

    #[test]
    fn string_literal() {
        let expr = parse_expr("\"hello\"");
        let hir = convert_expr(&expr).unwrap();
        assert_eq!(hir, Expression::StringLiteral("hello".into()));
    }

    #[test]
    fn bool_literal() {
        let expr = parse_expr("True");
        let hir = convert_expr(&expr).unwrap();
        assert_eq!(hir, Expression::BoolLiteral(true));
    }

    #[test]
    fn none_literal() {
        let expr = parse_expr("None");
        let hir = convert_expr(&expr).unwrap();
        assert_eq!(hir, Expression::NoneLiteral);
    }

    #[test]
    fn name_expression() {
        let expr = parse_expr("x");
        let hir = convert_expr(&expr).unwrap();
        assert_eq!(
            hir,
            Expression::Name {
                id: "x".into(),
                ty: TypeAnnotation::Unknown
            }
        );
    }

    #[test]
    fn dict_literal() {
        let expr = parse_expr("{\"key\": \"value\"}");
        let hir = convert_expr(&expr).unwrap();
        match hir {
            Expression::Dict { keys, values, .. } => {
                assert_eq!(keys.len(), 1);
                assert_eq!(values.len(), 1);
                assert_eq!(keys[0], Expression::StringLiteral("key".into()));
                assert_eq!(values[0], Expression::StringLiteral("value".into()));
            }
            _ => panic!("expected Dict expression"),
        }
    }

    #[test]
    fn call_expression() {
        let expr = parse_expr("foo(1, 2)");
        let hir = convert_expr(&expr).unwrap();
        match hir {
            Expression::Call { func, args, .. } => {
                assert!(matches!(*func, Expression::Name { ref id, .. } if id == "foo"));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected Call expression"),
        }
    }

    #[test]
    fn attribute_expression() {
        let expr = parse_expr("obj.attr");
        let hir = convert_expr(&expr).unwrap();
        match hir {
            Expression::Attribute { value, attr, .. } => {
                assert!(matches!(*value, Expression::Name { ref id, .. } if id == "obj"));
                assert_eq!(attr, "attr");
            }
            _ => panic!("expected Attribute expression"),
        }
    }
}
