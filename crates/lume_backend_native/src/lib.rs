#[derive(Clone, Debug, Default)]
pub struct NativeBackend;

#[cfg(test)]
mod tests {
    use super::NativeBackend;

    #[test]
    fn skeleton_is_constructible() {
        let _backend = NativeBackend;
    }
}
