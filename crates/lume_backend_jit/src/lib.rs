use lume_ast::ServerActionDecl;
use lume_codegen_llvm::LlvmBackend;

#[derive(Clone, Debug, Default)]
pub struct JitBackend {
    llvm: LlvmBackend,
}

#[derive(Clone, Debug)]
pub struct JitOutput {
    pub value: i64,
    pub llvm_ir: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JitError {
    pub message: String,
}

impl JitBackend {
    pub fn new() -> Self {
        Self {
            llvm: LlvmBackend::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.llvm.is_available()
    }

    pub fn execute_i64(
        &self,
        action: &ServerActionDecl,
        args: &[i64],
    ) -> Result<JitOutput, JitError> {
        let output = self
            .llvm
            .execute_server_action_i64(action, args)
            .map_err(JitError::new)?;
        Ok(JitOutput {
            value: output.value,
            llvm_ir: output.ir,
        })
    }
}

impl JitError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JitBackend;

    #[test]
    fn backend_is_constructible() {
        let _backend = JitBackend::new();
    }

    #[test]
    fn availability_check_does_not_panic() {
        let _ = JitBackend::new().is_available();
    }
}
