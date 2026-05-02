use lume_session::{BuildOptions, BuildTarget};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
            if has_errors(&result) {
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
            if has_errors(&result) {
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
        "lume {version}

USAGE:
  lume build [options] [entry]
  lume dev [options] [entry]
  lume check [options] [entry]
  lume fmt [--check] [path]
  lume init

COMMON OPTIONS:
  -e, --entry <path>       Entry file, default src/app.lume
  -o, --out-dir <path>     Output directory, default dist
  -c, --config <path>      Config file, default lume.toml
  -t, --target <target>    html-js-css or html-js-css-wasm
      --wasm               Shortcut for --target html-js-css-wasm
      --no-wasm            Shortcut for --target html-js-css
  -h, --help               Show help
  -V, --version            Show version

EXAMPLES:
  lume build
  lume build --target html-js-css-wasm
  lume dev --port 3000
  lume check examples/counter/src/app.lume",
        version = env!("CARGO_PKG_VERSION")
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

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "-h" || arg == "--help")
}

fn has_errors(result: &lume_driver::BuildResult) -> bool {
    result
        .diagnostics
        .iter()
        .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
}

fn print_emitted(paths: &[String]) {
    println!("lume build: emitted {} files", paths.len());
    for path in paths {
        println!("  {path}");
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
