use lume_ast::*;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_hir::HirProgram;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct LumeProgram {
    pub component: ComponentDecl,
    pub components: Vec<ComponentDecl>,
    pub styles: Vec<StyleDecl>,
    pub themes: Vec<ThemeDecl>,
    pub routes: Vec<IrRoute>,
    pub route_tree: Vec<IrRouteNode>,
    pub server_actions: Vec<ServerActionDecl>,
    pub queries: Vec<QueryDecl>,
    pub ffi_modules: Vec<FfiModuleDecl>,
    pub ffi_structs: Vec<FfiStructDecl>,
    pub ffi_enums: Vec<FfiEnumDecl>,
    pub ffi_opaques: Vec<FfiOpaqueDecl>,
}

#[derive(Clone, Debug)]
pub struct IrRouteNode {
    pub route: IrRoute,
    pub children: Vec<IrRouteNode>,
}

#[derive(Clone, Debug)]
pub struct IrRoute {
    pub id: String,
    pub path: String,
    pub source_path: String,
    pub segments: Vec<RouteSegment>,
    pub params: Vec<RouteParam>,
    pub layout: Option<String>,
    pub guards: Vec<String>,
    pub page: Option<String>,
    pub view: Option<ViewBlock>,
    pub is_index: bool,
    pub matcher_rank: Vec<u8>,
    pub span: lume_span::Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteSegment {
    Static(String),
    Dynamic { name: String, ty: Option<String> },
    CatchAll { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteParam {
    pub name: String,
    pub ty: String,
    pub catch_all: bool,
}

#[derive(Clone, Debug)]
pub struct RouteMatch<'a> {
    pub route: &'a IrRoute,
    pub params: HashMap<String, String>,
}

pub fn build(program: &HirProgram) -> Result<LumeProgram, Diagnostics> {
    build_with_base(program, None)
}

pub fn build_with_base(
    program: &HirProgram,
    base_dir: Option<&Path>,
) -> Result<LumeProgram, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let declarations = collect_declarations(program, base_dir);
    let styles = declarations
        .iter()
        .filter_map(style_decl)
        .cloned()
        .collect::<Vec<_>>();
    let themes = declarations
        .iter()
        .filter_map(theme_decl)
        .cloned()
        .collect::<Vec<_>>();
    let route_decls = declarations
        .iter()
        .filter_map(route_decl)
        .cloned()
        .collect::<Vec<_>>();
    let (route_tree, routes) = lower_routes(&route_decls, &mut diagnostics);
    let server_actions = declarations
        .iter()
        .filter_map(server_action_decl)
        .cloned()
        .collect::<Vec<_>>();
    let queries = declarations
        .iter()
        .filter_map(query_decl)
        .cloned()
        .collect::<Vec<_>>();
    let ffi_modules = declarations
        .iter()
        .filter_map(ffi_module_decl)
        .cloned()
        .collect::<Vec<_>>();
    let ffi_structs = declarations
        .iter()
        .filter_map(ffi_struct_decl)
        .cloned()
        .collect::<Vec<_>>();
    let ffi_enums = declarations
        .iter()
        .filter_map(ffi_enum_decl)
        .cloned()
        .collect::<Vec<_>>();
    let ffi_opaques = declarations
        .iter()
        .filter_map(ffi_opaque_decl)
        .cloned()
        .collect::<Vec<_>>();
    let components = declarations
        .iter()
        .filter_map(component_decl)
        .cloned()
        .collect::<Vec<_>>();

    if diagnostics.has_errors() {
        return Err(diagnostics);
    }
    if let Some(component) = entry_component(program) {
        return Ok(LumeProgram {
            component: component.clone(),
            components,
            styles,
            themes,
            routes,
            route_tree,
            server_actions,
            queries,
            ffi_modules,
            ffi_structs,
            ffi_enums,
            ffi_opaques,
        });
    }
    diagnostics.push(Diagnostic::error(
        "LUME4001",
        "no component or page found to render",
        Some(program.ast.span),
    ));
    Err(diagnostics)
}

fn entry_component(program: &HirProgram) -> Option<&ComponentDecl> {
    let mut first = None;
    for decl in program.ast.declarations.iter().map(export_inner) {
        if let Decl::Component(component) | Decl::Page(component) = decl {
            if component.name == "App" {
                return Some(component);
            }
            if first.is_none() {
                first = Some(component);
            }
        }
    }
    first
}

fn collect_declarations(program: &HirProgram, base_dir: Option<&Path>) -> Vec<Decl> {
    let mut declarations = Vec::new();
    for decl in &program.ast.declarations {
        if let Decl::Import { items, from } = decl {
            declarations.extend(imported_declarations(items, from, base_dir));
        }
    }
    declarations.extend(program.ast.declarations.iter().map(export_inner).cloned());
    declarations
}

fn imported_declarations(items: &[String], from: &str, base_dir: Option<&Path>) -> Vec<Decl> {
    let Some(base_dir) = base_dir else {
        return Vec::new();
    };
    if !from.starts_with('.') {
        return Vec::new();
    }
    let Ok(source) = fs::read_to_string(base_dir.join(from)) else {
        return Vec::new();
    };
    let (program, diagnostics) = lume_parser::parse(&source);
    if diagnostics.has_errors() {
        return Vec::new();
    }
    let requested = items.iter().cloned().collect::<HashSet<_>>();
    program
        .declarations
        .iter()
        .filter_map(|decl| exported_decl_named(decl, &requested))
        .cloned()
        .collect()
}

fn exported_decl_named<'a>(decl: &'a Decl, requested: &HashSet<String>) -> Option<&'a Decl> {
    let Decl::Export(inner) = decl else {
        return None;
    };
    let inner = inner.as_ref();
    exported_name(inner)
        .is_some_and(|name| requested.contains(name))
        .then_some(inner)
}

fn export_inner(decl: &Decl) -> &Decl {
    match decl {
        Decl::Export(inner) => inner,
        _ => decl,
    }
}

fn exported_name(decl: &Decl) -> Option<&str> {
    match decl {
        Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
            Some(&component.name)
        }
        Decl::Style(style) => Some(&style.name),
        Decl::Theme(theme) => Some(&theme.name),
        Decl::Type(decl)
        | Decl::App(decl)
        | Decl::Form(decl)
        | Decl::Ffi(decl)
        | Decl::Reserved(decl) => decl.name.as_deref(),
        Decl::Query(decl) => Some(&decl.name),
        Decl::FfiModule(decl) => Some(&decl.name),
        Decl::FfiStruct(decl) => Some(&decl.name),
        Decl::FfiEnum(decl) => Some(&decl.name),
        Decl::FfiOpaque(decl) => Some(&decl.name),
        Decl::ServerAction(decl) => Some(&decl.name),
        _ => None,
    }
}

fn component_decl(decl: &Decl) -> Option<&ComponentDecl> {
    match decl {
        Decl::Component(component) | Decl::Page(component) | Decl::Layout(component) => {
            Some(component)
        }
        _ => None,
    }
}

fn style_decl(decl: &Decl) -> Option<&StyleDecl> {
    match decl {
        Decl::Style(style) => Some(style),
        _ => None,
    }
}

fn theme_decl(decl: &Decl) -> Option<&ThemeDecl> {
    match decl {
        Decl::Theme(theme) => Some(theme),
        _ => None,
    }
}

fn route_decl(decl: &Decl) -> Option<&RouteDecl> {
    match decl {
        Decl::Route(route) => Some(route),
        _ => None,
    }
}

fn server_action_decl(decl: &Decl) -> Option<&ServerActionDecl> {
    match decl {
        Decl::ServerAction(action) => Some(action),
        _ => None,
    }
}

fn query_decl(decl: &Decl) -> Option<&QueryDecl> {
    match decl {
        Decl::Query(query) => Some(query),
        _ => None,
    }
}

fn ffi_module_decl(decl: &Decl) -> Option<&FfiModuleDecl> {
    match decl {
        Decl::FfiModule(module) => Some(module),
        _ => None,
    }
}

fn ffi_struct_decl(decl: &Decl) -> Option<&FfiStructDecl> {
    match decl {
        Decl::FfiStruct(item) => Some(item),
        _ => None,
    }
}

fn ffi_enum_decl(decl: &Decl) -> Option<&FfiEnumDecl> {
    match decl {
        Decl::FfiEnum(item) => Some(item),
        _ => None,
    }
}

fn ffi_opaque_decl(decl: &Decl) -> Option<&FfiOpaqueDecl> {
    match decl {
        Decl::FfiOpaque(item) => Some(item),
        _ => None,
    }
}

fn lower_routes(
    route_decls: &[RouteDecl],
    diagnostics: &mut Diagnostics,
) -> (Vec<IrRouteNode>, Vec<IrRoute>) {
    let mut seen = HashSet::new();
    let mut flat = Vec::new();
    let mut tree = Vec::new();
    for decl in route_decls {
        let node = lower_route_node(
            decl,
            None,
            None,
            &[],
            false,
            diagnostics,
            &mut seen,
            &mut flat,
        );
        tree.push(node);
    }
    flat.sort_by(compare_routes);
    (tree, flat)
}

fn lower_route_node(
    decl: &RouteDecl,
    parent_path: Option<&str>,
    inherited_layout: Option<String>,
    inherited_guards: &[String],
    nested: bool,
    diagnostics: &mut Diagnostics,
    seen: &mut HashSet<String>,
    flat: &mut Vec<IrRoute>,
) -> IrRouteNode {
    if nested && decl.path.starts_with('/') {
        diagnostics.push(Diagnostic::error(
            "LUME7008",
            "nested route path must be relative",
            Some(decl.span),
        ));
    }

    let full_path = join_route_path(parent_path, &decl.path);
    let layout = route_layout(decl).or(inherited_layout);
    let mut guards = inherited_guards.to_vec();
    guards.extend(route_guards(decl));
    let view = (!decl.body.view_nodes.is_empty()).then_some(ViewBlock {
        nodes: decl.body.view_nodes.clone(),
        span: decl.body.span,
    });
    let mut route = make_ir_route(
        decl,
        full_path.clone(),
        layout.clone(),
        guards.clone(),
        view.as_ref(),
        false,
        diagnostics,
    );
    if !seen.insert(route.path.clone()) {
        diagnostics.push(Diagnostic::error(
            "LUME7001",
            format!("duplicate route pattern `{}`", route.path),
            Some(decl.span),
        ));
    }
    flat.push(route.clone());

    let mut children = Vec::new();
    if let Some(index) = &decl.body.index {
        let index_route = make_ir_route(
            decl,
            full_path.clone(),
            layout.clone(),
            guards.clone(),
            Some(index),
            true,
            diagnostics,
        );
        children.push(IrRouteNode {
            route: index_route.clone(),
            children: Vec::new(),
        });
        flat.push(index_route);
    }
    for child in &decl.body.children {
        children.push(lower_route_node(
            child,
            Some(&full_path),
            layout.clone(),
            &guards,
            true,
            diagnostics,
            seen,
            flat,
        ));
    }
    route.page = page_from_view(view.as_ref());
    IrRouteNode { route, children }
}

fn make_ir_route(
    decl: &RouteDecl,
    path: String,
    layout: Option<String>,
    guards: Vec<String>,
    view: Option<&ViewBlock>,
    is_index: bool,
    diagnostics: &mut Diagnostics,
) -> IrRoute {
    let segments = parse_route_segments(&path, decl.span, diagnostics);
    let params = route_params(&segments);
    IrRoute {
        id: route_id(&path, is_index),
        path: path.clone(),
        source_path: decl.path.clone(),
        matcher_rank: segments.iter().map(segment_rank).collect(),
        segments,
        params,
        layout,
        guards,
        page: page_from_view(view),
        view: view.cloned(),
        is_index,
        span: decl.span,
    }
}

fn route_layout(decl: &RouteDecl) -> Option<String> {
    decl.attrs.iter().find_map(|attr| match attr {
        RouteAttr::Layout { name, .. } => Some(name.clone()),
        _ => None,
    })
}

fn route_guards(decl: &RouteDecl) -> Vec<String> {
    decl.attrs
        .iter()
        .filter_map(|attr| match attr {
            RouteAttr::Guard { expr, .. } => Some(expr.raw.clone()),
            _ => None,
        })
        .filter(|guard| !guard.is_empty())
        .collect()
}

fn page_from_view(view: Option<&ViewBlock>) -> Option<String> {
    view.and_then(|view| {
        view.nodes.iter().find_map(|node| match node {
            ViewNode::Element(element) => Some(element.name.clone()),
            _ => None,
        })
    })
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

fn parse_route_segments(
    path: &str,
    span: lume_span::Span,
    diagnostics: &mut Diagnostics,
) -> Vec<RouteSegment> {
    let mut segments = Vec::new();
    if path == "/" {
        return segments;
    }
    for (index, raw) in path.trim_matches('/').split('/').enumerate() {
        if let Some(rest) = raw.strip_prefix(':') {
            let (name, ty) = parse_dynamic_segment(rest);
            if name.is_empty() {
                diagnostics.push(Diagnostic::error(
                    "LUME1009",
                    format!("invalid route pattern `{path}`"),
                    Some(span),
                ));
            }
            segments.push(RouteSegment::Dynamic { name, ty });
        } else if let Some(name) = raw.strip_prefix('*') {
            if index + 1 != path.trim_matches('/').split('/').count() {
                diagnostics.push(Diagnostic::error(
                    "LUME1009",
                    "catch-all route segment must be last",
                    Some(span),
                ));
            }
            segments.push(RouteSegment::CatchAll { name: name.into() });
        } else {
            segments.push(RouteSegment::Static(raw.into()));
        }
    }
    segments
}

fn parse_dynamic_segment(raw: &str) -> (String, Option<String>) {
    let Some(start) = raw.find('<') else {
        return (raw.into(), None);
    };
    let end = raw.rfind('>').unwrap_or(raw.len());
    (raw[..start].into(), Some(raw[start + 1..end].into()))
}

fn route_params(segments: &[RouteSegment]) -> Vec<RouteParam> {
    segments
        .iter()
        .filter_map(|segment| match segment {
            RouteSegment::Dynamic { name, ty } => Some(RouteParam {
                name: name.clone(),
                ty: ty.clone().unwrap_or_else(|| "String".into()),
                catch_all: false,
            }),
            RouteSegment::CatchAll { name } => Some(RouteParam {
                name: name.clone(),
                ty: "String[]".into(),
                catch_all: true,
            }),
            RouteSegment::Static(_) => None,
        })
        .collect()
}

fn segment_rank(segment: &RouteSegment) -> u8 {
    match segment {
        RouteSegment::Static(_) => 0,
        RouteSegment::Dynamic { ty: Some(_), .. } => 1,
        RouteSegment::Dynamic { ty: None, .. } => 2,
        RouteSegment::CatchAll { .. } => 3,
    }
}

fn compare_routes(left: &IrRoute, right: &IrRoute) -> std::cmp::Ordering {
    left.matcher_rank
        .cmp(&right.matcher_rank)
        .then_with(|| right.segments.len().cmp(&left.segments.len()))
        .then_with(|| right.page.is_some().cmp(&left.page.is_some()))
        .then_with(|| left.path.cmp(&right.path))
}

fn route_id(path: &str, is_index: bool) -> String {
    let mut id = path
        .trim_matches('/')
        .replace(['/', ':', '*', '<', '>'], "_")
        .replace("__", "_");
    if id.is_empty() {
        id = "root".into();
    }
    if is_index {
        format!("{id}_index")
    } else {
        id
    }
}

pub fn match_route<'a>(routes: &'a [IrRoute], path: &str) -> Option<RouteMatch<'a>> {
    let request = normalize_route_path(path);
    let request_segments = if request == "/" {
        Vec::new()
    } else {
        request.trim_matches('/').split('/').collect::<Vec<_>>()
    };
    let mut sorted = routes.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| compare_routes(left, right));
    sorted.into_iter().find_map(|route| {
        match_segments(route, &request_segments).map(|params| RouteMatch { route, params })
    })
}

fn match_segments(route: &IrRoute, request_segments: &[&str]) -> Option<HashMap<String, String>> {
    let mut params = HashMap::new();
    let mut index = 0usize;
    for segment in &route.segments {
        match segment {
            RouteSegment::Static(value) => {
                if request_segments.get(index).copied() != Some(value.as_str()) {
                    return None;
                }
                index += 1;
            }
            RouteSegment::Dynamic { name, ty } => {
                let value = request_segments.get(index)?;
                if !matches_route_type(value, ty.as_deref()) {
                    return None;
                }
                params.insert(name.clone(), (*value).into());
                index += 1;
            }
            RouteSegment::CatchAll { name } => {
                params.insert(name.clone(), request_segments[index..].join("/"));
                index = request_segments.len();
                break;
            }
        }
    }
    (index == request_segments.len()).then_some(params)
}

fn matches_route_type(value: &str, ty: Option<&str>) -> bool {
    match ty.unwrap_or("String") {
        "i32" | "i64" | "u32" | "u64" | "Int" => value.parse::<i64>().is_ok(),
        "String" => true,
        _ => true,
    }
}

impl LumeProgram {
    pub fn states(&self) -> impl Iterator<Item = &StateDecl> {
        self.component.items.iter().filter_map(|item| match item {
            ComponentItem::State(state) => Some(state),
            _ => None,
        })
    }

    pub fn view(&self) -> Option<&ViewBlock> {
        self.component.items.iter().find_map(|item| match item {
            ComponentItem::View(view) => Some(view),
            _ => None,
        })
    }

    pub fn component_named(&self, name: &str) -> Option<&ComponentDecl> {
        self.components
            .iter()
            .find(|component| component.name == name)
            .filter(|component| component.name != self.component.name)
    }

    pub fn expand_component_view(
        &self,
        component: &ComponentDecl,
        call: &ElementNode,
    ) -> Option<ViewBlock> {
        let view = component.items.iter().find_map(|item| match item {
            ComponentItem::View(view) => Some(view),
            _ => None,
        })?;
        let props = component_props(component, call);
        let slots = component_slots(call);
        Some(substitute_view(view, &props, &slots))
    }
}

fn component_props(component: &ComponentDecl, call: &ElementNode) -> HashMap<String, Expr> {
    let mut props = HashMap::new();
    let mut positional = call.args.iter().filter_map(|arg| match arg {
        Arg::Positional(expr) => Some(expr),
        Arg::Named(_, _) => None,
    });
    for param in &component.params {
        let value = call
            .args
            .iter()
            .find_map(|arg| match arg {
                Arg::Named(name, expr) if name == &param.name => Some(expr),
                _ => None,
            })
            .or_else(|| positional.next())
            .or(param.default.as_ref());
        if let Some(value) = value {
            props.insert(param.name.clone(), value.clone());
        }
    }
    props
}

fn component_slots(call: &ElementNode) -> HashMap<Option<String>, ViewBlock> {
    let mut slots = HashMap::new();
    let Some(children) = &call.children else {
        return slots;
    };
    let mut default_nodes = Vec::new();
    for node in &children.nodes {
        match node {
            ViewNode::SlotFill { name, body, .. } => {
                slots.insert(Some(name.clone()), body.clone());
            }
            _ => default_nodes.push(node.clone()),
        }
    }
    if !default_nodes.is_empty() {
        slots.insert(
            None,
            ViewBlock {
                nodes: default_nodes,
                span: children.span,
            },
        );
    }
    slots
}

fn substitute_view(
    view: &ViewBlock,
    props: &HashMap<String, Expr>,
    slots: &HashMap<Option<String>, ViewBlock>,
) -> ViewBlock {
    let mut nodes = Vec::new();
    for node in &view.nodes {
        match substitute_node(node, props, slots) {
            SubstitutedNode::One(node) => nodes.push(node),
            SubstitutedNode::Many(extra) => nodes.extend(extra),
            SubstitutedNode::None => {}
        }
    }
    ViewBlock {
        nodes,
        span: view.span,
    }
}

enum SubstitutedNode {
    One(ViewNode),
    Many(Vec<ViewNode>),
    None,
}

fn substitute_node(
    node: &ViewNode,
    props: &HashMap<String, Expr>,
    slots: &HashMap<Option<String>, ViewBlock>,
) -> SubstitutedNode {
    match node {
        ViewNode::Element(element) => {
            SubstitutedNode::One(ViewNode::Element(substitute_element(element, props, slots)))
        }
        ViewNode::If(node) => SubstitutedNode::One(ViewNode::If(IfNode {
            condition: substitute_expr(&node.condition, props),
            then_block: substitute_view(&node.then_block, props, slots),
            else_block: node
                .else_block
                .as_ref()
                .map(|view| substitute_view(view, props, slots)),
            span: node.span,
        })),
        ViewNode::For(node) => SubstitutedNode::One(ViewNode::For(ForNode {
            item: node.item.clone(),
            index: node.index.clone(),
            iterable: substitute_expr(&node.iterable, props),
            key: node.key.as_ref().map(|expr| substitute_expr(expr, props)),
            body: substitute_view(&node.body, props, slots),
            span: node.span,
        })),
        ViewNode::SlotUse { name, .. } => slots
            .get(name)
            .or_else(|| slots.get(&None))
            .map(|view| SubstitutedNode::Many(view.nodes.clone()))
            .unwrap_or(SubstitutedNode::None),
        ViewNode::SlotFill { .. } => SubstitutedNode::None,
        ViewNode::Event(event) => SubstitutedNode::One(ViewNode::Event(EventNode {
            event: event.event.clone(),
            params: event.params.clone(),
            body: substitute_block(&event.body, props),
            span: event.span,
        })),
        ViewNode::Text(text) => SubstitutedNode::One(ViewNode::Text(TextNode {
            value: substitute_expr(&text.value, props),
            span: text.span,
        })),
        ViewNode::Match(decl) => SubstitutedNode::One(ViewNode::Match(decl.clone())),
    }
}

fn substitute_element(
    element: &ElementNode,
    props: &HashMap<String, Expr>,
    slots: &HashMap<Option<String>, ViewBlock>,
) -> ElementNode {
    ElementNode {
        name: element.name.clone(),
        args: element
            .args
            .iter()
            .map(|arg| match arg {
                Arg::Positional(expr) => Arg::Positional(substitute_expr(expr, props)),
                Arg::Named(name, expr) => Arg::Named(name.clone(), substitute_expr(expr, props)),
            })
            .collect(),
        attrs: element
            .attrs
            .iter()
            .map(|attr| Attribute {
                name: attr.name.clone(),
                value: attr.value.as_ref().map(|expr| substitute_expr(expr, props)),
                span: attr.span,
            })
            .collect(),
        children: element
            .children
            .as_ref()
            .map(|view| substitute_view(view, props, slots)),
        span: element.span,
    }
}

fn substitute_block(block: &Block, props: &HashMap<String, Expr>) -> Block {
    Block {
        statements: block
            .statements
            .iter()
            .map(|stmt| match stmt {
                Stmt::Assign {
                    target,
                    op,
                    expr,
                    span,
                } => Stmt::Assign {
                    target: target.clone(),
                    op: op.clone(),
                    expr: substitute_expr(expr, props),
                    span: *span,
                },
                Stmt::Expr(expr) => Stmt::Expr(substitute_expr(expr, props)),
            })
            .collect(),
        span: block.span,
    }
}

fn substitute_expr(expr: &Expr, props: &HashMap<String, Expr>) -> Expr {
    let raw = expr.raw.trim();
    if let Some(value) = props.get(raw) {
        return value.clone();
    }
    if is_string_literal(raw) {
        return Expr {
            raw: substitute_template(raw, props),
            span: expr.span,
        };
    }
    Expr {
        raw: substitute_expr_identifiers(raw, props),
        span: expr.span,
    }
}

fn substitute_expr_identifiers(raw: &str, props: &HashMap<String, Expr>) -> String {
    let mut out = String::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' || ch == '\'' {
            let quote = ch;
            out.push(ch);
            let mut escaped = false;
            for (_, inner) in chars.by_ref() {
                out.push(inner);
                if escaped {
                    escaped = false;
                } else if inner == '\\' {
                    escaped = true;
                } else if inner == quote {
                    break;
                }
            }
            continue;
        }

        if is_identifier_start(ch) {
            let start = idx;
            let mut end = idx + ch.len_utf8();
            while let Some((next_idx, next)) = chars.peek().copied() {
                if is_identifier_continue(next) {
                    chars.next();
                    end = next_idx + next.len_utf8();
                } else {
                    break;
                }
            }
            let ident = &raw[start..end];
            if let Some(value) = props.get(ident) {
                out.push('(');
                out.push_str(value.raw.trim());
                out.push(')');
            } else {
                out.push_str(ident);
            }
            continue;
        }

        out.push(ch);
    }
    out
}

fn substitute_template(raw: &str, props: &HashMap<String, Expr>) -> String {
    let quote = &raw[..1];
    let mut out = String::new();
    let mut rest = &raw[1..raw.len().saturating_sub(1)];
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(after);
            return format!("{quote}{out}{quote}");
        };
        let name = after[..end].trim();
        if let Some(value) = props.get(name) {
            if is_string_literal(value.raw.trim()) {
                out.push_str(value.raw.trim().trim_matches(['"', '\'']));
            } else {
                out.push('{');
                out.push_str(value.raw.trim());
                out.push('}');
            }
        } else {
            out.push('{');
            out.push_str(name);
            out.push('}');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    format!("{quote}{out}{quote}")
}

fn is_string_literal(raw: &str) -> bool {
    (raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\''))
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::{build_with_base, match_route};
    use lume_ast::ViewNode;
    use lume_hir::lower;
    use lume_parser::parse;
    use std::fs;

    #[test]
    fn pulls_exported_local_declarations_into_ir() {
        let temp = std::env::temp_dir().join("lume-ir-local-import-test");
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("Card.lume"),
            r#"export style card {
  padding: 12
}

export component Card {
  view {
    Box style=card {
      Text("Card")
    }
  }
}
"#,
        )
        .unwrap();
        let source = r#"
import { Card, card } from "./Card.lume"

component App {
  view {
    Card()
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build_with_base(&lower(program), Some(&temp)).expect("ir");
        assert!(ir.component_named("Card").is_some());
        assert!(ir.styles.iter().any(|style| style.name == "card"));
    }

    #[test]
    fn collects_server_actions() {
        let source = r#"
server action add(amount: i64): i64 {
  return amount
}

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build_with_base(&lower(program), None).expect("ir");
        assert_eq!(ir.server_actions.len(), 1);
        assert_eq!(ir.server_actions[0].name, "add");
    }

    #[test]
    fn expands_component_props_and_slots() {
        let source = r#"
component Card(title: String = "Untitled") {
  view {
    Box {
      Text("Title: {title}")
      slot
    }
  }
}

component App {
  view {
    Card(title="Hello") {
      Text("Body")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build_with_base(&lower(program), None).expect("ir");
        let app_view = ir.view().expect("view");
        let ViewNode::Element(call) = &app_view.nodes[0] else {
            panic!("expected call");
        };
        let expanded = ir
            .expand_component_view(ir.component_named("Card").expect("card"), call)
            .expect("expanded");
        let debug = format!("{expanded:?}");
        assert!(debug.contains("Title: Hello"));
        assert!(debug.contains("Body"));
    }

    #[test]
    fn lowers_nested_routes_and_matches_by_priority() {
        let source = r#"
layout AppLayout {
  view {
    Outlet()
  }
}

route "/users" layout=AppLayout guard=requireLogin {
  index {
    UsersIndex()
  }

  route "new" {
    NewUserPage()
  }

  route ":id<i64>" {
    UserPage(id=params.id)
  }

  route ":slug" {
    UserSlugPage(slug=params.slug)
  }

  route "*path" {
    UsersCatchAll(path=params.path)
  }
}

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build_with_base(&lower(program), None).expect("ir");
        assert_eq!(ir.route_tree.len(), 1);
        assert_eq!(ir.route_tree[0].children.len(), 5);

        let new_match = match_route(&ir.routes, "/users/new").expect("new match");
        assert_eq!(new_match.route.page.as_deref(), Some("NewUserPage"));

        let id_match = match_route(&ir.routes, "/users/42").expect("id match");
        assert_eq!(id_match.route.page.as_deref(), Some("UserPage"));
        assert_eq!(id_match.params.get("id").map(String::as_str), Some("42"));

        let slug_match = match_route(&ir.routes, "/users/alice").expect("slug match");
        assert_eq!(slug_match.route.page.as_deref(), Some("UserSlugPage"));

        let catch_all = match_route(&ir.routes, "/users/a/b").expect("catch-all match");
        assert_eq!(catch_all.route.page.as_deref(), Some("UsersCatchAll"));
        assert_eq!(
            catch_all.params.get("path").map(String::as_str),
            Some("a/b")
        );
        assert_eq!(
            catch_all.route.layout.as_deref(),
            Some("AppLayout"),
            "layout should be inherited"
        );
        assert_eq!(catch_all.route.guards, vec!["requireLogin"]);
    }
}
