use lume_ast::Decl;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_hir::HirProgram;
use std::path::Path;

pub fn resolve(program: &HirProgram) -> Diagnostics {
    resolve_with_base(program, None)
}

pub fn resolve_with_base(program: &HirProgram, base_dir: Option<&Path>) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    for decl in &program.ast.declarations {
        if let Decl::Import { items, from } = decl {
            check_import(items, from, base_dir, &mut diagnostics);
        }
    }
    diagnostics
}

fn check_import(
    items: &[String],
    from: &str,
    base_dir: Option<&Path>,
    diagnostics: &mut Diagnostics,
) {
    let Some(exports) = standard_exports(from) else {
        if from.starts_with("lume/std/") {
            diagnostics.push(Diagnostic::error(
                "LUME6101",
                format!("unknown standard module `{from}`"),
                None,
            ));
        } else if from.starts_with('.') {
            if let Some(base_dir) = base_dir {
                let path = base_dir.join(from);
                if !path.exists() {
                    diagnostics.push(Diagnostic::error(
                        "LUME6106",
                        format!("local import cannot be resolved: `{from}`"),
                        None,
                    ));
                }
            }
        }
        return;
    };

    for item in items {
        if !exports.contains(&item.as_str()) {
            diagnostics.push(Diagnostic::error(
                "LUME6102",
                format!("standard module `{from}` does not export `{item}`"),
                None,
            ));
        }
    }
}

fn standard_exports(module: &str) -> Option<&'static [&'static str]> {
    match module {
        "lume/std/ui" => Some(&["Text", "Button", "Input", "Image", "Form", "Anchor"]),
        "lume/std/layout" => Some(&["Box", "Row", "Column", "Grid", "Stack"]),
        "lume/std/router" => Some(&[
            "Link",
            "NavLink",
            "Outlet",
            "navigate",
            "redirect",
            "notFound",
            "prefetchRoute",
        ]),
        "lume/std/form" => Some(&["Form", "Field", "FormData", "validate"]),
        "lume/std/action" => Some(&["ActionResult", "ActionError"]),
        "lume/std/query" => Some(&["query", "invalidate"]),
        "lume/std/ffi" => Some(&["Owned", "Borrowed", "View", "Handle", "Ptr", "StatusCode"]),
        "lume/std/a11y" => Some(&["VisuallyHidden", "FocusTrap", "Landmark"]),
        "lume/std/i18n" => Some(&["t", "locale"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use lume_hir::lower;
    use lume_parser::parse;

    #[test]
    fn accepts_known_standard_imports() {
        let source = r#"
import { Text, Button } from "lume/std/ui"

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = resolve(&lower(program));
        assert!(!diagnostics.has_errors());
    }

    #[test]
    fn reports_unknown_standard_item() {
        let source = r#"
import { Nope } from "lume/std/ui"

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = resolve(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6102"));
    }

    #[test]
    fn reports_missing_local_import() {
        let source = r#"
import { Card } from "./Missing.lume"

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let temp = std::env::temp_dir().join("lume-missing-local-import-test");
        let diagnostics = super::resolve_with_base(&lower(program), Some(&temp));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6106"));
    }
}
