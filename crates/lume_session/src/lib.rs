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
    pub frontend: FrontendOptions,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            project_name: "lume-app".into(),
            entry: PathBuf::from("src/app.lume"),
            out_dir: PathBuf::from("dist"),
            target: BuildTarget::default(),
            activation: FrontendActivation::default(),
            frontend: FrontendOptions::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontendOptions {
    pub routing: FrontendRouting,
    pub base_path: String,
    pub trailing_slash: TrailingSlash,
    pub router: RouterOptions,
}

impl Default for FrontendOptions {
    fn default() -> Self {
        Self {
            routing: FrontendRouting::default(),
            base_path: "/".into(),
            trailing_slash: TrailingSlash::default(),
            router: RouterOptions::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontendRouting {
    Spa,
    Mpa,
    Hybrid,
    Server,
}

impl FrontendRouting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spa => "spa",
            Self::Mpa => "mpa",
            Self::Hybrid => "hybrid",
            Self::Server => "server",
        }
    }

    pub fn emits_route_html(self) -> bool {
        matches!(self, Self::Mpa | Self::Hybrid)
    }

    pub fn uses_spa_fallback(self) -> bool {
        matches!(self, Self::Spa | Self::Hybrid)
    }
}

impl Default for FrontendRouting {
    fn default() -> Self {
        Self::Spa
    }
}

impl FromStr for FrontendRouting {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "spa" => Ok(Self::Spa),
            "mpa" => Ok(Self::Mpa),
            "hybrid" => Ok(Self::Hybrid),
            "server" => Ok(Self::Server),
            other => Err(format!(
                "unknown frontend.routing `{other}`; expected `spa`, `mpa`, `hybrid`, or `server`"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrailingSlash {
    Never,
    Always,
    Preserve,
}

impl TrailingSlash {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Always => "always",
            Self::Preserve => "preserve",
        }
    }
}

impl Default for TrailingSlash {
    fn default() -> Self {
        Self::Never
    }
}

impl FromStr for TrailingSlash {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "never" => Ok(Self::Never),
            "always" => Ok(Self::Always),
            "preserve" | "auto" => Ok(Self::Preserve),
            other => Err(format!(
                "unknown frontend.trailing_slash `{other}`; expected `never`, `always`, or `preserve`"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterOptions {
    pub scroll_restoration: bool,
    pub focus_main_on_navigation: bool,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            scroll_restoration: true,
            focus_main_on_navigation: true,
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
                ("frontend", "routing") => {
                    options.frontend.routing = value.parse().map_err(invalid_data)?;
                }
                ("frontend", "base_path") => {
                    options.frontend.base_path = normalize_base_path(value)
                }
                ("frontend", "trailing_slash") => {
                    options.frontend.trailing_slash = value.parse().map_err(invalid_data)?;
                }
                ("frontend.router", "scroll_restoration") => {
                    options.frontend.router.scroll_restoration =
                        parse_bool(value, "frontend.router.scroll_restoration")?;
                }
                ("frontend.router", "focus_main_on_navigation") => {
                    options.frontend.router.focus_main_on_navigation =
                        parse_bool(value, "frontend.router.focus_main_on_navigation")?;
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

fn parse_bool(value: &str, key: &str) -> io::Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(invalid_data(format!(
            "invalid {key} value `{other}`; expected true or false"
        ))),
    }
}

fn normalize_base_path(value: &str) -> String {
    let mut base = value.trim().to_string();
    if base.is_empty() || base == "/" {
        return "/".into();
    }
    if !base.starts_with('/') {
        base.insert(0, '/');
    }
    while base.len() > 1 && base.ends_with('/') {
        base.pop();
    }
    base
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
    use super::{BuildOptions, BuildTarget, FrontendActivation, FrontendRouting, TrailingSlash};
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

    #[test]
    fn parses_frontend_router_output_options() {
        let path =
            std::env::temp_dir().join(format!("lume-session-routing-{}.toml", std::process::id()));
        fs::write(
            &path,
            "[frontend]\nrouting = \"hybrid\"\nbase_path = \"docs/\"\ntrailing_slash = \"always\"\n\n[frontend.router]\nscroll_restoration = false\nfocus_main_on_navigation = true\n",
        )
        .expect("write config");

        let options = BuildOptions::from_toml_file(&path).expect("parse config");
        fs::remove_file(path).ok();

        assert_eq!(options.frontend.routing, FrontendRouting::Hybrid);
        assert_eq!(options.frontend.base_path, "/docs");
        assert_eq!(options.frontend.trailing_slash, TrailingSlash::Always);
        assert!(!options.frontend.router.scroll_restoration);
        assert!(options.frontend.router.focus_main_on_navigation);
    }
}
