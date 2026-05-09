use lume_backend_native::{
    DynamicLibraryResolver, NativeAbiType, NativeBackend, NativeBridgeModule, NativeBridgePlan,
    NativeSymbol, ResolvedNativeModule,
};
use lume_diagnostics::{emit, Diagnostic};
use lume_ffi::FfiRegistry;
use lume_session::{BuildOptions, Session};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug)]
pub struct BuildResult {
    pub diagnostics: Vec<Diagnostic>,
    pub emitted: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct BenchMetric {
    pub name: &'static str,
    pub duration: Duration,
}

#[derive(Clone, Debug)]
pub struct BenchResult {
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed: Duration,
    pub metrics: Vec<BenchMetric>,
    pub emitted_files: usize,
    pub bundle_bytes: u64,
}

struct BuildExecution {
    diagnostics: Vec<Diagnostic>,
    emitted: Vec<String>,
    metrics: Vec<BenchMetric>,
}

#[repr(C)]
struct NativeBytes {
    ptr: *mut u8,
    len: usize,
}

const MAX_NATIVE_BYTES_LEN: usize = 64 * 1024 * 1024;

#[allow(dead_code)]
#[derive(Clone, Debug)]
enum NativeScalarValue {
    I32(i32),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
struct NativeBridgeRuntime {
    plan: NativeBridgePlan,
    resolved: Vec<ResolvedNativeModule>,
    _resolver: DynamicLibraryResolver,
}

pub fn build(options: BuildOptions) -> io::Result<BuildResult> {
    let execution = build_execution(options, false)?;
    Ok(BuildResult {
        diagnostics: execution.diagnostics,
        emitted: execution.emitted,
    })
}

pub fn bench(options: BuildOptions) -> io::Result<BenchResult> {
    let bench_out_dir = temporary_bench_out_dir();
    let mut bench_options = options.clone();
    bench_options.out_dir = bench_out_dir.clone();

    let started = Instant::now();
    let execution = build_execution(bench_options, true)?;
    let elapsed = started.elapsed();
    let bundle_bytes = execution.emitted.iter().try_fold(0u64, |total, path| {
        fs::metadata(path).map(|metadata| total + metadata.len())
    })?;
    let _ = fs::remove_dir_all(&bench_out_dir);

    Ok(BenchResult {
        diagnostics: execution.diagnostics,
        elapsed,
        metrics: execution.metrics,
        emitted_files: execution.emitted.len(),
        bundle_bytes,
    })
}

fn build_execution(options: BuildOptions, record_metrics: bool) -> io::Result<BuildExecution> {
    let mut session = Session::default();
    let mut metrics = Vec::new();

    let load_started = Instant::now();
    let file = session.load_source(&options.entry)?;
    push_bench_metric(&mut metrics, record_metrics, "source load", load_started);

    let parse_started = Instant::now();
    let (program, mut diagnostics) = lume_parser::parse(&file.source);
    push_bench_metric(&mut metrics, record_metrics, "parse", parse_started);
    if !diagnostics.has_errors() {
        let lower_started = Instant::now();
        let hir = lume_hir::lower(program);
        push_bench_metric(&mut metrics, record_metrics, "lower", lower_started);

        let resolve_started = Instant::now();
        diagnostics
            .extend(lume_resolver::resolve_with_base(&hir, options.entry.parent()).into_vec());
        let component_symbols =
            lume_resolver::component_symbols_with_base(&hir, options.entry.parent());
        push_bench_metric(&mut metrics, record_metrics, "resolve", resolve_started);

        let typecheck_started = Instant::now();
        diagnostics
            .extend(lume_typeck::check_with_known_components(&hir, &component_symbols).into_vec());
        push_bench_metric(&mut metrics, record_metrics, "typecheck", typecheck_started);

        if !diagnostics.has_errors() {
            let ir_started = Instant::now();
            match lume_ir::build_with_base(&hir, options.entry.parent()) {
                Ok(ir) => {
                    push_bench_metric(&mut metrics, record_metrics, "IR generation", ir_started);

                    let html_started = Instant::now();
                    let html = lume_codegen_html::generate_with_resume(
                        &ir,
                        options.activation.is_resume(),
                    );
                    push_bench_metric(&mut metrics, record_metrics, "HTML emit", html_started);

                    let css_started = Instant::now();
                    let css = lume_codegen_css::generate(&ir);
                    push_bench_metric(&mut metrics, record_metrics, "CSS emit", css_started);

                    let js_started = Instant::now();
                    let js = lume_codegen_js::generate_with_options(
                        &ir,
                        &html,
                        options.target.wasm_enabled(),
                        options.activation.is_resume(),
                    );
                    push_bench_metric(&mut metrics, record_metrics, "JS emit", js_started);

                    write_dist(
                        &options,
                        &ir,
                        &html,
                        &css,
                        &js,
                        record_metrics,
                        &mut metrics,
                    )?;

                    let mut emitted = vec![
                        options.out_dir.join("index.html").display().to_string(),
                        options.out_dir.join("assets/app.js").display().to_string(),
                        options
                            .out_dir
                            .join("assets/style.css")
                            .display()
                            .to_string(),
                        options
                            .out_dir
                            .join("assets/lume.manifest.json")
                            .display()
                            .to_string(),
                        options
                            .out_dir
                            .join("assets/lume.backend.json")
                            .display()
                            .to_string(),
                    ];
                    if options.target.wasm_enabled() {
                        emitted.push(
                            options
                                .out_dir
                                .join("assets/app.wasm")
                                .display()
                                .to_string(),
                        );
                    }
                    if diagnostics.has_errors() {
                        eprint!("{}", emit(diagnostics.as_slice(), Some(&file)));
                    }
                    return Ok(BuildExecution {
                        diagnostics: diagnostics.into_vec(),
                        emitted,
                        metrics,
                    });
                }
                Err(more) => diagnostics.extend(more.into_vec()),
            }
        }
    }
    if diagnostics.has_errors() {
        eprint!("{}", emit(diagnostics.as_slice(), Some(&file)));
    }
    Ok(BuildExecution {
        diagnostics: diagnostics.into_vec(),
        emitted: Vec::new(),
        metrics,
    })
}

pub fn check(options: BuildOptions) -> io::Result<BuildResult> {
    let mut session = Session::default();
    let file = session.load_source(&options.entry)?;
    let (program, mut diagnostics) = lume_parser::parse(&file.source);
    if !diagnostics.has_errors() {
        let hir = lume_hir::lower(program);
        diagnostics
            .extend(lume_resolver::resolve_with_base(&hir, options.entry.parent()).into_vec());
        let component_symbols =
            lume_resolver::component_symbols_with_base(&hir, options.entry.parent());
        diagnostics
            .extend(lume_typeck::check_with_known_components(&hir, &component_symbols).into_vec());
        if !diagnostics.has_errors() {
            if let Err(more) = lume_ir::build_with_base(&hir, options.entry.parent()) {
                diagnostics.extend(more.into_vec());
            }
        }
    }
    if !diagnostics.is_empty() {
        eprint!("{}", emit(diagnostics.as_slice(), Some(&file)));
    }
    Ok(BuildResult {
        diagnostics: diagnostics.into_vec(),
        emitted: Vec::new(),
    })
}

pub fn fmt(path: &Path, check_only: bool) -> io::Result<bool> {
    let source = fs::read_to_string(path)?;
    let formatted = lume_formatter::format_source(&source);
    if check_only {
        Ok(source == formatted)
    } else {
        fs::write(path, formatted)?;
        Ok(true)
    }
}

pub fn init() -> io::Result<()> {
    fs::create_dir_all("src")?;
    if !Path::new("lume.toml").exists() {
        fs::write(
            "lume.toml",
            "[project]\nname = \"lume-app\"\nentry = \"src/app.lume\"\nout_dir = \"dist\"\n\n[build]\ntarget = \"html-js-css-wasm\"\n",
        )?;
    }
    if !Path::new("src/app.lume").exists() {
        fs::write("src/app.lume", "component App {\n  state count: i32 = 0\n\n  view {\n    Column gap=12 padding=16 {\n      Text(\"Count: {count}\")\n\n      Button(\"増やす\") {\n        on click {\n          count += 1\n        }\n      }\n    }\n  }\n}\n")?;
    }
    Ok(())
}

fn temporary_bench_out_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("lume-bench-{}-{}", std::process::id(), stamp))
}

fn push_bench_metric(
    metrics: &mut Vec<BenchMetric>,
    enabled: bool,
    name: &'static str,
    started: Instant,
) {
    if enabled {
        metrics.push(BenchMetric {
            name,
            duration: started.elapsed(),
        });
    }
}

pub fn dev(options: BuildOptions, port: u16) -> io::Result<()> {
    let result = build(options.clone())?;
    if result
        .diagnostics
        .iter()
        .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
    {
        eprintln!("lume dev: initial build failed");
    }

    let watch_options = options.clone();
    thread::spawn(move || {
        if let Err(err) = watch_loop(watch_options) {
            eprintln!("lume dev: watch failed: {err}");
        }
    });

    serve(options, port)
}

fn watch_loop(options: BuildOptions) -> io::Result<()> {
    let mut last_modified = SystemTime::UNIX_EPOCH;
    loop {
        let modified = fs::metadata(&options.entry)?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if modified > last_modified {
            last_modified = modified;
            let result = build(options.clone())?;
            if result
                .diagnostics
                .iter()
                .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
            {
                eprintln!("lume dev: build failed");
            } else {
                eprintln!("lume dev: rebuilt {}", options.entry.display());
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn serve(options: BuildOptions, port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let local_addr = listener.local_addr()?;
    eprintln!("lume dev: serving http://{local_addr}");
    eprintln!("lume dev: serving files from {}", options.out_dir.display());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let options = options.clone();
                thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, &options) {
                        eprintln!("lume dev: request failed: {err}");
                    }
                });
            }
            Err(err) => eprintln!("lume dev: connection failed: {err}"),
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, options: &BuildOptions) -> io::Result<()> {
    let request = read_http_request(&mut stream)?;
    let method = request.method.as_str();
    let target = request.target.as_str();

    if method == "POST" {
        if let Some(action_id) = action_id_from_target(target) {
            return handle_action_request(
                &mut stream,
                options,
                &action_id,
                &request.headers,
                &request.body,
            );
        }
    }

    if matches!(method, "GET" | "HEAD") {
        if let Some((module_name, symbol_name, args)) = native_bridge_id_from_target(target) {
            return handle_native_request(
                &mut stream,
                options,
                &module_name,
                &symbol_name,
                &args,
                method != "HEAD",
            );
        }
    }

    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            method != "HEAD",
            b"Method Not Allowed",
        );
    }

    let path = resolve_request_path(&options.out_dir, target)
        .or_else(|| spa_fallback_path(&options.out_dir, target));
    let Some(path) = path else {
        return write_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            method != "HEAD",
            b"Bad Request",
        );
    };

    if let Ok(body) = fs::read(&path) {
        let body = response_body_with_dev_csrf(&path, body);
        return write_response(
            &mut stream,
            "200 OK",
            mime_type(&path),
            method != "HEAD",
            body.as_slice(),
        );
    }

    if let Some(fallback) = spa_fallback_path(&options.out_dir, target) {
        if let Ok(body) = fs::read(&fallback) {
            let body = response_body_with_dev_csrf(&fallback, body);
            return write_response(
                &mut stream,
                "200 OK",
                mime_type(&fallback),
                method != "HEAD",
                body.as_slice(),
            );
        }
    }

    write_response(
        &mut stream,
        "404 Not Found",
        "text/plain; charset=utf-8",
        method != "HEAD",
        b"Not Found",
    )
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut temp = [0; 4096];
    let mut header_end = None;
    let mut content_length = 0usize;
    loop {
        let bytes = stream.read(&mut temp)?;
        if bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..bytes]);
        if header_end.is_none() {
            if let Some(index) = find_header_end(&buffer) {
                header_end = Some(index);
                content_length = parse_content_length(&buffer[..index]);
            }
        }
        if let Some(index) = header_end {
            let body_start = index + 4;
            if buffer.len().saturating_sub(body_start) >= content_length {
                break;
            }
        }
    }
    let header_end = header_end.unwrap_or(buffer.len());
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let mut parts = headers
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let headers_vec = headers
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    let body_start = (header_end + 4).min(buffer.len());
    let body_end = (body_start + content_length).min(buffer.len());
    Ok(HttpRequest {
        method,
        target,
        headers: headers_vec,
        body: buffer[body_start..body_end].to_vec(),
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> usize {
    let headers = String::from_utf8_lossy(headers);
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn action_id_from_target(target: &str) -> Option<String> {
    let path = target.split(['?', '#']).next().unwrap_or(target);
    let id = path.strip_prefix("/__lume/actions/")?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    percent_decode(id)
}

fn native_bridge_id_from_target(target: &str) -> Option<(String, String, Vec<String>)> {
    let path = target.split(['?', '#']).next().unwrap_or(target);
    let path = path.strip_prefix("/__lume/native/")?;
    let mut segments = path.split('/');
    let module = percent_decode(segments.next()?)?;
    let symbol = percent_decode(segments.next()?)?;
    if segments.next().is_some() {
        return None;
    }
    let query = target
        .split_once('?')
        .map(|(_, query)| query.split('#').next().unwrap_or(query))
        .unwrap_or("");
    let args = native_bridge_query_args(query);
    Some((module, symbol, args))
}

fn native_bridge_query_args(query: &str) -> Vec<String> {
    let mut args = Vec::new();
    for item in query.split('&') {
        let Some((key, value)) = item.split_once('=') else {
            continue;
        };
        if key == "args" {
            if let Some(value) = percent_decode_query(value) {
                args.push(value);
            }
        }
    }
    args
}

fn handle_action_request(
    stream: &mut TcpStream,
    options: &BuildOptions,
    action_id: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> io::Result<()> {
    let runtime = server_runtime(options)?;
    let context = lume_runtime_server::ActionRequestContext {
        csrf_token: request_header(headers, "x-lume-csrf"),
        expected_csrf_token: Some(dev_csrf_token()),
        authenticated: false,
        roles: Vec::new(),
        permissions: Vec::new(),
        rate_limit_exceeded: action_rate_limited(action_id, headers),
    };
    let body = std::str::from_utf8(body).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("action body is not utf-8: {err}"),
        )
    })?;
    let wants_stream = request_header(headers, "accept")
        .is_some_and(|accept| accept.contains("text/event-stream"));
    let response = if wants_stream {
        runtime
            .call_sse_with_context(action_id, body, &context)
            .map(|body| ("text/event-stream; charset=utf-8", body))
    } else {
        runtime
            .call_json_with_context(action_id, body, &context)
            .map(|body| ("application/json; charset=utf-8", body))
    };
    match response {
        Ok(response) => write_response(stream, "200 OK", response.0, true, response.1.as_bytes()),
        Err(err) => {
            let body = format!(
                "{{\"error\":{{\"code\":\"ACTION_FAILED\",\"message\":\"{}\",\"status\":{}}}}}",
                json_escape(&err.message),
                err.status
            );
            write_response(
                stream,
                status_line(err.status),
                "application/json; charset=utf-8",
                true,
                body.as_bytes(),
            )
        }
    }
}

fn handle_native_request(
    stream: &mut TcpStream,
    options: &BuildOptions,
    module_name: &str,
    symbol_name: &str,
    args: &[String],
    include_body: bool,
) -> io::Result<()> {
    let runtime = match native_bridge_runtime(options) {
        Ok(runtime) => runtime,
        Err(err) => {
            let body = err.to_string();
            return write_response(
                stream,
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                include_body,
                body.as_bytes(),
            );
        }
    };
    let Some((module, resolved_module)) = runtime.module(module_name) else {
        return write_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            include_body,
            b"Unknown native module",
        );
    };
    let bytes = match invoke_native_buffer(module, resolved_module, symbol_name, args) {
        Ok(bytes) => bytes,
        Err(err) => {
            return write_response(
                stream,
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                include_body,
                err.as_bytes(),
            );
        }
    };
    write_response(
        stream,
        "200 OK",
        "application/octet-stream",
        include_body,
        &bytes,
    )
}

fn native_bridge_runtime(options: &BuildOptions) -> io::Result<NativeBridgeRuntime> {
    let source = fs::read_to_string(&options.entry)?;
    let (program, diagnostics) = lume_parser::parse(&source);
    if diagnostics.has_errors() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot execute native bridge because parsing failed",
        ));
    }
    let hir = lume_hir::lower(program);
    let ir = lume_ir::build_with_base(&hir, options.entry.parent()).map_err(|diagnostics| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "cannot execute native bridge because IR build failed with {} diagnostics",
                diagnostics.as_slice().len()
            ),
        )
    })?;
    let ffi_registry = FfiRegistry::from_ast(
        &ir.ffi_modules,
        &ir.ffi_structs,
        &ir.ffi_enums,
        &ir.ffi_opaques,
    );
    let mut plan = NativeBackend.plan_ffi(&ffi_registry).map_err(|errors| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "cannot execute native bridge because {} FFI errors",
                errors.len()
            ),
        )
    })?;
    absolutize_native_libraries(&mut plan, &project_root_for_entry(&options.entry));
    let resolver = DynamicLibraryResolver::default();
    let resolved = plan.resolve(&resolver).map_err(|errors| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "cannot resolve native bridge symbols: {}",
                errors
                    .iter()
                    .map(|error| error.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    })?;
    Ok(NativeBridgeRuntime {
        plan,
        resolved,
        _resolver: resolver,
    })
}

fn absolutize_native_libraries(plan: &mut NativeBridgePlan, project_root: &Path) {
    for module in &mut plan.modules {
        if let Some(library) = module.library.as_deref() {
            module.library = Some(
                resolve_project_path(project_root, library)
                    .display()
                    .to_string(),
            );
        }
    }
}

impl NativeBridgeRuntime {
    fn module(&self, name: &str) -> Option<(&NativeBridgeModule, &ResolvedNativeModule)> {
        self.plan
            .modules
            .iter()
            .zip(self.resolved.iter())
            .find(|(module, _)| module.name == name)
            .map(|(module, resolved)| (module, resolved))
    }
}

fn invoke_native_buffer(
    module: &NativeBridgeModule,
    resolved_module: &ResolvedNativeModule,
    symbol_name: &str,
    args: &[String],
) -> Result<Vec<u8>, String> {
    let Some(symbol) = module
        .symbols
        .iter()
        .find(|candidate| candidate.native_name == symbol_name)
    else {
        return Err(format!("unknown native symbol `{symbol_name}`"));
    };
    if !matches!(symbol.result, NativeAbiType::Buffer) {
        return Err(format!(
            "native symbol `{symbol_name}` does not return a buffer"
        ));
    }
    let Some(resolved_symbol) = resolved_module
        .symbols
        .iter()
        .find(|candidate| candidate.name == symbol_name)
    else {
        return Err(format!("native symbol `{symbol_name}` was not resolved"));
    };
    let values = parse_native_args(symbol, args)?;
    let out = call_native_buffer(resolved_symbol.address, &symbol.params, &values)?;
    copy_native_bytes(module, resolved_module, symbol, out)
}

fn parse_native_args(
    symbol: &NativeSymbol,
    args: &[String],
) -> Result<Vec<NativeScalarValue>, String> {
    if symbol.params.len() != args.len() {
        return Err(format!(
            "native symbol `{}` expected {} args, got {}",
            symbol.native_name,
            symbol.params.len(),
            args.len()
        ));
    }
    symbol
        .params
        .iter()
        .zip(args)
        .map(|(ty, value)| parse_native_arg(ty, value))
        .collect()
}

fn parse_native_arg(ty: &NativeAbiType, raw: &str) -> Result<NativeScalarValue, String> {
    match ty {
        NativeAbiType::Scalar(name) => match name.as_str() {
            "i32" => raw
                .parse::<i32>()
                .map(NativeScalarValue::I32)
                .map_err(|_| format!("native argument `{raw}` is not a valid i32")),
            "i64" => raw
                .parse::<i64>()
                .map(NativeScalarValue::I64)
                .map_err(|_| format!("native argument `{raw}` is not a valid i64")),
            "u64" => raw
                .parse::<u64>()
                .map(NativeScalarValue::U64)
                .map_err(|_| format!("native argument `{raw}` is not a valid u64")),
            "f64" => raw
                .parse::<f64>()
                .map(NativeScalarValue::F64)
                .map_err(|_| format!("native argument `{raw}` is not a valid f64")),
            "bool" => match raw {
                "true" => Ok(NativeScalarValue::Bool(true)),
                "false" => Ok(NativeScalarValue::Bool(false)),
                _ => Err(format!("native argument `{raw}` is not a valid bool")),
            },
            other => Err(format!("native scalar type `{other}` is not supported yet")),
        },
        NativeAbiType::Buffer => Ok(NativeScalarValue::Bytes(raw.as_bytes().to_vec())),
        NativeAbiType::Enum { repr, .. } => {
            parse_native_arg(&NativeAbiType::Scalar(repr.clone()), raw)
        }
        _ => Err(format!(
            "native argument type `{ty:?}` is not supported yet"
        )),
    }
}

fn call_native_buffer(
    address: usize,
    params: &[NativeAbiType],
    values: &[NativeScalarValue],
) -> Result<NativeBytes, String> {
    match (params, values) {
        ([NativeAbiType::Buffer], [NativeScalarValue::Bytes(input)]) => {
            let function: unsafe extern "C" fn(*const u8, usize, *mut NativeBytes) -> c_int =
                unsafe { std::mem::transmute(address) };
            let mut out = NativeBytes {
                ptr: std::ptr::null_mut(),
                len: 0,
            };
            let status = unsafe {
                function(
                    if input.is_empty() {
                        std::ptr::null()
                    } else {
                        input.as_ptr()
                    },
                    input.len(),
                    &mut out,
                )
            };
            if status != 0 {
                Err(format!("native call returned status {status}"))
            } else {
                Ok(out)
            }
        }
        (
            [NativeAbiType::Scalar(a), NativeAbiType::Scalar(b), NativeAbiType::Buffer],
            [NativeScalarValue::I32(width), NativeScalarValue::I32(height), NativeScalarValue::Bytes(input)],
        ) if a == "i32" && b == "i32" => {
            let function: unsafe extern "C" fn(
                i32,
                i32,
                *const u8,
                usize,
                *mut NativeBytes,
            ) -> c_int = unsafe { std::mem::transmute(address) };
            let mut out = NativeBytes {
                ptr: std::ptr::null_mut(),
                len: 0,
            };
            let status = unsafe {
                function(
                    *width,
                    *height,
                    if input.is_empty() {
                        std::ptr::null()
                    } else {
                        input.as_ptr()
                    },
                    input.len(),
                    &mut out,
                )
            };
            if status != 0 {
                Err(format!("native call returned status {status}"))
            } else {
                Ok(out)
            }
        }
        (
            [NativeAbiType::Scalar(a), NativeAbiType::Scalar(b), NativeAbiType::Scalar(c), NativeAbiType::Scalar(d), NativeAbiType::Scalar(e), NativeAbiType::Scalar(f)],
            [NativeScalarValue::I32(a_value), NativeScalarValue::I32(b_value), NativeScalarValue::I32(c_value), NativeScalarValue::F64(d_value), NativeScalarValue::F64(e_value), NativeScalarValue::F64(f_value)],
        ) if a == "i32" && b == "i32" && c == "i32" && d == "f64" && e == "f64" && f == "f64" => {
            let function: unsafe extern "C" fn(
                i32,
                i32,
                i32,
                f64,
                f64,
                f64,
                *mut NativeBytes,
            ) -> c_int = unsafe { std::mem::transmute(address) };
            let mut out = NativeBytes {
                ptr: std::ptr::null_mut(),
                len: 0,
            };
            let status = unsafe {
                function(
                    *a_value, *b_value, *c_value, *d_value, *e_value, *f_value, &mut out,
                )
            };
            if status != 0 {
                Err(format!("native call returned status {status}"))
            } else {
                Ok(out)
            }
        }
        _ => Err(format!(
            "unsupported native buffer signature: params={params:?}, values={values:?}"
        )),
    }
}

fn copy_native_bytes(
    module: &NativeBridgeModule,
    resolved_module: &ResolvedNativeModule,
    symbol: &NativeSymbol,
    out: NativeBytes,
) -> Result<Vec<u8>, String> {
    if out.ptr.is_null() {
        if out.len != 0 {
            return Err(format!(
                "native symbol `{}` in module `{}` returned a null pointer with non-zero length {}",
                symbol.native_name, module.name, out.len
            ));
        }
        return Ok(Vec::new());
    }
    let Some(free_name) = symbol.requires_free.as_deref() else {
        return Err(format!(
            "native symbol `{}` in module `{}` returned owned bytes without a free function",
            symbol.native_name, module.name
        ));
    };
    let Some(free_symbol) = resolved_module
        .symbols
        .iter()
        .find(|candidate| candidate.name == free_name)
    else {
        return Err(format!(
            "native free function `{free_name}` was not resolved"
        ));
    };
    let free: unsafe extern "C" fn(*mut u8) = unsafe { std::mem::transmute(free_symbol.address) };
    if out.len > MAX_NATIVE_BYTES_LEN {
        unsafe {
            free(out.ptr);
        }
        return Err(format!(
            "native symbol `{}` in module `{}` returned {} bytes, exceeding the {} byte limit",
            symbol.native_name, module.name, out.len, MAX_NATIVE_BYTES_LEN
        ));
    }
    let bytes = if out.len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(out.ptr, out.len) }.to_vec()
    };
    unsafe {
        free(out.ptr);
    }
    Ok(bytes)
}

fn server_runtime(options: &BuildOptions) -> io::Result<lume_runtime_server::ServerRuntime> {
    let source = fs::read_to_string(&options.entry)?;
    let (program, diagnostics) = lume_parser::parse(&source);
    if diagnostics.has_errors() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot execute Server Action because parsing failed",
        ));
    }
    let hir = lume_hir::lower(program);
    let ir = lume_ir::build_with_base(&hir, options.entry.parent()).map_err(|diagnostics| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "cannot execute Server Action because IR build failed with {} diagnostics",
                diagnostics.as_slice().len()
            ),
        )
    })?;
    Ok(lume_runtime_server::ServerRuntime::new(ir.server_actions))
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    include_body: bool,
    body: &[u8],
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if include_body {
        stream.write_all(body)?;
    }
    Ok(())
}

fn status_line(status: u16) -> &'static str {
    match status {
        400 => "400 Bad Request",
        401 => "401 Unauthorized",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        429 => "429 Too Many Requests",
        413 => "413 Payload Too Large",
        500 => "500 Internal Server Error",
        _ => "500 Internal Server Error",
    }
}

fn request_header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find_map(|(header, value)| header.eq_ignore_ascii_case(name).then(|| value.clone()))
}

fn action_rate_limited(action_id: &str, headers: &[(String, String)]) -> bool {
    let identity = request_header(headers, "authorization")
        .or_else(|| request_header(headers, "x-forwarded-for"))
        .unwrap_or_else(|| "anonymous".into());
    let key = format!("{action_id}:{identity}");
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let store = RATE_LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut store) = store.lock() else {
        return false;
    };
    let (window_start, count) = store.entry(key).or_insert((now, 0));
    if now.saturating_sub(*window_start) >= 60 {
        *window_start = now;
        *count = 0;
    }
    *count += 1;
    *count > 60
}

static RATE_LIMITS: OnceLock<Mutex<HashMap<String, (u64, u32)>>> = OnceLock::new();

fn dev_csrf_token() -> String {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(generate_dev_csrf_token).clone()
}

fn generate_dev_csrf_token() -> String {
    let mut bytes = [0_u8; 32];
    if fill_random_bytes(&mut bytes).is_err() {
        let fallback = format!(
            "{:?}:{:?}:{:?}",
            SystemTime::now(),
            std::process::id(),
            thread::current().id()
        );
        for (index, byte) in fallback.as_bytes().iter().enumerate() {
            bytes[index % 32] ^= *byte;
        }
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fill_random_bytes(bytes: &mut [u8]) -> io::Result<()> {
    fs::File::open("/dev/urandom")?.read_exact(bytes)
}

fn response_body_with_dev_csrf(path: &Path, body: Vec<u8>) -> Vec<u8> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("html") {
        return body;
    }
    let mut html = match String::from_utf8(body) {
        Ok(html) => html,
        Err(err) => return err.into_bytes(),
    };
    if html.contains("name=\"lume-csrf\"") {
        return html.into_bytes();
    }
    let meta = format!(
        "    <meta name=\"lume-csrf\" content=\"{}\">\n",
        dev_csrf_token()
    );
    if let Some(index) = html.find("    <title>") {
        html.insert_str(index, &meta);
        return html.into_bytes();
    }
    html.replace("<head>", &format!("<head>\n{meta}"))
        .into_bytes()
}

fn resolve_request_path(out_dir: &Path, target: &str) -> Option<PathBuf> {
    let path = target
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let path = percent_decode(path)?;
    let path = if path.is_empty() { "index.html" } else { &path };
    let mut resolved = out_dir.to_path_buf();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') {
            return None;
        }
        resolved.push(segment);
    }
    Some(resolved)
}

fn spa_fallback_path(out_dir: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or(target);
    if path.starts_with("/assets/") || path == "/assets" {
        return None;
    }
    if path.contains('.') {
        return None;
    }
    Some(out_dir.join("index.html"))
}

fn percent_decode(input: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1)?;
            let lo = *bytes.get(i + 2)?;
            out.push(from_hex_pair(hi, lo)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn percent_decode_query(input: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hi = *bytes.get(i + 1)?;
                let lo = *bytes.get(i + 2)?;
                out.push(from_hex_pair(hi, lo)?);
                i += 3;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn from_hex_pair(hi: u8, lo: u8) -> Option<u8> {
    Some(from_hex(hi)? * 16 + from_hex(lo)?)
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mime_type, parse_byte_size, resolve_request_path, response_body_with_dev_csrf,
        spa_fallback_path,
    };
    use std::path::Path;

    #[test]
    fn resolves_root_to_index() {
        let path = resolve_request_path(Path::new("dist"), "/").expect("path");
        assert_eq!(path, Path::new("dist").join("index.html"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(resolve_request_path(Path::new("dist"), "/../secret").is_none());
        assert!(resolve_request_path(Path::new("dist"), "/assets/%2e%2e/secret").is_none());
    }

    #[test]
    fn falls_back_to_index_for_spa_routes() {
        assert_eq!(
            spa_fallback_path(Path::new("dist"), "/users/login"),
            Some(Path::new("dist").join("index.html"))
        );
        assert_eq!(spa_fallback_path(Path::new("dist"), "/assets/app.js"), None);
        assert_eq!(spa_fallback_path(Path::new("dist"), "/robots.txt"), None);
    }

    #[test]
    fn maps_common_mime_types() {
        assert_eq!(
            mime_type(Path::new("dist/index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            mime_type(Path::new("dist/assets/app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_type(Path::new("dist/assets/app.wasm")),
            "application/wasm"
        );
    }

    #[test]
    fn injects_dev_csrf_meta_into_html_responses() {
        let body =
            b"<!doctype html>\n<html>\n  <head>\n    <title>Lume App</title>\n  </head>\n</html>\n"
                .to_vec();
        let html = String::from_utf8(response_body_with_dev_csrf(
            Path::new("dist/index.html"),
            body,
        ))
        .expect("html");
        assert!(html.contains("<meta name=\"lume-csrf\" content=\""));
        assert!(html.contains("    <title>Lume App</title>"));
    }

    #[test]
    fn parses_action_body_size_units() {
        assert_eq!(parse_byte_size("256"), Some(256));
        assert_eq!(parse_byte_size("5 MB"), Some(5 * 1024 * 1024));
        assert_eq!(parse_byte_size("1GiB"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_byte_size("lots"), None);
    }
}

fn write_dist(
    options: &BuildOptions,
    ir: &lume_ir::LumeProgram,
    html: &lume_codegen_html::HtmlOutput,
    css: &str,
    js: &str,
    record_metrics: bool,
    metrics: &mut Vec<BenchMetric>,
) -> io::Result<()> {
    let assets = options.out_dir.join("assets");
    fs::create_dir_all(&assets)?;
    let native_started = Instant::now();
    build_ffi_sources(options, ir)?;
    push_bench_metric(metrics, record_metrics, "native link", native_started);

    let wasm_bytes = if options.target.wasm_enabled() {
        let wasm_started = Instant::now();
        let wasm_events = html
            .events
            .iter()
            .map(|event| lume_codegen_wasm::WasmEvent {
                id: event.id,
                statements: event.statements.clone(),
            })
            .collect::<Vec<_>>();
        let wasm = match lume_codegen_wasm::WasmBackend::new().emit_with_events(ir, &wasm_events) {
            Ok(artifact) => artifact.wasm,
            Err(err) => {
                eprintln!(
                    "lume build: LLVM WASM generation failed, using fallback skeleton: {err}"
                );
                lume_codegen_wasm::WasmBackend::new().emit_skeleton()
            }
        };
        push_bench_metric(metrics, record_metrics, "WASM generation", wasm_started);
        Some(wasm)
    } else {
        None
    };

    let write_started = Instant::now();
    fs::write(options.out_dir.join("index.html"), &html.html)?;
    fs::write(assets.join("style.css"), css)?;
    fs::write(assets.join("app.js"), js)?;
    if let Some(wasm) = wasm_bytes {
        fs::write(assets.join("app.wasm"), wasm)?;
    } else {
        let stale_wasm = assets.join("app.wasm");
        if stale_wasm.exists() {
            fs::remove_file(stale_wasm)?;
        }
    }
    fs::write(
        assets.join("lume.manifest.json"),
        manifest(ir, html, options),
    )?;
    fs::write(
        assets.join("lume.backend.json"),
        backend_manifest(ir, options),
    )?;
    push_bench_metric(metrics, record_metrics, "dist write", write_started);
    Ok(())
}

fn build_ffi_sources(options: &BuildOptions, ir: &lume_ir::LumeProgram) -> io::Result<()> {
    let project_root = project_root_for_entry(&options.entry);
    for module in &ir.ffi_modules {
        let target = module
            .targets
            .iter()
            .find(|target| target.name == lume_ffi::current_platform());
        let sources = target
            .filter(|target| !target.sources.is_empty())
            .map(|target| target.sources.as_slice())
            .unwrap_or_else(|| module.sources.as_slice());
        if sources.is_empty() {
            continue;
        }
        let library = target
            .and_then(|target| target.library.as_ref())
            .or(module.library.as_ref());
        let Some(library) = library else {
            continue;
        };
        let output = resolve_project_path(&project_root, library);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut command = Command::new(c_compiler_for(module.language.as_deref()));
        if !matches!(module.language.as_deref(), Some("cpp" | "c++")) {
            command.arg("-std=c11");
        }
        command.arg("-shared").arg("-fPIC");
        for source in sources {
            command.arg(resolve_project_path(&project_root, source));
        }
        let header = target
            .and_then(|target| target.header.as_ref())
            .or(module.header.as_ref());
        if let Some(header) = header {
            if let Some(include_dir) = resolve_project_path(&project_root, header).parent() {
                command.arg("-I").arg(include_dir);
            }
        }
        command.arg("-o").arg(&output);
        let status = command.status().map_err(|err| {
            io::Error::new(
                err.kind(),
                format!(
                    "failed to run native FFI compiler for `{}`: {err}",
                    module.name
                ),
            )
        })?;
        if !status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("native FFI compiler failed for `{}`", module.name),
            ));
        }
    }
    Ok(())
}

fn project_root_for_entry(entry: &Path) -> PathBuf {
    let Some(parent) = entry.parent() else {
        return PathBuf::from(".");
    };
    if parent.file_name().is_some_and(|name| name == "src") {
        parent.parent().unwrap_or(parent).to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

fn resolve_project_path(project_root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn c_compiler_for(language: Option<&str>) -> &'static str {
    match language {
        Some("cpp" | "c++") => "c++",
        _ => "cc",
    }
}

fn manifest(
    ir: &lume_ir::LumeProgram,
    html: &lume_codegen_html::HtmlOutput,
    options: &BuildOptions,
) -> String {
    let states = ir
        .states()
        .map(|state| {
            format!(
                "      {{ \"name\": \"{}\", \"type\": \"{}\", \"initial\": {} }}",
                state.name, state.ty, state.init.raw
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let routes = ir
        .routes
        .iter()
        .map(|route| format!("      {}", route_json(route, 6)))
        .collect::<Vec<_>>()
        .join(",\n");
    let route_tree = ir
        .route_tree
        .iter()
        .map(|route| format!("      {}", route_node_json(route, 6)))
        .collect::<Vec<_>>()
        .join(",\n");
    let styles = ir
        .styles
        .iter()
        .map(|style| format!("\"{}\"", style.name))
        .collect::<Vec<_>>()
        .join(", ");
    let themes = ir
        .themes
        .iter()
        .map(|theme| format!("\"{}\"", theme.name))
        .collect::<Vec<_>>()
        .join(", ");
    let actions = ir
        .server_actions
        .iter()
        .map(|action| {
            let modifier_value = |name: &str| {
                action
                    .modifiers
                    .iter()
                    .find(|modifier| modifier.name == name)
                    .and_then(|modifier| modifier.value.as_deref())
            };
            let modifier_presence_value = |name: &str, default: &str| {
                action
                    .modifiers
                    .iter()
                    .find(|modifier| modifier.name == name)
                    .map(|modifier| modifier.value.clone().unwrap_or_else(|| default.into()))
            };
            let runtime = modifier_value("runtime")
                .map(unquote_lume_value)
                .unwrap_or_else(|| "server".into());
            let auth = modifier_value("auth").map(unquote_lume_value);
            let csrf = modifier_value("csrf")
                .map(|value| unquote_lume_value(value) != "false")
                .unwrap_or(true);
            let invalidates = modifier_value("invalidates")
                .map(unquote_lume_value)
                .unwrap_or_default();
            let max_body_size = modifier_value("maxBodySize")
                .map(unquote_lume_value)
                .unwrap_or_default();
            let rate_limit = modifier_value("rateLimit")
                .map(unquote_lume_value)
                .unwrap_or_default();
            let transaction = modifier_presence_value("transaction", "true")
                .map(|value| unquote_lume_value(&value))
                .unwrap_or_default();
            let validation = modifier_value("validate")
                .map(unquote_lume_value)
                .unwrap_or_default();
            let input = action
                .params
                .iter()
                .map(|param| {
                    format!(
                        "{{ \"name\": \"{}\", \"type\": \"{}\", \"modifier\": {} }}",
                        json_escape(&param.name),
                        json_escape(&param.ty),
                        param
                            .modifier
                            .as_ref()
                            .map(|value| format!("\"{}\"", json_escape(value)))
                            .unwrap_or_else(|| "null".into())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "      {{ \"id\": \"{}\", \"runtime\": \"{}\", \"auth\": {}, \"csrf\": {}, \"input\": [{}], \"output\": \"{}\", \"validation\": {}, \"invalidates\": {}, \"maxBodySize\": {}, \"rateLimit\": {}, \"transaction\": {} }}",
                json_escape(&action.name),
                json_escape(&runtime),
                auth
                    .map(|value| format!("\"{}\"", json_escape(&value)))
                    .unwrap_or_else(|| "null".into()),
                csrf,
                input,
                json_escape(&action.return_ty),
                optional_json_string(&validation),
                optional_json_string(&invalidates),
                optional_json_number(&max_body_size),
                optional_json_string(&rate_limit),
                optional_json_string(&transaction)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let queries = ir
        .queries
        .iter()
        .map(|query| {
            format!(
                "      {{ \"name\": \"{}\", \"server\": {}, \"mutation\": {}, \"key\": {}, \"source\": \"{}\" }}",
                json_escape(&query.name),
                query.is_server,
                query.is_mutation,
                query
                    .key
                    .as_ref()
                    .map(|expr| format!("\"{}\"", json_escape(&expr.raw)))
                    .unwrap_or_else(|| "null".into()),
                json_escape(&query.source.raw)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let state_names = ir
        .states()
        .map(|state| state.name.clone())
        .collect::<Vec<_>>();
    let symbols = html
        .events
        .iter()
        .map(|event| {
            let captures = resumable_event_captures(event, &state_names)
                .into_iter()
                .map(|capture| format!("\"s0.{}\"", json_escape(&capture)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "    \"sym_event_{}\": {{ \"captures\": [{}], \"chunk\": null, \"boundary\": \"b0\" }}",
                event.id,
                captures
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let event_bindings = html
        .events
        .iter()
        .map(|event| {
            format!(
                "      {{ \"node\": \"{}\", \"event\": \"{}\", \"symbol\": \"sym_event_{}\", \"state\": \"s0\" }}",
                json_escape(&event.node_id),
                json_escape(&event.event),
                event.id
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let resume_graph_boundaries = if options.activation.is_resume() {
        format!(
            "      {{ \"id\": \"b0\", \"stateScopes\": [\"s0\"], \"symbols\": [{}], \"fallback\": \"HydrateBoundary\" }}",
            html.events
                .iter()
                .map(|event| format!("\"sym_event_{}\"", event.id))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        String::new()
    };
    let symbols = if options.activation.is_resume() {
        symbols
    } else {
        String::new()
    };
    let event_bindings = if options.activation.is_resume() {
        event_bindings
    } else {
        String::new()
    };
    let serialized_state = if options.activation.is_resume() {
        format!(
            "    \"s0\": {{ \"value\": {}, \"hash\": null }}",
            html.serialized_state_json
        )
    } else {
        String::new()
    };
    let ffi = ir
        .ffi_modules
        .iter()
        .map(|module| {
            let functions = module
                .functions
                .iter()
                .map(|function| {
                    let params = function
                        .params
                        .iter()
                        .map(|param| {
                            format!(
                                "{{ \"name\": \"{}\", \"type\": \"{}\", \"modifier\": {} }}",
                                json_escape(&param.name),
                                json_escape(&param.ty),
                                param
                                    .modifier
                                    .as_ref()
                                    .map(|value| format!("\"{}\"", json_escape(value)))
                                    .unwrap_or_else(|| "null".into())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{{ \"name\": \"{}\", \"params\": [{}], \"return\": \"{}\", \"callback\": {}, \"ownership\": {}, \"free\": {}, \"throws\": {} }}",
                        json_escape(&function.name),
                        params,
                        json_escape(&function.return_ty),
                        function.callback,
                        function
                            .ownership
                            .as_ref()
                            .map(|value| format!("\"{}\"", json_escape(value)))
                            .unwrap_or_else(|| "null".into()),
                        function
                            .free
                            .as_ref()
                            .map(|value| format!("\"{}\"", json_escape(value)))
                            .unwrap_or_else(|| "null".into()),
                        function
                            .throws
                            .as_ref()
                            .map(|value| format!("\"{}\"", json_escape(value)))
                            .unwrap_or_else(|| "null".into())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "      {{ \"name\": \"{}\", \"language\": {}, \"library\": {}, \"header\": {}, \"namespace\": {}, \"abi\": {}, \"targets\": [{}], \"sources\": [{}], \"runtime\": [{}], \"safety\": {}, \"threadSafe\": {}, \"lock\": {}, \"functions\": [{}] }}",
                json_escape(&module.name),
                module
                    .language
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                module
                    .library
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                module
                    .header
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                module
                    .namespace
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                module
                    .abi
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                module
                    .targets
                    .iter()
                    .map(|target| {
                        format!(
                            "{{ \"name\": \"{}\", \"library\": {}, \"header\": {} }}",
                            json_escape(&target.name),
                            target
                                .library
                                .as_ref()
                                .map(|value| format!("\"{}\"", json_escape(value)))
                                .unwrap_or_else(|| "null".into()),
                            target
                                .header
                                .as_ref()
                                .map(|value| format!("\"{}\"", json_escape(value)))
                                .unwrap_or_else(|| "null".into())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                module
                    .sources
                    .iter()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .collect::<Vec<_>>()
                    .join(", "),
                if module.runtime.is_empty() {
                    "\"native\"".into()
                } else {
                    module
                        .runtime
                        .iter()
                        .map(|value| format!("\"{}\"", json_escape(value)))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                module
                    .safety
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "\"safe\"".into()),
                module
                    .thread_safe
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".into()),
                module
                    .lock
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                functions
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let ffi_structs = ir
        .ffi_structs
        .iter()
        .map(|item| {
            let fields = item
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "{{ \"name\": \"{}\", \"type\": \"{}\" }}",
                        json_escape(&field.name),
                        json_escape(&field.ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "      {{ \"name\": \"{}\", \"repr\": {}, \"fields\": [{}] }}",
                json_escape(&item.name),
                item.repr
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "\"C\"".into()),
                fields
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let ffi_enums = ir
        .ffi_enums
        .iter()
        .map(|item| {
            format!(
                "      {{ \"name\": \"{}\", \"repr\": {}, \"variants\": [{}] }}",
                json_escape(&item.name),
                item.repr
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "\"i32\"".into()),
                item.variants
                    .iter()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let ffi_opaques = ir
        .ffi_opaques
        .iter()
        .map(|item| {
            format!(
                "      {{ \"name\": \"{}\", \"ownership\": {}, \"lifetime\": {} }}",
                json_escape(&item.name),
                item.ownership
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into()),
                item.lifetime
                    .as_ref()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .unwrap_or_else(|| "null".into())
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"version\": \"0.1.0\",\n  \"component\": \"{}\",\n  \"target\": \"{}\",\n  \"activation\": \"{}\",\n  \"backends\": {},\n  \"state\": [\n{}\n  ],\n  \"resumeGraph\": {{\n    \"boundaries\": [\n{}\n    ]\n  }},\n  \"symbols\": {{\n{}\n  }},\n  \"eventBindings\": [\n{}\n  ],\n  \"serializedState\": {{\n{}\n  }},\n  \"routes\": [\n{}\n  ],\n  \"routeTree\": [\n{}\n  ],\n  \"styles\": [{}],\n  \"themes\": [{}],\n  \"actions\": [\n{}\n  ],\n  \"queries\": [\n{}\n  ],\n  \"ffi\": [\n{}\n  ],\n  \"ffiStructs\": [\n{}\n  ],\n  \"ffiEnums\": [\n{}\n  ],\n  \"ffiOpaques\": [\n{}\n  ]\n}}\n",
        json_escape(&ir.component.name),
        options.target.as_str(),
        options.activation.as_str(),
        backend_list(options),
        states,
        resume_graph_boundaries,
        symbols,
        event_bindings,
        serialized_state,
        routes,
        route_tree,
        styles,
        themes,
        actions,
        queries,
        ffi,
        ffi_structs,
        ffi_enums,
        ffi_opaques
    )
}

fn resumable_event_captures(
    event: &lume_codegen_html::EventBinding,
    state_names: &[String],
) -> Vec<String> {
    let mut captures = Vec::new();
    for stmt in &event.statements {
        match stmt {
            lume_ast::Stmt::Assign { target, expr, .. } => {
                if state_names.iter().any(|state| state == target) {
                    push_unique(&mut captures, target.clone());
                }
                for ident in identifiers_in_expr(&expr.raw) {
                    if state_names.iter().any(|state| state == &ident) {
                        push_unique(&mut captures, ident);
                    }
                }
            }
            lume_ast::Stmt::Expr(expr) => {
                for ident in identifiers_in_expr(&expr.raw) {
                    if state_names.iter().any(|state| state == &ident) {
                        push_unique(&mut captures, ident);
                    }
                }
            }
        }
    }
    captures
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn identifiers_in_expr(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '\"' || ch == '\'' {
            while let Some((_, inner)) = chars.next() {
                if inner == '\\' {
                    let _ = chars.next();
                    continue;
                }
                if inner == ch {
                    break;
                }
            }
            continue;
        }
        if ch == '_' || ch.is_ascii_alphabetic() {
            let start = idx;
            let mut end = idx + ch.len_utf8();
            while let Some((next_idx, next)) = chars.peek().copied() {
                if next == '_' || next.is_ascii_alphanumeric() {
                    let _ = chars.next();
                    end = next_idx + next.len_utf8();
                } else {
                    break;
                }
            }
            let ident = raw[start..end].to_string();
            if !matches!(
                ident.as_str(),
                "true" | "false" | "null" | "undefined" | "Math" | "String" | "Number"
            ) {
                push_unique(&mut out, ident);
            }
        }
    }
    out
}

fn backend_manifest(ir: &lume_ir::LumeProgram, options: &BuildOptions) -> String {
    let ffi_registry = lume_ffi::FfiRegistry::from_ast(
        &ir.ffi_modules,
        &ir.ffi_structs,
        &ir.ffi_enums,
        &ir.ffi_opaques,
    );
    let native_bridge = lume_backend_native::NativeBackend
        .plan_ffi(&ffi_registry)
        .map(|plan| plan.to_json())
        .unwrap_or_else(|errors| {
            let errors = errors
                .iter()
                .map(|error| {
                    format!(
                        "{{ \"module\": \"{}\", \"symbol\": {}, \"message\": \"{}\" }}",
                        json_escape(&error.module),
                        error
                            .symbol
                            .as_ref()
                            .map(|symbol| format!("\"{}\"", json_escape(symbol)))
                            .unwrap_or_else(|| "null".into()),
                        json_escape(&error.message)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ \"modules\": [], \"errors\": [{}] }}", errors)
        });
    format!(
        "{{\n  \"ssr\": {{ \"entry\": \"index.html\", \"routes\": {} }},\n  \"wasm\": {{ \"enabled\": {}, \"entry\": {}, \"target\": {}, \"abi\": {} }},\n  \"native\": {{ \"enabled\": true, \"actions\": {}, \"ffiModules\": {}, \"bridge\": {} }},\n  \"jit\": {{ \"enabled\": true, \"actions\": {} }}\n}}\n",
        ir.routes.len(),
        options.target.wasm_enabled(),
        if options.target.wasm_enabled() {
            "\"assets/app.wasm\""
        } else {
            "null"
        },
        if options.target.wasm_enabled() {
            "\"wasm32-unknown-unknown\""
        } else {
            "null"
        },
        if options.target.wasm_enabled() {
            "[\"lume_init\", \"lume_dispatch\", \"lume_get_patch_len\", \"lume_alloc\", \"lume_free\", \"lume_set_state\", \"lume_get_state\"]"
        } else {
            "[]"
        },
        ir.server_actions.len(),
        ir.ffi_modules.len(),
        native_bridge,
        ir.server_actions
            .iter()
            .filter(|action| {
                action.modifiers.iter().any(|modifier| {
                    modifier.name == "runtime"
                        && modifier
                            .value
                            .as_deref()
                            .map(unquote_lume_value)
                            .as_deref()
                            == Some("jit")
                })
            })
            .count()
    )
}

fn backend_list(options: &BuildOptions) -> &'static str {
    if options.target.wasm_enabled() {
        "[\"ssr\", \"wasm\", \"native\", \"jit\"]"
    } else {
        "[\"ssr\", \"native\", \"jit\"]"
    }
}

fn route_node_json(node: &lume_ir::IrRouteNode, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let child_pad = " ".repeat(indent + 2);
    let children = node
        .children
        .iter()
        .map(|child| format!("{child_pad}{}", route_node_json(child, indent + 2)))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{ \"route\": {}, \"children\": [\n{}\n{pad}] }}",
        route_json(&node.route, indent),
        children
    )
}

fn route_json(route: &lume_ir::IrRoute, _indent: usize) -> String {
    let params = route
        .params
        .iter()
        .map(|param| {
            format!(
                "\"{}\": {{ \"type\": \"{}\", \"catchAll\": {} }}",
                json_escape(&param.name),
                json_escape(&param.ty),
                param.catch_all
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let guards = route
        .guards
        .iter()
        .map(|guard| format!("\"{}\"", json_escape(guard)))
        .collect::<Vec<_>>()
        .join(", ");
    let matcher = route
        .segments
        .iter()
        .map(route_segment_json)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{ \"id\": \"{}\", \"path\": \"{}\", \"sourcePath\": \"{}\", \"page\": {}, \"layout\": {}, \"guards\": [{}], \"index\": {}, \"params\": {{ {} }}, \"matcher\": [{}] }}",
        json_escape(&route.id),
        json_escape(&route.path),
        json_escape(&route.source_path),
        route
            .page
            .as_ref()
            .map(|page| format!("\"{}\"", json_escape(page)))
            .unwrap_or_else(|| "null".into()),
        route
            .layout
            .as_ref()
            .map(|layout| format!("\"{}\"", json_escape(layout)))
            .unwrap_or_else(|| "null".into()),
        guards,
        route.is_index,
        params,
        matcher
    )
}

fn route_segment_json(segment: &lume_ir::RouteSegment) -> String {
    match segment {
        lume_ir::RouteSegment::Static(value) => {
            format!(
                "{{ \"kind\": \"static\", \"value\": \"{}\" }}",
                json_escape(value)
            )
        }
        lume_ir::RouteSegment::Dynamic { name, ty } => format!(
            "{{ \"kind\": \"dynamic\", \"name\": \"{}\", \"type\": \"{}\" }}",
            json_escape(name),
            json_escape(ty.as_deref().unwrap_or("String"))
        ),
        lume_ir::RouteSegment::CatchAll { name } => format!(
            "{{ \"kind\": \"catchAll\", \"name\": \"{}\" }}",
            json_escape(name)
        ),
    }
}

fn unquote_lume_value(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).to_string()
}

fn optional_json_string(value: &str) -> String {
    if value.is_empty() {
        "null".into()
    } else {
        format!("\"{}\"", json_escape(value))
    }
}

fn optional_json_number(value: &str) -> String {
    if value.is_empty() {
        "null".into()
    } else {
        parse_byte_size(value)
            .map(|size| size.to_string())
            .unwrap_or_else(|| value.to_string())
    }
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn parse_byte_size(value: &str) -> Option<usize> {
    let compact = value
        .trim()
        .trim_matches(['"', '\''])
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '_')
        .collect::<String>();
    if compact.is_empty() {
        return None;
    }
    let split = compact
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(compact.len());
    let number = compact[..split].parse::<usize>().ok()?;
    let unit = compact[split..].to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1,
        "KB" | "KIB" => 1024,
        "MB" | "MIB" => 1024 * 1024,
        "GB" | "GIB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
}
