use lume_session::BuildOptions;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("lume: {err}");
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
            let options = options_from(&rest)?;
            let result = lume_driver::build(options)?;
            Ok(
                if result
                    .diagnostics
                    .iter()
                    .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
                {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                },
            )
        }
        "check" => {
            let options = options_from(&rest)?;
            let result = lume_driver::check(options)?;
            Ok(
                if result
                    .diagnostics
                    .iter()
                    .any(|d| matches!(d.severity, lume_diagnostics::Severity::Error))
                {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                },
            )
        }
        "dev" => {
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
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--entry" | "-e" => {
                i += 1;
                entry = args.get(i).map(PathBuf::from);
            }
            "--out-dir" | "-o" => {
                i += 1;
                out_dir = args.get(i).map(PathBuf::from);
            }
            "--port" | "-p" => {
                i += 1;
            }
            path if !path.starts_with('-') && entry.is_none() => entry = Some(PathBuf::from(path)),
            _ => {}
        }
        i += 1;
    }
    Ok(BuildOptions::from_args(entry, out_dir)?)
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
    println!("lume 0.1.0\n\nUSAGE:\n  lume build [--entry src/app.lume] [--out-dir dist]\n  lume dev [--entry src/app.lume] [--out-dir dist] [--port 3000]\n  lume check [--entry src/app.lume]\n  lume fmt [--check] [path]\n  lume init");
}
