#[derive(Clone, Debug, Default)]
pub struct WasmBackend;

impl WasmBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn emit_skeleton(&self) -> &'static [u8] {
        b""
    }
}

#[cfg(test)]
mod tests {
    use super::WasmBackend;

    #[test]
    fn skeleton_emits_empty_placeholder() {
        assert_eq!(WasmBackend::new().emit_skeleton(), b"");
    }
}
