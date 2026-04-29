use lume_diagnostics::{emit, Diagnostic};
use lume_session::{BuildOptions, Session};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
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

    serve(options.out_dir, port)
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

fn serve(out_dir: PathBuf, port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let local_addr = listener.local_addr()?;
    eprintln!("lume dev: serving http://{local_addr}");
    eprintln!("lume dev: serving files from {}", out_dir.display());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let out_dir = out_dir.clone();
                thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, &out_dir) {
                        eprintln!("lume dev: request failed: {err}");
                    }
                });
            }
            Err(err) => eprintln!("lume dev: connection failed: {err}"),
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, out_dir: &Path) -> io::Result<()> {
    let mut buffer = [0; 4096];
    let bytes = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let mut parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            method != "HEAD",
            b"Method Not Allowed",
        );
    }

    let Some(path) = resolve_request_path(out_dir, target) else {
        return write_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            method != "HEAD",
            b"Bad Request",
        );
    };

    match fs::read(&path) {
        Ok(body) => write_response(
            &mut stream,
            "200 OK",
            mime_type(&path),
            method != "HEAD",
            &body,
        ),
        Err(_) => write_response(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            method != "HEAD",
            b"Not Found",
        ),
    }
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
    use super::{mime_type, resolve_request_path};
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
    fs::write(assets.join("lume.manifest.json"), manifest(ir))?;
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
        .map(|route| format!("      {{ \"path\": \"{}\" }}", route.path))
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
    format!(
        "{{\n  \"version\": \"0.1.0\",\n  \"component\": \"{}\",\n  \"target\": \"html-js-css\",\n  \"state\": [\n{}\n  ],\n  \"routes\": [\n{}\n  ],\n  \"styles\": [{}],\n  \"themes\": [{}]\n}}\n",
        ir.component.name, states, routes, styles, themes
    )
}
