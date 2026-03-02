use serde::{Deserialize, Serialize};

use crate::stmt::Statement;

/// HIR representation of a Python module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub path: String,
    pub body: Vec<Statement>,
}

impl Module {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            body: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_module() {
        let m = Module::new("main", "main.py");
        assert_eq!(m.name, "main");
        assert!(m.body.is_empty());
    }
}
