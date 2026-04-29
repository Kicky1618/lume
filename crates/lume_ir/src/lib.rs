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
    pub routes: Vec<RouteDecl>,
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
    let routes = declarations
        .iter()
        .filter_map(route_decl)
        .cloned()
        .collect::<Vec<_>>();
    let components = declarations
        .iter()
        .filter_map(component_decl)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(component) = entry_component(program) {
        return Ok(LumeProgram {
            component: component.clone(),
            components,
            styles,
            themes,
            routes,
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
        | Decl::ServerAction(decl)
        | Decl::Form(decl)
        | Decl::Ffi(decl)
        | Decl::Reserved(decl) => decl.name.as_deref(),
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
    expr.clone()
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

#[cfg(test)]
mod tests {
    use super::build_with_base;
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
}
