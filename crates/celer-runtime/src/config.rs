/// Configuration for the Celer runtime.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub python_path: Option<String>,
    pub optimization_level: OptLevel,
    pub debug: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    None,
    Speed,
    Size,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            python_path: None,
            optimization_level: OptLevel::Speed,
            debug: false,
        }
    }
}
