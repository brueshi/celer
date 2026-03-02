use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the Python source file
    pub input: PathBuf,
}

pub fn execute(args: &RunArgs) -> Result<()> {
    let _source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;

    // TODO: full pipeline -- parse, infer, codegen, execute via runtime bridge
    anyhow::bail!("run command not yet implemented (use `celerate compile` to emit IR)")
}
