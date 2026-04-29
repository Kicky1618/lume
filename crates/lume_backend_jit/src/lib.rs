#[derive(Clone, Debug, Default)]
pub struct JitBackend;

#[cfg(test)]
mod tests {
    use super::JitBackend;

    #[test]
    fn skeleton_is_constructible() {
        let _backend = JitBackend;
    }
}
