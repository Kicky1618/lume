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
    let mut actions = HashSet::new();
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
                if !actions.insert(action.name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        "LUME3007",
                        format!("duplicate action `{}`", action.name),
                        Some(action.span),
                    ));
                }
                check_client_action(action, diagnostics);
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

fn check_client_action(action: &ActionDecl, diagnostics: &mut Diagnostics) {
    if let Some(mode) = action.concurrency.as_deref() {
        if !matches!(mode, "enqueue" | "drop" | "restart") {
            diagnostics.push(Diagnostic::error(
                "LUME3010",
                format!(
                    "action `{}` has unsupported concurrency mode `{mode}`",
                    action.name
                ),
                Some(action.span),
            ));
        } else if !action.is_async {
            diagnostics.push(Diagnostic::warning(
                "LUME3011",
                format!(
                    "action `{}` declares concurrency but is not async",
                    action.name
                ),
                Some(action.span),
            ));
        }
    }
    if let Some(return_ty) = action.return_ty.as_deref() {
        let return_ty = return_ty.trim();
        if !return_ty.is_empty() && !serializable_type(return_ty) {
            diagnostics.push(Diagnostic::warning(
                "LUME3012",
                format!(
                    "action `{}` returns unknown type `{return_ty}`; treating it as opaque",
                    action.name
                ),
                Some(action.span),
            ));
        }
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
    if matches!(element.name.as_str(), "Input" | "TextArea")
        && !has_attr(element, "label")
        && !has_attr(element, "aria-label")
    {
        diagnostics.push(Diagnostic::error(
            "LUME6202",
            format!("{} requires label or aria-label", element.name),
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
    if element.name == "Script" {
        check_script(element, diagnostics);
    }
    if matches!(
        element.name.as_str(),
        "Canvas" | "ImageCanvas" | "NativeCanvas" | "GpuCanvas"
    ) {
        check_canvas(element, diagnostics);
    }
    if let Some(children) = &element.children {
        check_view(children, states, known_components, ctx, diagnostics);
    }
}

fn check_script(element: &ElementNode, diagnostics: &mut Diagnostics) {
    if let Some(src) = attr_literal(element, "src") {
        if src.starts_with("javascript:") || src.starts_with("data:") {
            diagnostics.push(Diagnostic::error(
                "LUME6209",
                "Script src uses a disallowed URL scheme",
                Some(element.span),
            ));
        }
    } else {
        diagnostics.push(Diagnostic::error(
            "LUME6208",
            "Script requires a `src` attribute; inline script bodies are not allowed",
            Some(element.span),
        ));
    }
    if element.children.is_some() {
        diagnostics.push(Diagnostic::error(
            "LUME6010",
            "raw JavaScript embedding is not allowed",
            Some(element.span),
        ));
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
            "Canvas cannot use nativeModule/nativeSymbol/nativeArgs; use ImageCanvas(renderer=..., args={...}) or NativeCanvas(renderer=..., args={...})",
            Some(element.span),
        ));
    }
    if matches!(element.name.as_str(), "ImageCanvas" | "NativeCanvas") {
        match attr_literal(element, "renderer") {
            Some(renderer) if renderer.contains('.') => {}
            _ => diagnostics.push(Diagnostic::error(
                "LUME5202",
                format!(
                    "{} renderer must be a module function such as renderkit.mandelbrot_render",
                    element.name
                ),
                Some(element.span),
            )),
        }
        if let Some(args) = attr_raw(element, "args") {
            let raw = args.trim();
            if !(raw.starts_with('{') && raw.ends_with('}')) {
                diagnostics.push(Diagnostic::error(
                    "LUME5203",
                    format!("{} args must be a structured object", element.name),
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
            if !serializable_input_type(&param.ty) {
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
        if !serializable_output_type(&action.return_ty) {
            diagnostics.push(Diagnostic::error(
                "LUME2002",
                format!(
                    "server action `{}` returns non-serializable `{}`",
                    action.name, action.return_ty
                ),
                Some(action.span),
            ));
        }
        if let Some(max_body_size) = modifier_value(action, "maxBodySize") {
            if parse_byte_size(&max_body_size).is_none() {
                diagnostics.push(Diagnostic::error(
                    "LUME2008",
                    format!(
                        "server action `{}` has invalid maxBodySize `{max_body_size}`",
                        action.name
                    ),
                    Some(action.span),
                ));
            }
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
    serializable_type_with_options(ty, false)
}

fn serializable_input_type(ty: &str) -> bool {
    serializable_type_with_options(ty, false)
}

fn serializable_output_type(ty: &str) -> bool {
    serializable_type_with_options(ty, true)
}

fn serializable_type_with_options(ty: &str, allow_stream: bool) -> bool {
    let ty = ty.trim().trim_matches(['"', '\'']);
    if ty.ends_with('?') || ty.ends_with("[]") {
        return serializable_type_with_options(
            ty.trim_end_matches('?').trim_end_matches("[]"),
            allow_stream,
        );
    }
    if let Some(inner) = ty
        .strip_prefix("Array<")
        .and_then(|value| value.strip_suffix('>'))
    {
        return serializable_type_with_options(inner, allow_stream);
    }
    if let Some(inner) = generic_inner(ty, "Optional") {
        return serializable_type_with_options(inner, allow_stream);
    }
    if let Some(inner) = generic_inner(ty, "Stream") {
        return allow_stream && serializable_type_with_options(inner, false);
    }
    if let Some(inner) = generic_inner(ty, "Result") {
        let args = split_generic_args(inner);
        return args.len() == 2
            && args
                .iter()
                .all(|arg| serializable_type_with_options(arg, allow_stream));
    }
    if let Some(inner) = generic_inner(ty, "Union") {
        let args = split_generic_args(inner);
        return !args.is_empty()
            && args
                .iter()
                .all(|arg| serializable_type_with_options(arg, allow_stream));
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
            | "Bytes"
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

fn generic_inner<'a>(ty: &'a str, name: &str) -> Option<&'a str> {
    ty.strip_prefix(name)?
        .strip_prefix('<')?
        .strip_suffix('>')
        .map(str::trim)
}

fn split_generic_args(raw: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(raw[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    args.push(raw[start..].trim());
    args.into_iter().filter(|arg| !arg.is_empty()).collect()
}

fn parse_byte_size(value: &str) -> Option<usize> {
    let compact = value
        .trim()
        .trim_matches(['"', '\''])
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '_')
        .collect::<String>();
    if compact.is_empty() {
        return None;
    }
    let split = compact
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(compact.len());
    let number = compact[..split].parse::<usize>().ok()?;
    let unit = compact[split..].to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1,
        "KB" | "KIB" => 1024,
        "MB" | "MIB" => 1024 * 1024,
        "GB" | "GIB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
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

    #[test]
    fn validates_client_action_concurrency() {
        let source = r#"
component App {
  state term: String = ""

  async action search(value: String): String concurrency=restart {
    return value
  }

  async action bad concurrency=parallel {
    term = "bad"
  }

  view {
    Text("ok")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3010"));
        assert!(!diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3012"));
    }

    #[test]
    fn validates_server_action_result_stream_file_and_body_size_types() {
        let source = r#"
server action upload(file: File): Result<URL, ActionError> maxBodySize 5MB {
  return { ok: true, value: file.name }
}

server action chunks(prompt: String): Stream<String> {
  return ["a", prompt]
}

server action bad(input: Stream<String>): String maxBodySize lots {
  return "bad"
}

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, parse_diags) = parse(source);
        assert!(!parse_diags.has_errors());
        let diagnostics = check(&lower(program));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME2001"));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME2008"));
        assert!(!diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME2002"));
    }

    #[test]
    fn accepts_external_script_and_rejects_inline_or_unsafe_script() {
        let source = r#"
component App {
  view {
    Column {
      Script(src="/assets/widget.js")
      Script(src="javascript:alert(1)")
      Script {
        Text("not inline js")
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
            .any(|diagnostic| diagnostic.code == "LUME6209"));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6208"));
        assert!(diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME6010"));
        assert!(!diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.code == "LUME3004"));
    }
}
