use celer_hir::{Expression, Module, Statement, TypeAnnotation};

use crate::context::TypeContext;
use crate::error::TypeError;

/// Walks a HIR module and resolves Unknown types where possible.
pub struct InferenceEngine {
    ctx: TypeContext,
}

impl InferenceEngine {
    pub fn new() -> Self {
        Self {
            ctx: TypeContext::new(),
        }
    }

    /// Run type inference on a module, resolving Unknown types.
    pub fn infer_module(&mut self, module: &mut Module) -> Result<(), TypeError> {
        for stmt in &mut module.body {
            self.infer_statement(stmt)?;
        }
        Ok(())
    }

    fn infer_statement(&mut self, stmt: &mut Statement) -> Result<(), TypeError> {
        match stmt {
            Statement::Assign {
                target,
                annotation,
                value,
            } => {
                let inferred = self.infer_expression(value)?;
                let resolved = if let Some(ann) = annotation {
                    if *ann != TypeAnnotation::Unknown {
                        ann.clone()
                    } else {
                        inferred
                    }
                } else {
                    inferred
                };
                self.ctx.define(target.clone(), resolved);
                Ok(())
            }
            Statement::Return { value } => {
                if let Some(expr) = value {
                    self.infer_expression(expr)?;
                }
                Ok(())
            }
            Statement::If { test, body, orelse } => {
                self.infer_expression(test)?;
                self.ctx.push_scope();
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                self.ctx.push_scope();
                for s in orelse {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::While { test, body } => {
                self.infer_expression(test)?;
                self.ctx.push_scope();
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::For { target, iter, body } => {
                self.infer_expression(iter)?;
                self.ctx.push_scope();
                // Infer the iteration variable type from the iterable
                let iter_ty = self.infer_expression(iter)?;
                let elem_ty = match iter_ty {
                    TypeAnnotation::List(inner) => *inner,
                    TypeAnnotation::Set(inner) => *inner,
                    _ => TypeAnnotation::Unknown,
                };
                self.ctx.define(target.clone(), elem_ty);
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::Expr(expr) => {
                self.infer_expression(expr)?;
                Ok(())
            }
            Statement::AugAssign { target, value, .. } => {
                self.infer_expression(value)?;
                // The target type stays unchanged for augmented assignment
                if self.ctx.lookup(target).is_none() {
                    return Err(TypeError::UndefinedVariable(target.clone()));
                }
                Ok(())
            }
            Statement::Assert { test, msg } => {
                self.infer_expression(test)?;
                if let Some(m) = msg {
                    self.infer_expression(m)?;
                }
                Ok(())
            }
            Statement::Raise { value } => {
                if let Some(expr) = value {
                    self.infer_expression(expr)?;
                }
                Ok(())
            }
            // Statements that don't carry type information
            Statement::FunctionDef(_)
            | Statement::ClassDef { .. }
            | Statement::Import { .. }
            | Statement::ImportFrom { .. }
            | Statement::Pass
            | Statement::Break
            | Statement::Continue => Ok(()),
        }
    }

    fn infer_expression(&self, expr: &Expression) -> Result<TypeAnnotation, TypeError> {
        match expr {
            Expression::Name { id, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                self.ctx
                    .lookup(id)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedVariable(id.clone()))
            }
            _ => Ok(expr.ty().clone()),
        }
    }
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::Module;

    #[test]
    fn infer_assign_from_literal() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "x".into(),
            annotation: None,
            value: Expression::IntLiteral(42),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("x"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn infer_respects_explicit_annotation() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "y".into(),
            annotation: Some(TypeAnnotation::Float),
            value: Expression::IntLiteral(1),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        // Explicit annotation wins over inferred type
        assert_eq!(engine.ctx.lookup("y"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_resolves_unknown_annotation() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "z".into(),
            annotation: Some(TypeAnnotation::Unknown),
            value: Expression::StringLiteral("hello".into()),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("z"), Some(&TypeAnnotation::Str));
    }

    #[test]
    fn infer_name_lookup() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "a".into(),
            annotation: None,
            value: Expression::IntLiteral(10),
        });
        module.body.push(Statement::Assign {
            target: "b".into(),
            annotation: None,
            value: Expression::Name {
                id: "a".into(),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("b"), Some(&TypeAnnotation::Int));
    }
}
