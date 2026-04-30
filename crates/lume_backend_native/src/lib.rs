#[derive(Clone, Debug, Default)]
pub struct NativeBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeArtifact {
    pub target: String,
    pub symbols: Vec<String>,
}

impl NativeBackend {
    pub fn emit_artifact(&self, target: impl Into<String>, symbols: Vec<String>) -> NativeArtifact {
        NativeArtifact {
            target: target.into(),
            symbols,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeBackend;

    #[test]
    fn skeleton_is_constructible() {
        let _backend = NativeBackend;
    }

    #[test]
    fn emits_native_artifact_metadata() {
        let artifact = NativeBackend.emit_artifact("native", vec!["add".into()]);
        assert_eq!(artifact.target, "native");
        assert_eq!(artifact.symbols, vec!["add"]);
    }
}
