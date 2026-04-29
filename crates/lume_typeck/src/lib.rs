use lume_ast::*;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_hir::HirProgram;
use std::collections::HashSet;

pub fn check(program: &HirProgram) -> Diagnostics {
    check_with_known_components(program, &standard_component_symbols())
}

pub fn check_with_known_components(
    program: &HirProgram,
    known_components: &HashSet<String>,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    for decl in &program.ast.declarations {
        if let Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) = decl {
            check_component(component, known_components, &mut diagnostics);
        }
    }
    diagnostics
}

fn check_component(
    component: &ComponentDecl,
    known_components: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    let mut states = HashSet::new();
    let mut has_view = false;
    for item in &component.items {
        match item {
            ComponentItem::State(state) => {
                if !states.insert(state.name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        "LUME3005",
                        format!("duplicate state `{}`", state.name),
                        Some(state.span),
                    ));
                }
                if !matches!(
                    state.ty.as_str(),
                    "i32"
                        | "i64"
                        | "u32"
                        | "u64"
                        | "Int"
                        | "String"
                        | "Bool"
                        | "bool"
                        | "f64"
                        | "Number"
                ) {
                    diagnostics.push(Diagnostic::warning(
                        "LUME3001",
                        format!("unknown type `{}`; treating it as opaque", state.ty),
                        Some(state.span),
                    ));
                }
            }
            ComponentItem::View(view) => {
                has_view = true;
                check_view(view, &states, known_components, diagnostics);
            }
            ComponentItem::Action(action) => {
                check_statements(&action.body.statements, &states, diagnostics);
            }
            _ => {}
        }
    }
    if !has_view {
        diagnostics.push(Diagnostic::error(
            "LUME3003",
            format!("component `{}` has no view", component.name),
            Some(component.span),
        ));
    }
}

fn check_view(
    view: &ViewBlock,
    states: &HashSet<String>,
    known_components: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    for node in &view.nodes {
        match node {
            ViewNode::Element(element) => {
                check_element(element, states, known_components, diagnostics)
            }
            ViewNode::If(node) => {
                check_view(&node.then_block, states, known_components, diagnostics);
                if let Some(block) = &node.else_block {
                    check_view(block, states, known_components, diagnostics);
                }
            }
            ViewNode::For(node) => check_view(&node.body, states, known_components, diagnostics),
            ViewNode::Event(event) => check_statements(&event.body.statements, states, diagnostics),
            _ => {}
        }
    }
}

fn check_element(
    element: &ElementNode,
    states: &HashSet<String>,
    known_components: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    if !known_components.contains(&element.name) {
        diagnostics.push(Diagnostic::warning(
            "LUME3004",
            format!("unknown component `{}`", element.name),
            Some(element.span),
        ));
    }
    if element.name == "Image" && !has_attr(element, "alt") {
        diagnostics.push(Diagnostic::error(
            "LUME6201",
            "Image requires an alt attribute",
            Some(element.span),
        ));
    }
    if element.name == "Input" && !has_attr(element, "label") && !has_attr(element, "aria-label") {
        diagnostics.push(Diagnostic::error(
            "LUME6202",
            "Input requires label or aria-label",
            Some(element.span),
        ));
    }
    if element.name == "Button" && element.args.is_empty() {
        diagnostics.push(Diagnostic::warning(
            "LUME6203",
            "Button should have visible text",
            Some(element.span),
        ));
    }
    if let Some(children) = &element.children {
        check_view(children, states, known_components, diagnostics);
    }
}

fn check_statements(statements: &[Stmt], states: &HashSet<String>, diagnostics: &mut Diagnostics) {
    for stmt in statements {
        if let Stmt::Assign { target, span, .. } = stmt {
            if !states.contains(target) {
                diagnostics.push(Diagnostic::error(
                    "LUME3002",
                    format!("cannot assign to undeclared state `{target}`"),
                    Some(*span),
                ));
            }
        }
    }
}

fn has_attr(element: &ElementNode, name: &str) -> bool {
    element.attrs.iter().any(|attr| attr.name == name)
        || element
            .args
            .iter()
            .any(|arg| matches!(arg, Arg::Named(arg_name, _) if arg_name == name))
}

fn standard_component_symbols() -> HashSet<String> {
    [
        "Text",
        "Button",
        "Input",
        "Image",
        "Form",
        "Anchor",
        "Box",
        "Row",
        "Column",
        "Grid",
        "Stack",
        "Link",
        "NavLink",
        "Outlet",
        "Field",
        "VisuallyHidden",
        "FocusTrap",
        "Landmark",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{check, check_with_known_components};
    use lume_hir::lower;
    use lume_parser::parse;
    use std::collections::HashSet;

    #[test]
    fn reports_missing_input_label() {
        let source = r#"
component App {
  state name: String = ""
  view {
    Input(value=name)
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6202"));
    }

    #[test]
    fn reports_assignment_to_unknown_state() {
        let source = r#"
component App {
  view {
    Button("Go") {
      on click {
        missing += 1
      }
    }
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3002"));
    }

    #[test]
    fn reports_duplicate_state() {
        let source = r#"
component App {
  state count: i32 = 0
  state count: i32 = 1

  view {
    Text("Count {count}")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3005"));
    }

    #[test]
    fn accepts_known_custom_component() {
        let source = r#"
component Card {
  view {
    Text("Card")
  }
}

component App {
  view {
    Card()
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let mut known = HashSet::new();
        known.insert("Text".to_string());
        known.insert("Card".to_string());
        let diagnostics = check_with_known_components(&lower(program), &known);
        assert!(!diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3004"));
    }
}
