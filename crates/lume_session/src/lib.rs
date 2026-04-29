use lume_span::{SourceFile, SourceMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub project_name: String,
    pub entry: PathBuf,
    pub out_dir: PathBuf,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            project_name: "lume-app".into(),
            entry: PathBuf::from("src/app.lume"),
            out_dir: PathBuf::from("dist"),
        }
    }
}

impl BuildOptions {
    pub fn from_args(entry: Option<PathBuf>, out_dir: Option<PathBuf>) -> io::Result<Self> {
        let mut options = if Path::new("lume.toml").exists() {
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
        Ok(options)
    }

    pub fn from_toml_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let mut options = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with('[') || line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "name" => options.project_name = value.into(),
                "entry" => options.entry = PathBuf::from(value),
                "out_dir" => options.out_dir = PathBuf::from(value),
                _ => {}
            }
        }
        Ok(options)
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
