pub mod bench;
pub mod cloud;
pub mod compile;
pub mod run;
pub mod serve;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compile a Python file to native code
    Compile(compile::CompileArgs),
    /// Run a Python file through the Celer pipeline
    Run(run::RunArgs),
    /// Run built-in benchmarks comparing CPython vs Celer AOT
    Bench(bench::BenchArgs),
    /// Serve a Python web app with compiled native handlers
    Serve(serve::ServeArgs),
    /// Start the cloud compilation service
    Cloud(cloud::CloudArgs),
}
