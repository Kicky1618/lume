use lume_ast::*;
use lume_codegen_css::{layout_class, style_class_for, style_ref_class};
use lume_ir::LumeProgram;

#[derive(Clone, Debug)]
pub struct HtmlOutput {
    pub html: String,
    pub bindings: Vec<Binding>,
    pub events: Vec<EventBinding>,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub node_id: String,
    pub template: String,
    pub deps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EventBinding {
    pub id: usize,
    pub node_id: String,
    pub event: String,
    pub params: Vec<String>,
    pub loop_params: Vec<String>,
    pub statements: Vec<Stmt>,
}

pub fn generate(program: &LumeProgram) -> HtmlOutput {
    let mut ctx = Ctx {
        next_node: 1,
        next_event: 0,
        bindings: Vec::new(),
        events: Vec::new(),
        loop_params: Vec::new(),
    };
    let body = program
        .view()
        .map(|view| render_view(view, &mut ctx))
        .unwrap_or_default();
    let html = format!(
        "<!doctype html>\n<html lang=\"ja\">\n  <head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title>Lume App</title>\n    <link rel=\"stylesheet\" href=\"/assets/style.css\">\n    <script type=\"module\" src=\"/assets/app.js\"></script>\n  </head>\n  <body>\n    <div id=\"lume-root\" data-lume-component=\"{}\" data-lume-id=\"c0\">\n{}    </div>\n  </body>\n</html>\n",
        escape_attr(&program.component.name),
        indent(&body, 6)
    );
    HtmlOutput {
        html,
        bindings: ctx.bindings,
        events: ctx.events,
    }
}

struct Ctx {
    next_node: usize,
    next_event: usize,
    bindings: Vec<Binding>,
    events: Vec<EventBinding>,
    loop_params: Vec<String>,
}

fn render_view(view: &ViewBlock, ctx: &mut Ctx) -> String {
    view.nodes
        .iter()
        .map(|node| render_node(node, ctx))
        .collect::<Vec<_>>()
        .join("")
}

fn render_node(node: &ViewNode, ctx: &mut Ctx) -> String {
    match node {
        ViewNode::Element(element) => render_element(element, ctx),
        ViewNode::Text(text) => render_text_expr(&text.value, ctx),
        ViewNode::If(node) => {
            let then_html = render_view(&node.then_block, ctx);
            let else_html = node
                .else_block
                .as_ref()
                .map(|block| render_view(block, ctx))
                .unwrap_or_default();
            format!(
                "<!-- lume-if:{} -->{}{}\n",
                escape_attr(&node.condition.raw),
                then_html,
                else_html
            )
        }
        ViewNode::For(node) => {
            ctx.loop_params.push(node.item.clone());
            let pushed_index = node.index.clone();
            if let Some(index) = &pushed_index {
                ctx.loop_params.push(index.clone());
            }
            let html = render_view(&node.body, ctx);
            if pushed_index.is_some() {
                ctx.loop_params.pop();
            }
            ctx.loop_params.pop();
            format!(
                "<!-- lume-for:{} in {} -->{}",
                escape_attr(&node.item),
                escape_attr(&node.iterable.raw),
                html
            )
        }
        ViewNode::SlotUse { .. }
        | ViewNode::SlotFill { .. }
        | ViewNode::Match(_)
        | ViewNode::Event(_) => String::new(),
    }
}

fn render_element(element: &ElementNode, ctx: &mut Ctx) -> String {
    match element.name.as_str() {
        "Text" => {
            let expr = first_arg(element).cloned().unwrap_or(Expr {
                raw: "\"\"".into(),
                span: element.span,
            });
            render_text_expr(&expr, ctx)
        }
        "Button" => render_button(element, ctx),
        "Input" => render_input(element, ctx),
        "Image" => render_image(element, ctx),
        "Row" | "Column" | "Box" | "Grid" | "Stack" => render_container(element, ctx),
        _ => render_container(element, ctx),
    }
}

fn render_text_expr(expr: &Expr, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let template = expr.raw.trim().trim_matches('"').to_string();
    let deps = interpolation_deps(&template);
    if !deps.is_empty() {
        ctx.bindings.push(Binding {
            node_id: id.clone(),
            template: template.clone(),
            deps,
        });
    }
    format!(
        "<span data-lume-id=\"{}\"{}>{}</span>\n",
        id,
        bind_attr(&template),
        escape_html(&render_template_initial(&template))
    )
}

fn render_button(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let label = first_arg(element)
        .map(|e| e.raw.trim().trim_matches('"').to_string())
        .unwrap_or_default();
    let event_attr = event_attr(element, ctx, &id);
    format!(
        "<button data-lume-id=\"{}\"{}>{}</button>\n",
        id,
        event_attr,
        escape_html(&label)
    )
}

fn render_input(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let value = attr_value(element, "value")
        .map(|e| format!(" value=\"{}\"", escape_attr(&e.raw)))
        .unwrap_or_default();
    let placeholder = attr_value(element, "placeholder")
        .map(|e| format!(" placeholder=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let event_attr = event_attr(element, ctx, &id);
    format!(
        "<input data-lume-id=\"{}\"{}{}{}>\n",
        id, event_attr, value, placeholder
    )
}

fn render_image(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let src = attr_value(element, "src")
        .map(|e| e.raw.trim_matches('"').to_string())
        .unwrap_or_default();
    let alt = attr_value(element, "alt")
        .map(|e| e.raw.trim_matches('"').to_string())
        .unwrap_or_default();
    format!(
        "<img data-lume-id=\"{}\" src=\"{}\" alt=\"{}\">\n",
        id,
        escape_attr(&src),
        escape_attr(&alt)
    )
}

fn render_container(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let mut classes = Vec::new();
    if let Some(class) = layout_class(&element.name) {
        classes.push(class.to_string());
    }
    if element.attrs.iter().any(|attr| {
        matches!(
            attr.name.as_str(),
            "gap" | "padding" | "margin" | "width" | "height" | "align" | "justify" | "columns"
        )
    }) {
        classes.push(style_class_for(element));
    }
    if let Some(style) = attr_value(element, "style") {
        classes.push(style_ref_class(style.raw.trim_matches('"')));
    }
    let class_attr = if classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", classes.join(" "))
    };
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx))
        .unwrap_or_default();
    format!(
        "<div{} data-lume-id=\"{}\">\n{}</div>\n",
        class_attr,
        id,
        indent(&children, 2)
    )
}

fn find_event(view: &ViewBlock) -> Option<&EventNode> {
    view.nodes.iter().find_map(|node| match node {
        ViewNode::Event(event) => Some(event),
        _ => None,
    })
}

fn event_attr(element: &ElementNode, ctx: &mut Ctx, node_id: &str) -> String {
    let Some(event) = element.children.as_ref().and_then(find_event) else {
        return String::new();
    };
    let event_id = ctx.next_event;
    ctx.next_event += 1;
    ctx.events.push(EventBinding {
        id: event_id,
        node_id: node_id.to_string(),
        event: event.event.clone(),
        params: event.params.clone(),
        loop_params: ctx.loop_params.clone(),
        statements: event.body.statements.clone(),
    });
    format!(
        " data-lume-event=\"{}:{}\"",
        escape_attr(&event.event),
        event_id
    )
}

fn first_arg(element: &ElementNode) -> Option<&Expr> {
    element.args.iter().find_map(|arg| match arg {
        Arg::Positional(expr) => Some(expr),
        Arg::Named(_, _) => None,
    })
}

fn attr_value<'a>(element: &'a ElementNode, name: &str) -> Option<&'a Expr> {
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
}

fn node_id(ctx: &mut Ctx) -> String {
    let id = format!("n{}", ctx.next_node);
    ctx.next_node += 1;
    id
}

fn bind_attr(template: &str) -> String {
    let deps = interpolation_deps(template);
    if deps.is_empty() {
        String::new()
    } else {
        format!(" data-lume-bind=\"text:{}\"", escape_attr(&deps.join(",")))
    }
}

fn interpolation_deps(template: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let dep = after[..end].trim();
        if !dep.is_empty() {
            deps.push(dep.to_string());
        }
        rest = &after[end + 1..];
    }
    deps
}

fn render_template_initial(template: &str) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(after);
            return out;
        };
        out.push('0');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn indent(input: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    input.lines().map(|line| format!("{pad}{line}\n")).collect()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(input: &str) -> String {
    escape_html(input).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::generate;
    use lume_hir::lower;
    use lume_ir::build;
    use lume_parser::parse;

    #[test]
    fn emits_style_reference_class() {
        let source = r#"
style card {
  padding: 16
}

component App {
  view {
    Box style=card {
      Text("Hello")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("l-style-card"));
    }
}
