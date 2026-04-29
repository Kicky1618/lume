use lume_span::{SourceFile, Span};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span,
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{severity}[{}]: {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }
}

pub fn emit(diagnostics: &[Diagnostic], file: Option<&SourceFile>) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        out.push_str(&format!("{diagnostic}\n"));
        if let (Some(file), Some(span)) = (file, diagnostic.span) {
            let (line, col) = file.line_col(span.start);
            out.push_str(&format!("  --> {}:{line}:{col}\n", file.path.display()));
            if let Some(text) = file.line_text(line) {
                out.push_str(&format!("{line:>4} | {text}\n"));
                let marker_width = span.end.saturating_sub(span.start).max(1);
                out.push_str(&format!(
                    "     | {}{}\n",
                    " ".repeat(col.saturating_sub(1)),
                    "^".repeat(marker_width)
                ));
            }
        }
    }
    if !diagnostics.is_empty() {
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        let warnings = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Warning)
            .count();
        out.push_str(&format!(
            "{}{}, {}{}\n",
            errors,
            if errors == 1 { " error" } else { " errors" },
            warnings,
            if warnings == 1 {
                " warning"
            } else {
                " warnings"
            }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{emit, Diagnostic};
    use lume_span::{SourceFile, Span};
    use std::path::PathBuf;

    #[test]
    fn emits_caret_width_and_summary() {
        let file = SourceFile {
            id: 0,
            path: PathBuf::from("app.lume"),
            source: "component App {}\n".into(),
        };
        let output = emit(
            &[
                Diagnostic::error("LUME0001", "bad", Some(Span::new(0, 9))),
                Diagnostic::warning("LUME0002", "soft", None),
            ],
            Some(&file),
        );
        assert!(output.contains("^^^^^^^^^"));
        assert!(output.contains("1 error, 1 warning"));
    }
}
