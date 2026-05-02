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
    let route_paths = collect_route_paths(program);
    let server_actions = collect_server_actions(program);
    check_server_actions(&server_actions, program, &mut diagnostics);
    check_ffi(program, &mut diagnostics);
    let ctx = CheckCtx {
        route_paths: &route_paths,
        server_actions: &server_actions,
    };
    for decl in &program.ast.declarations {
        match decl {
            Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
                check_component(component, known_components, &ctx, &mut diagnostics);
            }
            Decl::Route(route) => check_route(route, known_components, &ctx, &mut diagnostics),
            Decl::Export(inner) => match inner.as_ref() {
                Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
                    check_component(component, known_components, &ctx, &mut diagnostics);
                }
                Decl::Route(route) => check_route(route, known_components, &ctx, &mut diagnostics),
                _ => {}
            },
            _ => {}
        }
    }
    diagnostics
}

struct CheckCtx<'a> {
    route_paths: &'a HashSet<String>,
    server_actions: &'a HashSet<String>,
}

fn check_component(
    component: &ComponentDecl,
    known_components: &HashSet<String>,
    ctx: &CheckCtx<'_>,
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
                check_view(view, &states, known_components, ctx, diagnostics);
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

fn check_route(
    route: &RouteDecl,
    known_components: &HashSet<String>,
    ctx: &CheckCtx<'_>,
    diagnostics: &mut Diagnostics,
) {
    let states = HashSet::new();
    if let Some(index) = &route.body.index {
        check_view(index, &states, known_components, ctx, diagnostics);
    }
    let view = ViewBlock {
        nodes: route.body.view_nodes.clone(),
        span: route.body.span,
    };
    check_view(&view, &states, known_components, ctx, diagnostics);
    for child in &route.body.children {
        check_route(child, known_components, ctx, diagnostics);
    }
}

fn check_view(
    view: &ViewBlock,
    states: &HashSet<String>,
    known_components: &HashSet<String>,
    ctx: &CheckCtx<'_>,
    diagnostics: &mut Diagnostics,
) {
    for node in &view.nodes {
        match node {
            ViewNode::Element(element) => {
                check_element(element, states, known_components, ctx, diagnostics)
            }
            ViewNode::If(node) => {
                check_view(&node.then_block, states, known_components, ctx, diagnostics);
                if let Some(block) = &node.else_block {
                    check_view(block, states, known_components, ctx, diagnostics);
                }
            }
            ViewNode::For(node) => {
                check_view(&node.body, states, known_components, ctx, diagnostics)
            }
            ViewNode::Event(event) => check_statements(&event.body.statements, states, diagnostics),
            _ => {}
        }
    }
}

fn check_element(
    element: &ElementNode,
    states: &HashSet<String>,
    known_components: &HashSet<String>,
    ctx: &CheckCtx<'_>,
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
    if matches!(element.name.as_str(), "Link" | "NavLink") {
        check_link(element, ctx, diagnostics);
    }
    if element.name == "Form" {
        check_form(element, ctx, diagnostics);
    }
    if matches!(
        element.name.as_str(),
        "Canvas" | "NativeCanvas" | "GpuCanvas"
    ) {
        check_canvas(element, diagnostics);
    }
    if let Some(children) = &element.children {
        check_view(children, states, known_components, ctx, diagnostics);
    }
}

fn check_canvas(element: &ElementNode, diagnostics: &mut Diagnostics) {
    if element.name == "Canvas"
        && (has_attr(element, "nativeModule")
            || has_attr(element, "native_module")
            || has_attr(element, "nativeSymbol")
            || has_attr(element, "native_symbol")
            || has_attr(element, "nativeArgs")
            || has_attr(element, "native_args"))
    {
        diagnostics.push(Diagnostic::error(
            "LUME5201",
            "Canvas cannot use nativeModule/nativeSymbol/nativeArgs; use NativeCanvas(renderer=..., args={...})",
            Some(element.span),
        ));
    }
    if element.name == "NativeCanvas" {
        match attr_literal(element, "renderer") {
            Some(renderer) if renderer.contains('.') => {}
            _ => diagnostics.push(Diagnostic::error(
                "LUME5202",
                "NativeCanvas renderer must be a module function such as renderkit.mandelbrot_render",
                Some(element.span),
            )),
        }
        if let Some(args) = attr_raw(element, "args") {
            let raw = args.trim();
            if !(raw.starts_with('{') && raw.ends_with('}')) {
                diagnostics.push(Diagnostic::error(
                    "LUME5203",
                    "NativeCanvas args must be a structured object",
                    Some(element.span),
                ));
            }
        }
    }
    if element.name == "GpuCanvas" && !has_attr(element, "graph") {
        diagnostics.push(Diagnostic::error(
            "LUME5205",
            "GpuCanvas graph must be a GPU graph value",
            Some(element.span),
        ));
    }
}

fn check_link(element: &ElementNode, ctx: &CheckCtx<'_>, diagnostics: &mut Diagnostics) {
    let Some(target) = attr_literal(element, "to").or_else(|| attr_literal(element, "href")) else {
        diagnostics.push(Diagnostic::error(
            "LUME6204",
            format!("{} requires a `to` or `href` attribute", element.name),
            Some(element.span),
        ));
        return;
    };
    if target.starts_with("javascript:") || target.starts_with("data:") {
        diagnostics.push(Diagnostic::error(
            "LUME6205",
            format!("{} target uses a disallowed URL scheme", element.name),
            Some(element.span),
        ));
        return;
    }
    if target.starts_with('/')
        && !target.contains('{')
        && !ctx.route_paths.is_empty()
        && !ctx
            .route_paths
            .iter()
            .any(|route| route_path_matches(route, &target))
    {
        diagnostics.push(Diagnostic::warning(
            "LUME7010",
            format!("{} points to unknown route `{target}`", element.name),
            Some(element.span),
        ));
    }
}

fn check_form(element: &ElementNode, ctx: &CheckCtx<'_>, diagnostics: &mut Diagnostics) {
    let Some(action) = attr_literal(element, "action") else {
        return;
    };
    if action.starts_with("javascript:") || action.starts_with("data:") {
        diagnostics.push(Diagnostic::error(
            "LUME6206",
            "Form action uses a disallowed URL scheme",
            Some(element.span),
        ));
        return;
    }
    if !action.starts_with('/')
        && !action.starts_with("http://")
        && !action.starts_with("https://")
        && !ctx.server_actions.contains(&action)
    {
        diagnostics.push(Diagnostic::error(
            "LUME6207",
            format!("Form action `{action}` does not match a server action"),
            Some(element.span),
        ));
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

fn attr_literal(element: &ElementNode, name: &str) -> Option<String> {
    attr_raw(element, name).and_then(|raw| {
        let raw = raw.trim();
        if raw.starts_with('"') && raw.ends_with('"') {
            Some(raw.trim_matches('"').to_string())
        } else if raw.starts_with('\'') && raw.ends_with('\'') {
            Some(raw.trim_matches('\'').to_string())
        } else if raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            Some(raw.to_string())
        } else {
            None
        }
    })
}

fn attr_raw<'a>(element: &'a ElementNode, name: &str) -> Option<&'a str> {
    element
        .attrs
        .iter()
        .find_map(|attr| (attr.name == name).then_some(attr.value.as_ref()).flatten())
        .or_else(|| {
            element.args.iter().find_map(|arg| match arg {
                Arg::Named(arg_name, expr) if arg_name == name => Some(expr),
                _ => None,
            })
        })
        .map(|expr| expr.raw.as_str())
}

fn collect_server_actions(program: &HirProgram) -> HashSet<String> {
    program
        .ast
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            Decl::ServerAction(action) => Some(action.name.clone()),
            Decl::Export(inner) => match inner.as_ref() {
                Decl::ServerAction(action) => Some(action.name.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn check_server_actions(
    names: &HashSet<String>,
    program: &HirProgram,
    diagnostics: &mut Diagnostics,
) {
    let mut seen = HashSet::new();
    for decl in &program.ast.declarations {
        let action = match decl {
            Decl::ServerAction(action) => Some(action),
            Decl::Export(inner) => match inner.as_ref() {
                Decl::ServerAction(action) => Some(action),
                _ => None,
            },
            _ => None,
        };
        let Some(action) = action else {
            continue;
        };
        if !seen.insert(action.name.clone()) || !names.contains(&action.name) {
            diagnostics.push(Diagnostic::error(
                "LUME3006",
                format!("duplicate server action `{}`", action.name),
                Some(action.span),
            ));
        }
        for param in &action.params {
            if !serializable_type(&param.ty) {
                diagnostics.push(Diagnostic::error(
                    "LUME2001",
                    format!(
                        "server action `{}` uses non-serializable input `{}`",
                        action.name, param.ty
                    ),
                    Some(param.span),
                ));
            }
        }
        if !serializable_type(&action.return_ty) {
            diagnostics.push(Diagnostic::error(
                "LUME2002",
                format!(
                    "server action `{}` returns non-serializable `{}`",
                    action.name, action.return_ty
                ),
                Some(action.span),
            ));
        }
        if modifier_value(action, "csrf").as_deref() == Some("false") {
            diagnostics.push(Diagnostic::warning(
                "LUME2004",
                format!("server action `{}` disables csrf protection", action.name),
                Some(action.span),
            ));
        }
        if modifier_value(action, "runtime").as_deref() == Some("jit")
            && action.body.statements.iter().any(|stmt| match stmt {
                Stmt::Expr(expr) => expr.raw.contains('.'),
                Stmt::Assign { expr, .. } => expr.raw.contains('.'),
            })
        {
            diagnostics.push(Diagnostic::warning(
                "LUME2005",
                format!(
                    "server action `{}` may need an FFI bridge for jit runtime",
                    action.name
                ),
                Some(action.span),
            ));
        }
    }
}

fn modifier_value(action: &ServerActionDecl, name: &str) -> Option<String> {
    action
        .modifiers
        .iter()
        .find(|modifier| modifier.name == name)
        .and_then(|modifier| modifier.value.as_deref())
        .map(|value| value.trim().trim_matches(['"', '\'']).to_string())
}

fn serializable_type(ty: &str) -> bool {
    let ty = ty.trim().trim_matches(['"', '\'']);
    if ty.ends_with('?') || ty.ends_with("[]") {
        return serializable_type(ty.trim_end_matches('?').trim_end_matches("[]"));
    }
    if let Some(inner) = ty
        .strip_prefix("Array<")
        .and_then(|value| value.strip_suffix('>'))
    {
        return serializable_type(inner);
    }
    if ty.starts_with("Result<") || ty.starts_with("Optional<") || ty.starts_with("Union<") {
        return true;
    }
    matches!(
        ty,
        "Any"
            | "Unknown"
            | "Void"
            | "Null"
            | "Bool"
            | "bool"
            | "String"
            | "str"
            | "Int"
            | "Number"
            | "Float"
            | "DateTime"
            | "URL"
            | "Array"
            | "Object"
            | "File"
            | "FormData"
            | "i64"
            | "i32"
            | "u64"
            | "u32"
            | "f64"
            | "f32"
    ) || ty.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
}

fn check_ffi(program: &HirProgram, diagnostics: &mut Diagnostics) {
    let mut modules = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    let mut opaques = Vec::new();
    for decl in program.ast.declarations.iter().map(export_inner) {
        match decl {
            Decl::FfiModule(decl) => modules.push(decl.clone()),
            Decl::FfiStruct(decl) => structs.push(decl.clone()),
            Decl::FfiEnum(decl) => enums.push(decl.clone()),
            Decl::FfiOpaque(decl) => opaques.push(decl.clone()),
            _ => {}
        }
    }
    if modules.is_empty() && structs.is_empty() && enums.is_empty() && opaques.is_empty() {
        return;
    }
    let registry = lume_ffi::FfiRegistry::from_ast(&modules, &structs, &enums, &opaques);
    for diagnostic in registry.validate().diagnostics {
        let message = match diagnostic.symbol {
            Some(symbol) => format!("{} ({symbol})", diagnostic.message),
            None => diagnostic.message,
        };
        diagnostics.push(match diagnostic.severity {
            lume_ffi::FfiSeverity::Error => {
                Diagnostic::error(diagnostic.code, message, Some(program.ast.span))
            }
            lume_ffi::FfiSeverity::Warning => {
                Diagnostic::warning(diagnostic.code, message, Some(program.ast.span))
            }
        });
    }
}

fn export_inner(decl: &Decl) -> &Decl {
    match decl {
        Decl::Export(inner) => inner,
        _ => decl,
    }
}

fn collect_route_paths(program: &HirProgram) -> HashSet<String> {
    let mut paths = HashSet::new();
    for decl in &program.ast.declarations {
        match decl {
            Decl::Route(route) => collect_route_path(route, None, &mut paths),
            Decl::Export(inner) => {
                if let Decl::Route(route) = inner.as_ref() {
                    collect_route_path(route, None, &mut paths);
                }
            }
            _ => {}
        }
    }
    paths
}

fn collect_route_path(route: &RouteDecl, parent: Option<&str>, paths: &mut HashSet<String>) {
    let path = join_route_path(parent, &route.path);
    paths.insert(path.clone());
    if route.body.index.is_some() {
        paths.insert(path.clone());
    }
    for child in &route.body.children {
        collect_route_path(child, Some(&path), paths);
    }
}

fn join_route_path(parent: Option<&str>, child: &str) -> String {
    let child = child.trim();
    if parent.is_none() || child.starts_with('/') {
        return normalize_route_path(child);
    }
    let parent = parent.unwrap_or("/");
    if child.is_empty() || child == "." {
        return normalize_route_path(parent);
    }
    normalize_route_path(&format!("{}/{}", parent.trim_end_matches('/'), child))
}

fn normalize_route_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".into();
    }
    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.into()
    } else {
        format!("/{without_trailing}")
    }
}

fn route_path_matches(pattern: &str, target: &str) -> bool {
    let pattern = normalize_route_path(pattern);
    let target = normalize_route_path(target);
    if pattern == target {
        return true;
    }
    let pattern_parts = if pattern == "/" {
        Vec::new()
    } else {
        pattern.trim_matches('/').split('/').collect::<Vec<_>>()
    };
    let target_parts = if target == "/" {
        Vec::new()
    } else {
        target.trim_matches('/').split('/').collect::<Vec<_>>()
    };
    let mut target_index = 0usize;
    for part in pattern_parts {
        if part.starts_with('*') {
            return true;
        }
        let Some(target_part) = target_parts.get(target_index) else {
            return false;
        };
        if !part.starts_with(':') && part != *target_part {
            return false;
        }
        target_index += 1;
    }
    target_index == target_parts.len()
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
        "Canvas",
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

    #[test]
    fn warns_when_link_points_to_unknown_route() {
        let source = r#"
route "/" {
  Text("Home")
}

component App {
  view {
    Link("Missing", to="/missing")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME7010"));
    }

    #[test]
    fn accepts_links_to_dynamic_routes_and_known_form_actions() {
        let source = r#"
server action save(message: String): String {
  return message
}

route "/users" {
  route ":id<String>" {
    Text("User")
  }
}

component App {
  view {
    Column {
      Link("User", to="/users/42")
      Form action=save {
        Input(name="message", label="Message")
      }
    }
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(!diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME7010" || diagnostic.code == "LUME6207"));
    }
}
