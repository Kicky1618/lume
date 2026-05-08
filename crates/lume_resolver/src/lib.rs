use lume_ast::Decl;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_hir::HirProgram;
use std::collections::HashSet;
use std::fs;
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

pub fn component_symbols(program: &HirProgram) -> HashSet<String> {
    component_symbols_with_base(program, None)
}

pub fn component_symbols_with_base(
    program: &HirProgram,
    base_dir: Option<&Path>,
) -> HashSet<String> {
    let mut symbols = standard_component_symbols();
    for decl in &program.ast.declarations {
        collect_local_component_decl(decl, &mut symbols);
        if let Decl::Import { items, from } = decl {
            collect_imported_component_symbols(items, from, base_dir, &mut symbols);
        }
    }
    symbols
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
                    return;
                }
                match local_exports(&path) {
                    Ok(exports) => {
                        for item in items {
                            if !exports.contains(item) {
                                diagnostics.push(Diagnostic::error(
                                    "LUME6004",
                                    format!("local module `{from}` does not export `{item}`"),
                                    None,
                                ));
                            }
                        }
                    }
                    Err(message) => diagnostics.push(Diagnostic::error(
                        "LUME6107",
                        format!("local import `{from}` could not be inspected: {message}"),
                        None,
                    )),
                }
            }
        } else if from.ends_with(".js") || from.starts_with("npm:") {
            diagnostics.push(Diagnostic::error(
                "LUME6009",
                format!("raw JavaScript imports are not supported in v0.2: `{from}`"),
                None,
            ));
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

fn collect_imported_component_symbols(
    items: &[String],
    from: &str,
    base_dir: Option<&Path>,
    symbols: &mut HashSet<String>,
) {
    if standard_exports(from).is_some() {
        for item in items {
            if is_standard_component(item) {
                symbols.insert(item.clone());
            }
        }
        return;
    }

    let Some(base_dir) = base_dir else {
        return;
    };
    if !from.starts_with('.') {
        return;
    }
    let path = base_dir.join(from);
    let Ok(exports) = local_exports(&path) else {
        return;
    };
    for item in items {
        if exports.contains(item) {
            symbols.insert(item.clone());
        }
    }
}

fn collect_local_component_decl(decl: &Decl, symbols: &mut HashSet<String>) {
    match decl {
        Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
            symbols.insert(component.name.clone());
        }
        Decl::Export(inner) => collect_local_component_decl(inner, symbols),
        _ => {}
    }
}

fn local_exports(path: &Path) -> Result<HashSet<String>, String> {
    let source = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let (program, diagnostics) = lume_parser::parse(&source);
    if diagnostics.has_errors() {
        return Err("parse failed".into());
    }
    Ok(program
        .declarations
        .iter()
        .filter_map(exported_name)
        .collect())
}

fn exported_name(decl: &Decl) -> Option<String> {
    let Decl::Export(inner) = decl else {
        return None;
    };
    match inner.as_ref() {
        Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
            Some(component.name.clone())
        }
        Decl::Style(style) => Some(style.name.clone()),
        Decl::Theme(theme) => Some(theme.name.clone()),
        Decl::Type(decl)
        | Decl::App(decl)
        | Decl::Form(decl)
        | Decl::Ffi(decl)
        | Decl::Reserved(decl) => decl.name.clone(),
        Decl::Query(decl) => Some(decl.name.clone()),
        Decl::FfiModule(decl) => Some(decl.name.clone()),
        Decl::FfiStruct(decl) => Some(decl.name.clone()),
        Decl::FfiEnum(decl) => Some(decl.name.clone()),
        Decl::FfiOpaque(decl) => Some(decl.name.clone()),
        Decl::ServerAction(decl) => Some(decl.name.clone()),
        _ => None,
    }
}

fn standard_component_symbols() -> HashSet<String> {
    [
        "Text",
        "Button",
        "Input",
        "TextArea",
        "Image",
        "Script",
        "Form",
        "Anchor",
        "Modal",
        "Dialog",
        "Tabs",
        "Table",
        "Spacer",
        "Box",
        "Container",
        "Row",
        "Column",
        "Grid",
        "Stack",
        "Router",
        "Route",
        "Link",
        "NavLink",
        "Outlet",
        "Canvas",
        "ImageCanvas",
        "NativeCanvas",
        "GpuCanvas",
        "Field",
        "VisuallyHidden",
        "FocusTrap",
        "Landmark",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn is_standard_component(name: &str) -> bool {
    standard_component_symbols().contains(name)
}

fn standard_exports(module: &str) -> Option<&'static [&'static str]> {
    match module {
        "lume/std/ui" => Some(&[
            "Text",
            "Button",
            "Input",
            "TextArea",
            "Image",
            "Script",
            "Form",
            "Anchor",
            "Canvas",
            "ImageCanvas",
            "NativeCanvas",
            "Modal",
            "Dialog",
            "Tabs",
            "Table",
            "Spacer",
        ]),
        "lume/std/layout" => Some(&[
            "Box",
            "Row",
            "Column",
            "Grid",
            "Stack",
            "Container",
            "Spacer",
        ]),
        "lume/std/router" => Some(&[
            "Router",
            "Route",
            "Link",
            "NavLink",
            "Outlet",
            "navigate",
            "redirect",
            "notFound",
            "prefetchRoute",
        ]),
        "lume/std/form" => Some(&["Form", "Field", "FormData", "validate"]),
        "lume/std/action" => Some(&[
            "ActionResult",
            "ActionError",
            "ActionController",
            "ActionStatus",
            "callAction",
            "useAction",
        ]),
        "lume/std/query" => Some(&["query", "invalidate"]),
        "lume/std/bytes" => Some(&["bytes"]),
        "lume/std/ffi" => Some(&[
            "Owned",
            "Borrowed",
            "View",
            "Handle",
            "Ptr",
            "StatusCode",
            "CanvasSurface",
        ]),
        "lume/std/image" => Some(&["ImageCanvas", "RgbaImage"]),
        "lume/std/gpu" => Some(&["GpuCanvas", "gpu"]),
        "lume/std/a11y" => Some(&["VisuallyHidden", "FocusTrap", "Landmark"]),
        "lume/std/i18n" => Some(&["t", "locale"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{component_symbols_with_base, resolve};
    use lume_hir::lower;
    use lume_parser::parse;
    use std::fs;

    #[test]
    fn accepts_known_standard_imports() {
        let source = r#"
import { Text, Button, TextArea, Modal, Dialog, Tabs, Table, Spacer } from "lume/std/ui"
import { Container } from "lume/std/layout"
import { bytes } from "lume/std/bytes"
import { ImageCanvas, RgbaImage } from "lume/std/image"
import { Router, Route } from "lume/std/router"
import { ActionController, ActionResult, callAction, useAction } from "lume/std/action"

component App {
  view {
    Container {
      Router {
        Route {
          Text("ok")
          TextArea(label="ok")
          Modal(title="ok") {
            Dialog(title="ok") {
              Tabs {
                Button("tab")
              }
              Table {
                Text("cell")
              }
              Spacer height=8
            }
          }
        }
      }
    }
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

    #[test]
    fn accepts_exported_local_import() {
        let temp = std::env::temp_dir().join("lume-exported-local-import-test");
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("Card.lume"),
            r#"export component Card {
  view {
    Text("card")
  }
}
"#,
        )
        .unwrap();
        let source = r#"
import { Card } from "./Card.lume"

component App {
  view {
    Card()
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let hir = lower(program);
        let diagnostics = super::resolve_with_base(&hir, Some(&temp));
        assert!(!diagnostics.has_errors());
        assert!(component_symbols_with_base(&hir, Some(&temp)).contains("Card"));
    }

    #[test]
    fn reports_private_local_import() {
        let temp = std::env::temp_dir().join("lume-private-local-import-test");
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("Card.lume"),
            r#"component Card {
  view {
    Text("card")
  }
}
"#,
        )
        .unwrap();
        let source = r#"
import { Card } from "./Card.lume"

component App {
  view {
    Card()
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = super::resolve_with_base(&lower(program), Some(&temp));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6004"));
    }
}
