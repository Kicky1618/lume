use lume_span::{SourceFile, SourceMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildTarget {
    HtmlJsCss,
    HtmlJsCssWasm,
}

impl BuildTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HtmlJsCss => "html-js-css",
            Self::HtmlJsCssWasm => "html-js-css-wasm",
        }
    }

    pub fn wasm_enabled(self) -> bool {
        matches!(self, Self::HtmlJsCssWasm)
    }
}

impl Default for BuildTarget {
    fn default() -> Self {
        Self::HtmlJsCssWasm
    }
}

impl FromStr for BuildTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "html-js-css" => Ok(Self::HtmlJsCss),
            "html-js-css-wasm" => Ok(Self::HtmlJsCssWasm),
            other => Err(format!(
                "unknown build target `{other}`; expected `html-js-css` or `html-js-css-wasm`"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontendActivation {
    Hydrate,
    PartialHydrate,
    Resume,
}

impl FrontendActivation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hydrate => "hydrate",
            Self::PartialHydrate => "partial-hydrate",
            Self::Resume => "resume",
        }
    }

    pub fn is_resume(self) -> bool {
        matches!(self, Self::Resume)
    }
}

impl Default for FrontendActivation {
    fn default() -> Self {
        Self::Hydrate
    }
}

impl FromStr for FrontendActivation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hydrate" => Ok(Self::Hydrate),
            "partial-hydrate" | "partial_hydrate" => Ok(Self::PartialHydrate),
            "resume" => Ok(Self::Resume),
            other => Err(format!(
                "unknown frontend activation `{other}`; expected `hydrate`, `partial-hydrate`, or `resume`"
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub project_name: String,
    pub entry: PathBuf,
    pub out_dir: PathBuf,
    pub target: BuildTarget,
    pub activation: FrontendActivation,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            project_name: "lume-app".into(),
            entry: PathBuf::from("src/app.lume"),
            out_dir: PathBuf::from("dist"),
            target: BuildTarget::default(),
            activation: FrontendActivation::default(),
        }
    }
}

impl BuildOptions {
    pub fn from_args(
        entry: Option<PathBuf>,
        out_dir: Option<PathBuf>,
        target: Option<BuildTarget>,
        config: Option<PathBuf>,
    ) -> io::Result<Self> {
        let mut options = if let Some(config) = config {
            Self::from_toml_file(config)?
        } else if Path::new("lume.toml").exists() {
            Self::from_toml_file("lume.toml")?
        } else {
            Self::default()
        };
        if let Some(entry) = entry {
            options.entry = entry;
        }
        if let Some(out_dir) = out_dir {
            options.out_dir = out_dir;
        }
        if let Some(target) = target {
            options.target = target;
        }
        Ok(options)
    }

    pub fn from_toml_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new(""));
        let mut options = Self::default();
        let mut section = String::new();
        let mut explicit_build_target = false;
        let mut explicit_activation = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line.trim_matches(['[', ']']).to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match (section.as_str(), key) {
                ("project" | "", "name") => options.project_name = value.into(),
                ("project" | "", "entry") => options.entry = config_relative_path(base_dir, value),
                ("project" | "", "out_dir") => {
                    options.out_dir = config_relative_path(base_dir, value)
                }
                ("build" | "", "target") => {
                    options.target = value.parse().map_err(invalid_data)?;
                    explicit_build_target = true;
                }
                ("frontend", "activation") => {
                    options.activation = value.parse().map_err(invalid_data)?;
                    explicit_activation = true;
                }
                ("frontend", "hydration") if !explicit_activation => {
                    options.activation = match value {
                        "partial" => FrontendActivation::PartialHydrate,
                        "full" | "hydrate" => FrontendActivation::Hydrate,
                        other => {
                            return Err(invalid_data(format!(
                                "invalid frontend.hydration value `{other}`; expected partial, full, or hydrate"
                            )))
                        }
                    };
                }
                ("wasm", "enabled") if !explicit_build_target => {
                    options.target = match value {
                        "true" => BuildTarget::HtmlJsCssWasm,
                        "false" => BuildTarget::HtmlJsCss,
                        other => {
                            return Err(invalid_data(format!(
                                "invalid wasm.enabled value `{other}`; expected true or false"
                            )))
                        }
                    };
                }
                _ => {}
            }
        }
        Ok(options)
    }
}

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn config_relative_path(base_dir: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Session {
    pub source_map: SourceMap,
}

impl Session {
    pub fn load_source(&mut self, path: impl AsRef<Path>) -> io::Result<SourceFile> {
        let path = path.as_ref().to_path_buf();
        let source = fs::read_to_string(&path)?;
        let id = self.source_map.add_file(path.clone(), source.clone());
        Ok(SourceFile { id, path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildOptions, BuildTarget, FrontendActivation};
    use std::fs;

    #[test]
    fn parses_build_target_without_confusing_wasm_target_triple() {
        let path =
            std::env::temp_dir().join(format!("lume-session-target-{}.toml", std::process::id()));
        fs::write(
            &path,
            "[project]\nname = \"demo\"\nentry = \"src/main.lume\"\nout_dir = \"public\"\n\n[build]\ntarget = \"html-js-css-wasm\"\n\n[wasm]\ntarget = \"wasm32-unknown-unknown\"\n",
        )
        .expect("write config");

        let options = BuildOptions::from_toml_file(&path).expect("parse config");
        fs::remove_file(path).ok();

        assert_eq!(options.project_name, "demo");
        assert_eq!(options.target, BuildTarget::HtmlJsCssWasm);
        assert!(options.target.wasm_enabled());
    }

    #[test]
    fn resolves_project_paths_relative_to_config_file() {
        let dir = std::env::temp_dir().join(format!("lume-session-paths-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp config dir");
        let path = dir.join("lume.toml");
        fs::write(
            &path,
            "[project]\nname = \"demo\"\nentry = \"src/main.lume\"\nout_dir = \"public\"\n",
        )
        .expect("write config");

        let options = BuildOptions::from_toml_file(&path).expect("parse config");
        fs::remove_file(&path).ok();
        fs::remove_dir(&dir).ok();

        assert_eq!(options.entry, dir.join("src/main.lume"));
        assert_eq!(options.out_dir, dir.join("public"));
    }

    #[test]
    fn parses_frontend_activation() {
        let path = std::env::temp_dir().join(format!(
            "lume-session-activation-{}.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            "[project]\nname = \"demo\"\n\n[frontend]\nactivation = \"resume\"\nhydration = \"partial\"\n",
        )
        .expect("write config");

        let options = BuildOptions::from_toml_file(&path).expect("parse config");
        fs::remove_file(path).ok();

        assert_eq!(options.activation, FrontendActivation::Resume);
    }

    #[test]
    fn maps_legacy_partial_hydration_to_partial_activation() {
        let path = std::env::temp_dir().join(format!(
            "lume-session-hydration-{}.toml",
            std::process::id()
        ));
        fs::write(&path, "[frontend]\nhydration = \"partial\"\n").expect("write config");

        let options = BuildOptions::from_toml_file(&path).expect("parse config");
        fs::remove_file(path).ok();

        assert_eq!(options.activation, FrontendActivation::PartialHydrate);
    }
}
