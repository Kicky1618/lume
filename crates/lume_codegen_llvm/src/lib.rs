#[derive(Clone, Debug, Default)]
pub struct LlvmBackend;

impl LlvmBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::LlvmBackend;

    #[test]
    fn skeleton_backend_is_constructible() {
        let backend = LlvmBackend::new();
        assert!(!backend.is_available());
    }
}
