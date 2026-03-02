use std::path::PathBuf;

/// Represents a compiled native module ready for execution.
pub struct CompiledModule {
    pub name: String,
    pub object_path: PathBuf,
    pub entry_point: Option<String>,
}

impl CompiledModule {
    pub fn new(name: impl Into<String>, object_path: PathBuf) -> Self {
        Self {
            name: name.into(),
            object_path,
            entry_point: None,
        }
    }

    pub fn with_entry_point(mut self, entry: impl Into<String>) -> Self {
        self.entry_point = Some(entry.into());
        self
    }
}
