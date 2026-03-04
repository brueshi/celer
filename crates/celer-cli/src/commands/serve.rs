use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::pipeline;

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Module and app variable in format `module:app` (e.g., `main:app`)
    pub target: String,

    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8000)]
    pub port: u16,
}

pub fn execute(args: &ServeArgs) -> Result<()> {
    // Parse module:app notation
    let (module_path, _app_var) = parse_target(&args.target)?;

    let source =
        std::fs::read_to_string(&module_path).with_context(|| format!("failed to read {module_path}"))?;

    let name = PathBuf::from(&module_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string();

    // Set up temp directory for compilation artifacts
    let temp_dir = std::env::temp_dir().join("celer-serve");
    std::fs::create_dir_all(&temp_dir)?;

    let obj_path = temp_dir.join(format!("{name}.o"));
    let ext = celer_runtime::shared_lib_extension();
    let lib_path = temp_dir.join(format!("{name}.{ext}"));

    // Full pipeline: parse -> infer -> analyze -> compile -> link
    let report = pipeline::compile_to_object_with_report(&name, &module_path, &source, &obj_path)
        .context("compilation failed")?;

    // Print compilability report
    println!("Compilability report:");
    if !report.compiled_functions.is_empty() {
        let mut funcs: Vec<&String> = report.compiled_functions.iter().collect();
        funcs.sort();
        for f in funcs {
            println!("  [native] {f}");
        }
    }
    for (fname, reason) in &report.skipped_functions {
        println!("  [skip]   {fname}: {reason}");
    }
    println!();

    // Link to shared library
    celer_runtime::link_shared(&obj_path, &lib_path).context("linking failed")?;

    // Parse module to extract FastAPI routes
    let mut module =
        celer_parser::parse_module(&name, &module_path, &source).context("parsing failed")?;
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    let routes = celer_fastapi::FastApiAdapter::extract_routes(&module)
        .map_err(|e| anyhow::anyhow!("route extraction failed: {e}"))?;

    if routes.is_empty() {
        bail!("no FastAPI routes found in {module_path}");
    }

    // Build router
    let mut router = celer_server::Router::new();
    let mut route_count = 0;

    for route_info in &routes {
        let method = match route_info.method {
            celer_fastapi::HttpMethod::Get => "GET",
            celer_fastapi::HttpMethod::Post => "POST",
            celer_fastapi::HttpMethod::Put => "PUT",
            celer_fastapi::HttpMethod::Delete => "DELETE",
            celer_fastapi::HttpMethod::Patch => "PATCH",
        };

        let is_json = report.json_functions.contains(&route_info.handler.name);
        let is_compiled = report.compiled_functions.contains(&route_info.handler.name);

        let mut path_params = Vec::new();
        let mut param_types = Vec::new();
        for param in &route_info.params {
            if param.source == celer_fastapi::route::ParamSource::Path {
                path_params.push(param.name.clone());
                let pt = match param.ty {
                    celer_hir::TypeAnnotation::Int => celer_server::ParamType::Int,
                    _ => celer_server::ParamType::Str,
                };
                param_types.push(pt);
            }
        }

        let status = if is_compiled { "compiled" } else { "fallback" };
        println!(
            "  {method} {} -> {} [{status}]",
            route_info.path, route_info.handler.name
        );

        router.add_route(
            method,
            &route_info.path,
            celer_server::CompiledRoute {
                handler_name: route_info.handler.name.clone(),
                is_json,
                path_params,
                param_types,
            },
        );
        route_count += 1;
    }

    println!();
    println!("Registered {route_count} route(s)");

    // Load native module
    let native = unsafe {
        celer_runtime::NativeModule::load(&lib_path).context("failed to load native module")?
    };

    let config = celer_server::ServerConfig {
        host: args.host.clone(),
        port: args.port,
    };

    println!(
        "Starting Celer server on http://{}:{}",
        config.host, config.port
    );
    println!();

    // Run async server
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let server = celer_server::CelerServer::new(config, router, native);
        server
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("server error: {e}"))
    })?;

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

/// Parse `module:app` or `module.py:app` notation into (file_path, app_var).
fn parse_target(target: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = target.splitn(2, ':').collect();
    if parts.len() != 2 {
        bail!("expected format `module:app` (e.g., `main:app`), got `{target}`");
    }

    let mut module_path = parts[0].to_string();
    let app_var = parts[1].to_string();

    // Add .py extension if not present
    if !module_path.ends_with(".py") {
        module_path.push_str(".py");
    }

    Ok((module_path, app_var))
}
