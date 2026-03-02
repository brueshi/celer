use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::pipeline;

#[derive(Debug, Args)]
pub struct CompileArgs {
    /// Path to the Python source file
    pub input: PathBuf,

    /// Output path for the compiled artifact
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Emit LLVM IR instead of native code
    #[arg(long)]
    pub emit_ir: bool,
}

pub fn execute(args: &CompileArgs) -> Result<()> {
    let source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;

    let name = args
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    let path = args.input.to_string_lossy();

    let ir = pipeline::compile(name, &path, &source)?;

    if args.emit_ir {
        let out = args
            .output
            .clone()
            .unwrap_or_else(|| args.input.with_extension("ll"));
        std::fs::write(&out, &ir).with_context(|| format!("failed to write {}", out.display()))?;
        println!("LLVM IR written to {}", out.display());
    } else {
        // Native codegen is not yet implemented; emit IR as default
        println!("{ir}");
    }

    Ok(())
}
