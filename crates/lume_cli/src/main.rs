use lume_session::{BuildOptions, BuildTarget};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("lume: {err}");
            eprintln!("Try `lume help` for usage.");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "build".into());
    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "build" => {
            if wants_help(&rest) {
                print_build_help();
                return Ok(ExitCode::SUCCESS);
            }
            let options = options_from(&rest)?;
            let result = lume_driver::build(options)?;
            if has_errors(&result.diagnostics) {
                Ok(ExitCode::from(1))
            } else {
                print_emitted(&result.emitted);
                Ok(ExitCode::SUCCESS)
            }
        }
        "check" => {
            if wants_help(&rest) {
                print_check_help();
                return Ok(ExitCode::SUCCESS);
            }
            let options = options_from(&rest)?;
            let result = lume_driver::check(options)?;
            if has_errors(&result.diagnostics) {
                Ok(ExitCode::from(1))
            } else {
                println!("lume check: ok");
                Ok(ExitCode::SUCCESS)
            }
        }
        "dev" => {
            if wants_help(&rest) {
                print_dev_help();
                return Ok(ExitCode::SUCCESS);
            }
            let options = options_from(&rest)?;
            let port = port_from(&rest)?;
            lume_driver::dev(options, port)?;
            Ok(ExitCode::SUCCESS)
        }
        "bench" => {
            if wants_help(&rest) {
                print_bench_help();
                return Ok(ExitCode::SUCCESS);
            }
            let options = options_from(&rest)?;
            let result = lume_driver::bench(options)?;
            if has_errors(&result.diagnostics) {
                Ok(ExitCode::from(1))
            } else {
                print_bench(&result);
                Ok(ExitCode::SUCCESS)
            }
        }
        "fmt" | "format" => {
            let check_only = rest.iter().any(|arg| arg == "--check");
            let path = rest
                .iter()
                .find(|arg| !arg.starts_with('-'))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("src/app.lume"));
            let ok = lume_driver::fmt(Path::new(&path), check_only)?;
            if ok {
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("{} is not formatted", path.display());
                Ok(ExitCode::from(1))
            }
        }
        "init" => {
            lume_driver::init()?;
            println!("lume init: created lume.toml and src/app.lume when missing");
            Ok(ExitCode::SUCCESS)
        }
        "-V" | "--version" | "version" => {
            println!("lume {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "-h" | "--help" | "help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("unknown command `{other}`");
            print_help();
            Ok(ExitCode::from(2))
        }
    }
}

fn options_from(args: &[String]) -> Result<BuildOptions, Box<dyn std::error::Error>> {
    let mut entry = None;
    let mut out_dir = None;
    let mut target = None;
    let mut config = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--entry" | "-e" => {
                i += 1;
                entry = Some(value_after(args, i, "--entry")?);
            }
            "--out-dir" | "-o" => {
                i += 1;
                out_dir = Some(value_after(args, i, "--out-dir")?);
            }
            "--target" | "-t" => {
                i += 1;
                let value = value_after_string(args, i, "--target")?;
                target = Some(value.parse::<BuildTarget>()?);
            }
            "--config" | "-c" => {
                i += 1;
                config = Some(value_after(args, i, "--config")?);
            }
            "--wasm" => {
                target = Some(BuildTarget::HtmlJsCssWasm);
            }
            "--no-wasm" => {
                target = Some(BuildTarget::HtmlJsCss);
            }
            "--port" | "-p" => {
                i += 1;
            }
            "-h" | "--help" => {}
            path if !path.starts_with('-') && entry.is_none() => entry = Some(PathBuf::from(path)),
            option if option.starts_with('-') => {
                return Err(format!("unknown option `{option}`").into())
            }
            extra => return Err(format!("unexpected argument `{extra}`").into()),
        }
        i += 1;
    }
    Ok(BuildOptions::from_args(entry, out_dir, target, config)?)
}

fn port_from(args: &[String]) -> Result<u16, Box<dyn std::error::Error>> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("missing value for --port".into());
                };
                return Ok(value.parse()?);
            }
            _ => i += 1,
        }
    }
    Ok(3000)
}

fn print_help() {
    println!(
        concat!(
            "lume {}\n\n",
            "USAGE:\n",
            "  lume build [options] [entry]\n",
            "  lume dev [options] [entry]\n",
            "  lume check [options] [entry]\n",
            "  lume bench [options] [entry]\n",
            "  lume fmt [--check] [path]\n",
            "  lume init\n\n",
            "COMMON OPTIONS:\n",
            "  -e, --entry <path>       Entry file, default src/app.lume\n",
            "  -o, --out-dir <path>     Output directory, default dist\n",
            "  -c, --config <path>      Config file, default lume.toml\n",
            "  -t, --target <target>    html-js-css or html-js-css-wasm\n",
            "      --wasm               Shortcut for --target html-js-css-wasm\n",
            "      --no-wasm            Shortcut for --target html-js-css\n",
            "  -h, --help               Show help\n",
            "  -V, --version            Show version\n\n",
            "EXAMPLES:\n",
            "  lume build\n",
            "  lume build --target html-js-css-wasm\n",
            "  lume dev --port 3000\n",
            "  lume check examples/counter/src/app.lume\n",
            "  lume bench examples/mega-workbench/src/app.lume"
        ),
        env!("CARGO_PKG_VERSION")
    );
}

fn print_build_help() {
    println!("Build a Lume app.\n\nUSAGE:\n  lume build [options] [entry]\n\nRun `lume help` to see all common options.");
}

fn print_check_help() {
    println!("Check a Lume app without writing dist files.\n\nUSAGE:\n  lume check [options] [entry]\n\nRun `lume help` to see all common options.");
}

fn print_dev_help() {
    println!("Build, watch, and serve a Lume app.\n\nUSAGE:\n  lume dev [options] [entry]\n\nOPTIONS:\n  -p, --port <port>        Dev server port, default 3000\n\nRun `lume help` to see all common options.");
}

fn print_bench_help() {
    println!(
        "Measure a Lume build and report elapsed time.\n\nUSAGE:\n  lume bench [options] [entry]\n\nRun `lume help` to see all common options."
    );
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "-h" || arg == "--help")
}

fn has_errors(diagnostics: &[lume_diagnostics::Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
}

fn print_emitted(paths: &[String]) {
    println!("lume build: emitted {} files", paths.len());
    for path in paths {
        println!("  {path}");
    }
}

fn print_bench(result: &lume_driver::BenchResult) {
    println!("compile:");
    for metric in &result.metrics {
        println!("  {}: {}", metric.name, format_duration(metric.duration));
    }
    println!("  total: {}", format_duration(result.elapsed));
    println!("bundle:");
    println!("  files: {}", result.emitted_files);
    println!("  size: {}", format_bytes(result.bundle_bytes));
}

fn format_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();
    if nanos < 1_000 {
        format!("{nanos}ns")
    } else if nanos < 1_000_000 {
        format!("{:.2}us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    if bytes < 1024 {
        format!("{bytes} B")
    } else if (bytes as f64) < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else if (bytes as f64) < GIB {
        format!("{:.1} MiB", bytes as f64 / MIB)
    } else {
        format!("{:.1} GiB", bytes as f64 / GIB)
    }
}

fn value_after(
    args: &[String],
    index: usize,
    option: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(PathBuf::from(value_after_string(args, index, option)?))
}

fn value_after_string(
    args: &[String],
    index: usize,
    option: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {option}").into());
    };
    if value.starts_with('-') {
        return Err(format!("missing value for {option}").into());
    }
    Ok(value.clone())
}
