use lume_diagnostics::{emit, Diagnostic};
use lume_session::{BuildOptions, Session};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug)]
pub struct BuildResult {
    pub diagnostics: Vec<Diagnostic>,
    pub emitted: Vec<String>,
}

pub fn build(options: BuildOptions) -> io::Result<BuildResult> {
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
            match lume_ir::build_with_base(&hir, options.entry.parent()) {
                Ok(ir) => {
                    let html = lume_codegen_html::generate(&ir);
                    let css = lume_codegen_css::generate(&ir);
                    let js = lume_codegen_js::generate(&ir, &html);
                    write_dist(&options, &ir, &html.html, &css, &js)?;
                    return Ok(BuildResult {
                        diagnostics: diagnostics.into_vec(),
                        emitted: vec![
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
                                .join("assets/app.wasm")
                                .display()
                                .to_string(),
                            options
                                .out_dir
                                .join("assets/lume.backend.json")
                                .display()
                                .to_string(),
                        ],
                    });
                }
                Err(more) => diagnostics.extend(more.into_vec()),
            }
        }
    }
    eprint!("{}", emit(diagnostics.as_slice(), Some(&file)));
    Ok(BuildResult {
        diagnostics: diagnostics.into_vec(),
        emitted: Vec::new(),
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
            "[project]\nname = \"lume-app\"\nentry = \"src/app.lume\"\nout_dir = \"dist\"\n",
        )?;
    }
    if !Path::new("src/app.lume").exists() {
        fs::write("src/app.lume", "component App {\n  state count: i32 = 0\n\n  view {\n    Column gap=12 padding=16 {\n      Text(\"Count: {count}\")\n\n      Button(\"増やす\") {\n        on click {\n          count += 1\n        }\n      }\n    }\n  }\n}\n")?;
    }
    Ok(())
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
        return write_response(
            &mut stream,
            "200 OK",
            mime_type(&path),
            method != "HEAD",
            &body,
        );
    }

    if let Some(fallback) = spa_fallback_path(&options.out_dir, target) {
        if let Ok(body) = fs::read(&fallback) {
            return write_response(
                &mut stream,
                "200 OK",
                mime_type(&fallback),
                method != "HEAD",
                &body,
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

fn handle_action_request(
    stream: &mut TcpStream,
    options: &BuildOptions,
    action_id: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> io::Result<()> {
    let runtime = server_runtime(options)?;
    let context = lume_runtime_server::ActionRequestContext {
        csrf_token: request_header(headers, "x-lume-csrf").or_else(|| Some("dev-csrf-token".into())),
        authenticated: request_header(headers, "authorization").is_some()
            || request_header(headers, "cookie")
                .is_some_and(|cookie| cookie.contains("lume_session=")),
        rate_limit_exceeded: action_rate_limited(action_id, headers),
    };
    let body = std::str::from_utf8(body).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("action body is not utf-8: {err}"),
        )
    })?;
    match runtime.call_json_with_context(action_id, body, &context) {
        Ok(response) => write_response(
            stream,
            "200 OK",
            "application/json; charset=utf-8",
            true,
            response.as_bytes(),
        ),
        Err(err) => {
            let body = format!("{{\"error\":\"{}\"}}", json_escape(&err.message));
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
    use super::{mime_type, resolve_request_path, spa_fallback_path};
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
}

fn write_dist(
    options: &BuildOptions,
    ir: &lume_ir::LumeProgram,
    html: &str,
    css: &str,
    js: &str,
) -> io::Result<()> {
    let assets = options.out_dir.join("assets");
    fs::create_dir_all(&assets)?;
    fs::write(options.out_dir.join("index.html"), html)?;
    fs::write(assets.join("style.css"), css)?;
    fs::write(assets.join("app.js"), js)?;
    fs::write(
        assets.join("app.wasm"),
        lume_codegen_wasm::WasmBackend::new().emit_skeleton(),
    )?;
    fs::write(assets.join("lume.manifest.json"), manifest(ir))?;
    fs::write(assets.join("lume.backend.json"), backend_manifest(ir))?;
    Ok(())
}

fn manifest(ir: &lume_ir::LumeProgram) -> String {
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
            let transaction = modifier_value("transaction")
                .map(unquote_lume_value)
                .unwrap_or_default();
            let input = action
                .params
                .iter()
                .map(|param| {
                    format!(
                        "{{ \"name\": \"{}\", \"type\": \"{}\" }}",
                        json_escape(&param.name),
                        json_escape(&param.ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "      {{ \"id\": \"{}\", \"runtime\": \"{}\", \"auth\": {}, \"csrf\": {}, \"input\": [{}], \"output\": \"{}\", \"invalidates\": {}, \"maxBodySize\": {}, \"rateLimit\": {}, \"transaction\": {} }}",
                json_escape(&action.name),
                json_escape(&runtime),
                auth
                    .map(|value| format!("\"{}\"", json_escape(&value)))
                    .unwrap_or_else(|| "null".into()),
                csrf,
                input,
                json_escape(&action.return_ty),
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
                "      {{ \"name\": \"{}\", \"server\": {}, \"key\": {}, \"source\": \"{}\" }}",
                json_escape(&query.name),
                query.is_server,
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
    let ffi = ir
        .ffi_modules
        .iter()
        .map(|module| {
            let functions = module
                .functions
                .iter()
                .map(|function| {
                    format!(
                        "{{ \"name\": \"{}\", \"return\": \"{}\", \"callback\": {}, \"ownership\": {} }}",
                        json_escape(&function.name),
                        json_escape(&function.return_ty),
                        function.callback,
                        function
                            .ownership
                            .as_ref()
                            .map(|value| format!("\"{}\"", json_escape(value)))
                            .unwrap_or_else(|| "null".into())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "      {{ \"name\": \"{}\", \"language\": {}, \"library\": {}, \"header\": {}, \"sources\": [{}], \"functions\": [{}] }}",
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
                    .sources
                    .iter()
                    .map(|value| format!("\"{}\"", json_escape(value)))
                    .collect::<Vec<_>>()
                    .join(", "),
                functions
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"version\": \"0.1.0\",\n  \"component\": \"{}\",\n  \"target\": \"html-js-css\",\n  \"backends\": [\"ssr\", \"wasm\", \"native\", \"jit\"],\n  \"state\": [\n{}\n  ],\n  \"routes\": [\n{}\n  ],\n  \"routeTree\": [\n{}\n  ],\n  \"styles\": [{}],\n  \"themes\": [{}],\n  \"actions\": [\n{}\n  ],\n  \"queries\": [\n{}\n  ],\n  \"ffi\": [\n{}\n  ]\n}}\n",
        json_escape(&ir.component.name),
        states,
        routes,
        route_tree,
        styles,
        themes,
        actions,
        queries,
        ffi
    )
}

fn backend_manifest(ir: &lume_ir::LumeProgram) -> String {
    format!(
        "{{\n  \"ssr\": {{ \"entry\": \"index.html\", \"routes\": {} }},\n  \"wasm\": {{ \"enabled\": true, \"entry\": \"assets/app.wasm\", \"abi\": [\"lume_init\", \"lume_dispatch\", \"lume_free\"] }},\n  \"native\": {{ \"enabled\": true, \"actions\": {}, \"ffiModules\": {} }},\n  \"jit\": {{ \"enabled\": true, \"actions\": {} }}\n}}\n",
        ir.routes.len(),
        ir.server_actions.len(),
        ir.ffi_modules.len(),
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
        value.to_string()
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
