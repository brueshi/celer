use std::path::{Path, PathBuf};
use std::sync::Arc;

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

    /// Number of tokio worker threads
    #[arg(long, default_value_t = 4)]
    pub workers: usize,

    /// Maximum request body size in bytes
    #[arg(long, default_value_t = 1_048_576)]
    pub max_body_size: usize,

    /// Disable native compilation (pure ASGI pass-through for debugging)
    #[arg(long)]
    pub no_native: bool,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

pub fn execute(args: &ServeArgs) -> Result<()> {
    let (module_path, app_var) = parse_target(&args.target)?;

    let source = std::fs::read_to_string(&module_path)
        .with_context(|| format!("failed to read {module_path}"))?;

    let name = PathBuf::from(&module_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string();

    println!("Celer v0.1.0 -- Hybrid AOT Python Server");
    println!("Analyzing {module_path}...");
    println!();

    // Parse and analyze the module
    let mut module = celer_parser::parse_module(&name, &module_path, &source)
        .context("parsing failed")?;
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    // Detect framework and extract routes
    let adapters: Vec<Box<dyn celer_adapter_core::FrameworkAdapter>> = vec![
        Box::new(celer_fastapi::FastApiAdapter),
        Box::new(celer_flask::FlaskAdapter),
        Box::new(celer_django::DjangoAdapter),
    ];

    let (framework_name, routes) =
        celer_adapter_core::detect::detect_and_extract(&module, &adapters)
            .map_err(|e| anyhow::anyhow!("route extraction failed: {e}"))?;

    // Compile native handlers (unless --no-native)
    let (router, native_module, native_count, asgi_count) = if args.no_native {
        println!("Compilability report:");
        println!("  [--no-native] all routes forwarded to ASGI");
        println!();
        let router = celer_server::Router::new();
        let native = create_empty_native()?;
        (router, native, 0, routes.len())
    } else {
        build_native_routes(&name, &module_path, &source, &routes)?
    };

    println!("Detected framework: {framework_name}");
    println!(
        "Compiled {native_count}/{} handlers to native code",
        native_count + asgi_count
    );
    println!();

    // Print route table
    println!("Route table:");
    for route_info in &routes {
        let method = method_str(&route_info.method);
        let disposition = if !args.no_native
            && router
                .match_route(method, &route_info.path)
                .is_some()
        {
            "native"
        } else {
            "asgi"
        };
        println!(
            "  {method:<6} {:<20} -> {:<20} [{disposition}]",
            route_info.path, route_info.handler.name
        );
    }
    println!();

    // Initialize Python runtime
    println!("Starting Python runtime...");
    let module_dir = Path::new(&module_path)
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."));

    let host = celer_pyhost::PythonHost::new(&module_dir, &name, &app_var)
        .map_err(|e| anyhow::anyhow!("Python init failed: {e}"))?;

    println!("  Imported {name}:{app_var} ({framework_name})");
    println!("  ASGI fallback ready");
    println!();

    let host = Arc::new(host);
    let asgi =
        celer_pyhost::AsgiDispatcher::new(host, args.host.clone(), args.port);

    let config = celer_server::ServerConfig {
        host: args.host.clone(),
        port: args.port,
        max_body_size: args.max_body_size,
    };

    println!(
        "Listening on http://{}:{}",
        config.host, config.port
    );

    // Run hybrid server
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.workers)
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    rt.block_on(async {
        let server =
            celer_server::HybridServer::new(config, router, native_module, asgi);
        server
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("server error: {e}"))
    })?;

    Ok(())
}

/// Build native routes: compile, link, and register in router.
fn build_native_routes(
    name: &str,
    module_path: &str,
    source: &str,
    routes: &[celer_adapter_core::RouteInfo],
) -> Result<(celer_server::Router, celer_runtime::NativeModule, usize, usize)> {
    let temp_dir = std::env::temp_dir().join("celer-serve");
    std::fs::create_dir_all(&temp_dir)?;

    let obj_path = temp_dir.join(format!("{name}.o"));
    let ext = celer_runtime::shared_lib_extension();
    let lib_path = temp_dir.join(format!("{name}.{ext}"));

    let report =
        pipeline::compile_to_object_with_report(name, module_path, source, &obj_path)
            .context("compilation failed")?;

    // Print compilability report
    println!("Compilability report:");
    if !report.compiled_functions.is_empty() {
        let mut funcs: Vec<&String> = report.compiled_functions.iter().collect();
        funcs.sort();
        for f in funcs {
            println!("  [native]   {f}");
        }
    }
    for (fname, reason) in &report.skipped_functions {
        println!("  [asgi]     {fname} -- {reason}");
    }
    println!();

    // Link to shared library
    celer_runtime::link_shared(&obj_path, &lib_path).context("linking failed")?;

    let native = unsafe {
        celer_runtime::NativeModule::load(&lib_path).context("failed to load native module")?
    };

    // Build router with only compiled routes
    let mut router = celer_server::Router::new();
    let mut native_count = 0;
    let mut asgi_count = 0;

    for route_info in routes {
        let method = method_str(&route_info.method);
        let is_compiled = report.compiled_functions.contains(&route_info.handler.name);

        if !is_compiled {
            asgi_count += 1;
            continue;
        }

        let is_json = report.json_functions.contains(&route_info.handler.name);

        let mut path_params = Vec::new();
        let mut param_types = Vec::new();
        for param in &route_info.params {
            if param.source == celer_adapter_core::ParamSource::Path {
                path_params.push(param.name.clone());
                let pt = match param.ty {
                    celer_hir::TypeAnnotation::Int => celer_server::ParamType::Int,
                    _ => celer_server::ParamType::Str,
                };
                param_types.push(pt);
            }
        }

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
        native_count += 1;
    }

    Ok((router, native, native_count, asgi_count))
}

/// Create an empty native module (for --no-native mode).
fn create_empty_native() -> Result<celer_runtime::NativeModule> {
    let temp_dir = std::env::temp_dir().join("celer-serve-empty");
    std::fs::create_dir_all(&temp_dir)?;

    let obj_path = temp_dir.join("empty.o");
    let ext = celer_runtime::shared_lib_extension();
    let lib_path = temp_dir.join(format!("empty.{ext}"));

    // Compile an empty Python source to produce a valid shared library
    pipeline::compile_to_object("empty", "<empty>", "", &obj_path)
        .context("compiling empty module")?;
    celer_runtime::link_shared(&obj_path, &lib_path).context("linking empty module")?;

    let native = unsafe {
        celer_runtime::NativeModule::load(&lib_path).context("failed to load empty module")?
    };

    Ok(native)
}

fn method_str(method: &celer_adapter_core::HttpMethod) -> &'static str {
    match method {
        celer_adapter_core::HttpMethod::Get => "GET",
        celer_adapter_core::HttpMethod::Post => "POST",
        celer_adapter_core::HttpMethod::Put => "PUT",
        celer_adapter_core::HttpMethod::Delete => "DELETE",
        celer_adapter_core::HttpMethod::Patch => "PATCH",
    }
}

/// Parse `module:app` or `module.py:app` notation into (file_path, app_var).
fn parse_target(target: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = target.splitn(2, ':').collect();
    if parts.len() != 2 {
        bail!("expected format `module:app` (e.g., `main:app`), got `{target}`");
    }

    let mut module_path = parts[0].to_string();
    let app_var = parts[1].to_string();

    if !module_path.ends_with(".py") {
        module_path.push_str(".py");
    }

    Ok((module_path, app_var))
}
