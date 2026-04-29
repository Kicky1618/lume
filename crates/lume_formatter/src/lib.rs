pub fn format_source(source: &str) -> String {
    let mut out = String::new();
    let mut indent = 0usize;
    let mut blank = false;
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if !blank {
                out.push('\n');
            }
            blank = true;
            continue;
        }
        if line.starts_with('}') {
            indent = indent.saturating_sub(1);
        }
        out.push_str(&"  ".repeat(indent));
        out.push_str(line);
        out.push('\n');
        if line.ends_with('{') {
            indent += 1;
        }
        if line.ends_with('}') && !line.starts_with('}') {
            indent = indent.saturating_sub(1);
        }
        blank = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_source;

    #[test]
    fn formatting_is_idempotent() {
        let source = "component App {\nview {\nText(\"Hi\")\n}\n}\n";
        let once = format_source(source);
        let twice = format_source(&once);
        assert_eq!(once, twice);
    }
}
