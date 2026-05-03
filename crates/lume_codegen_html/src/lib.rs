use lume_ast::*;
use lume_codegen_css::{layout_class, style_class_for, style_ref_class};
use lume_ir::{LumeProgram, ResumeGraph};
use std::collections::HashMap;

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
    generate_with_resume(program, false)
}

pub fn generate_with_resume(program: &LumeProgram, resume: bool) -> HtmlOutput {
    let mut ctx = Ctx {
        next_node: 1,
        next_event: 0,
        bindings: Vec::new(),
        events: Vec::new(),
        loop_params: Vec::new(),
        locals: HashMap::new(),
        state: initial_state(program),
        component_stack: Vec::new(),
        resume_mode: resume,
        resume_event_idx: 0,
    };
    let body = program
        .view()
        .map(|view| render_view(view, &mut ctx, program))
        .unwrap_or_default();

    // Build serialized state script block for resume mode
    let state_script = if resume {
        build_state_script(&program.resume_graph)
    } else {
        String::new()
    };

    // Root element: in resume mode add data-lume-r boundary marker
    let root_extra = if resume { " data-lume-r=\"b0\"" } else { "" };

    let html = format!(
        "<!doctype html>\n<html lang=\"ja\">\n  <head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title>Lume App</title>\n    <link rel=\"stylesheet\" href=\"/assets/style.css\">\n    <script type=\"module\" src=\"/assets/app.js\"></script>\n  </head>\n  <body>\n    <div id=\"lume-root\" data-lume-component=\"{}\" data-lume-id=\"c0\"{}>\n{}    </div>\n{}  </body>\n</html>\n",
        escape_attr(&program.component.name),
        root_extra,
        indent(&body, 6),
        state_script,
    );
    HtmlOutput {
        html,
        bindings: ctx.bindings,
        events: ctx.events,
    }
}

/// Build the inline `<script type="application/lume-state">` blocks.
fn build_state_script(resume_graph: &ResumeGraph) -> String {
    let mut out = String::new();
    for scope in &resume_graph.serialized_state {
        let mut fields = Vec::new();
        for (name, value) in &scope.values {
            fields.push(format!("\"{}\":{}", name, value));
        }
        out.push_str(&format!(
            "    <script type=\"application/lume-state\" id=\"lume-state-{}\">{{{}}}</script>\n",
            scope.id.0,
            fields.join(",")
        ));
    }
    out
}

struct Ctx {
    next_node: usize,
    next_event: usize,
    bindings: Vec<Binding>,
    events: Vec<EventBinding>,
    loop_params: Vec<String>,
    locals: HashMap<String, Value>,
    state: HashMap<String, Value>,
    component_stack: Vec<String>,
    resume_mode: bool,
    resume_event_idx: usize,
}

fn render_view(view: &ViewBlock, ctx: &mut Ctx, program: &LumeProgram) -> String {
    view.nodes
        .iter()
        .map(|node| render_node(node, ctx, program))
        .collect::<Vec<_>>()
        .join("")
}

fn render_node(node: &ViewNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    match node {
        ViewNode::Element(element) => render_element(element, ctx, program),
        ViewNode::Text(text) => render_text_expr(&text.value, ctx),
        ViewNode::If(node) => {
            let body = if eval_expr(&node.condition, ctx).is_truthy() {
                render_view(&node.then_block, ctx, program)
            } else {
                node.else_block
                    .as_ref()
                    .map(|block| render_view(block, ctx, program))
                    .unwrap_or_default()
            };
            format!(
                "<!-- lume-if:{} -->{}",
                escape_attr(&node.condition.raw),
                body
            )
        }
        ViewNode::For(node) => {
            ctx.loop_params.push(node.item.clone());
            let pushed_index = node.index.clone();
            if let Some(index) = &pushed_index {
                ctx.loop_params.push(index.clone());
            }
            let previous_item = ctx.locals.get(&node.item).cloned();
            let previous_index = pushed_index
                .as_ref()
                .and_then(|index| ctx.locals.get(index).cloned());
            let html = match eval_expr(&node.iterable, ctx) {
                Value::Array(items) => items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        ctx.locals.insert(node.item.clone(), item);
                        if let Some(name) = &pushed_index {
                            ctx.locals.insert(name.clone(), Value::Number(index as f64));
                        }
                        render_view(&node.body, ctx, program)
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            if let Some(value) = previous_item {
                ctx.locals.insert(node.item.clone(), value);
            } else {
                ctx.locals.remove(&node.item);
            }
            if let Some(name) = &pushed_index {
                if let Some(value) = previous_index {
                    ctx.locals.insert(name.clone(), value);
                } else {
                    ctx.locals.remove(name);
                }
            }
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

fn render_element(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    if let Some(component) = program.component_named(&element.name) {
        return render_component(component, element, ctx, program);
    }
    match element.name.as_str() {
        "Text" => {
            let expr = first_arg(element).cloned().unwrap_or(Expr {
                raw: "\"\"".into(),
                span: element.span,
            });
            render_text_expr(&expr, ctx)
        }
        "Button" => render_button(element, ctx, program),
        "Input" => render_input(element, ctx, program),
        "Image" => render_image(element, ctx),
        "Canvas" => render_canvas(element, ctx, program),
        "NativeCanvas" => render_native_canvas(element, ctx, program),
        "GpuCanvas" => render_gpu_canvas(element, ctx, program),
        "Link" | "NavLink" | "Anchor" => render_link(element, ctx, program),
        "Form" => render_form(element, ctx, program),
        "Row" | "Column" | "Box" | "Grid" | "Stack" => render_container(element, ctx, program),
        _ => render_container(element, ctx, program),
    }
}

fn render_canvas(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    render_canvas_surface(element, ctx, String::new(), program)
}

fn render_native_canvas(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    render_canvas_surface(element, ctx, native_canvas_attrs(element, ctx), program)
}

fn render_gpu_canvas(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    render_canvas_surface(element, ctx, gpu_canvas_attrs(element, ctx), program)
}

fn render_canvas_surface(element: &ElementNode, ctx: &mut Ctx, native_attrs: String, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let width = attr_value(element, "width")
        .map(|expr| eval_expr(expr, ctx).to_attr())
        .unwrap_or_else(|| "640".into());
    let height = attr_value(element, "height")
        .map(|expr| eval_expr(expr, ctx).to_attr())
        .unwrap_or_else(|| "360".into());
    let event_attr = event_attr(element, ctx, &id, program);
    let aria = attr_value(element, "ariaLabel")
        .or_else(|| attr_value(element, "aria-label"))
        .map(|expr| {
            format!(
                " aria-label=\"{}\"",
                escape_attr(&eval_expr(expr, ctx).to_attr())
            )
        })
        .unwrap_or_default();
    format!(
        "<canvas data-lume-id=\"{}\"{}{}{} width=\"{}\" height=\"{}\" style=\"max-width:100%;height:auto;border:1px solid #20242f;background:#05070c;display:block\"></canvas>\n",
        id,
        event_attr,
        aria,
        native_attrs,
        escape_attr(&width),
        escape_attr(&height)
    )
}

fn native_canvas_attrs(element: &ElementNode, ctx: &Ctx) -> String {
    let Some(renderer) =
        attr_value(element, "renderer").and_then(|expr| native_renderer(expr.raw.trim()))
    else {
        return String::new();
    };
    let args = attr_value(element, "args")
        .map(|expr| native_arg_fields(expr.raw.trim()))
        .unwrap_or_default();
    let arg_names = args
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let arg_attrs = args
        .into_iter()
        .map(|(name, expr)| {
            format!(
                " data-{}=\"{}\"",
                data_attr_name(&name),
                escape_attr(&eval_expr(&expr, ctx).to_attr())
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        " data-lume-native-module=\"{}\" data-lume-native-symbol=\"{}\" data-lume-native-args=\"{}\"{}",
        escape_attr(&renderer.0),
        escape_attr(&renderer.1),
        escape_attr(&arg_names),
        arg_attrs
    )
}

fn gpu_canvas_attrs(element: &ElementNode, _ctx: &Ctx) -> String {
    attr_value(element, "graph")
        .map(|expr| {
            format!(
                " data-lume-gpu-graph=\"{}\"",
                escape_attr(expr.raw.trim_matches(['"', '\'']))
            )
        })
        .unwrap_or_default()
}

fn native_renderer(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim_matches(['"', '\'']);
    let (module, symbol) = raw.split_once('.')?;
    Some((module.trim().to_string(), symbol.trim().to_string()))
}

fn native_arg_fields(raw: &str) -> Vec<(String, Expr)> {
    let raw = raw.trim();
    let Some(body) = raw
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Vec::new();
    };
    split_top_level(body, ',')
        .into_iter()
        .filter_map(|field| {
            let (key, value) = split_top_level_once(&field, ':')?;
            let key = key.trim().trim_matches(['"', '\'']).to_string();
            (!key.is_empty()).then_some((
                key,
                Expr {
                    raw: value.trim().to_string(),
                    span: Default::default(),
                },
            ))
        })
        .collect()
}

fn split_top_level(raw: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => {
                parts.push(raw[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(raw[start..].to_string());
    parts
}

fn split_top_level_once(raw: &str, delimiter: char) -> Option<(String, String)> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => {
                return Some((
                    raw[..idx].to_string(),
                    raw[idx + ch.len_utf8()..].to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}

fn data_attr_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch == '_' {
            out.push('-');
        } else if ch.is_ascii_uppercase() {
            out.push('-');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out.trim_start_matches('-').to_string()
}

fn render_component(
    component: &ComponentDecl,
    element: &ElementNode,
    ctx: &mut Ctx,
    program: &LumeProgram,
) -> String {
    if ctx.component_stack.contains(&component.name) {
        return String::new();
    }
    let Some(view) = program.expand_component_view(component, element) else {
        return String::new();
    };
    ctx.component_stack.push(component.name.clone());
    let html = render_view(&view, ctx, program);
    ctx.component_stack.pop();
    html
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
        escape_html(&render_template_initial(&template, ctx))
    )
}

fn render_button(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let label = first_arg(element)
        .map(|e| render_template_initial(e.raw.trim().trim_matches('"'), ctx))
        .unwrap_or_default();
    let event_attr = event_attr(element, ctx, &id, program);
    let type_attr = attr_value(element, "type")
        .map(|e| format!(" type=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    format!(
        "<button data-lume-id=\"{}\"{}{}>{}</button>\n",
        id,
        event_attr,
        type_attr,
        escape_html(&label)
    )
}

fn render_input(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let value = attr_value(element, "value")
        .map(|e| format!(" value=\"{}\"", escape_attr(&eval_expr(e, ctx).to_attr())))
        .unwrap_or_default();
    let placeholder = attr_value(element, "placeholder")
        .map(|e| format!(" placeholder=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let name = attr_value(element, "name")
        .map(|e| format!(" name=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let input_type = attr_value(element, "type")
        .map(|e| format!(" type=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let event_attr = event_attr(element, ctx, &id, program);
    format!(
        "<input data-lume-id=\"{}\"{}{}{}{}{}>\n",
        id, event_attr, value, placeholder, name, input_type
    )
}

fn render_image(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let src = attr_value(element, "src")
        .map(|e| render_template_initial(e.raw.trim().trim_matches('"'), ctx))
        .unwrap_or_default();
    let alt = attr_value(element, "alt")
        .map(|e| render_template_initial(e.raw.trim().trim_matches('"'), ctx))
        .unwrap_or_default();
    format!(
        "<img data-lume-id=\"{}\" src=\"{}\" alt=\"{}\">\n",
        id,
        escape_attr(&src),
        escape_attr(&alt)
    )
}

fn render_link(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let href = attr_value(element, "to")
        .or_else(|| attr_value(element, "href"))
        .map(|e| render_template_initial(e.raw.trim().trim_matches('"'), ctx))
        .unwrap_or_else(|| "#".into());
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_else(|| {
            first_arg(element)
                .map(|e| {
                    escape_html(&render_template_initial(
                        e.raw.trim().trim_matches('"'),
                        ctx,
                    ))
                })
                .unwrap_or_default()
        });
    let nav_attr = if element.name == "NavLink" {
        " data-lume-navlink=\"true\""
    } else {
        ""
    };
    format!(
        "<a data-lume-id=\"{}\" href=\"{}\" data-lume-link=\"true\"{}>{}</a>\n",
        id,
        escape_attr(&href),
        nav_attr,
        children
    )
}

fn render_form(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let method = attr_value(element, "method")
        .map(|e| e.raw.trim_matches('"').to_ascii_lowercase())
        .unwrap_or_else(|| "post".into());
    let action = attr_value(element, "action").map(|e| e.raw.trim().trim_matches('"').to_string());
    let action_attr = action
        .as_ref()
        .map(|name| {
            if name.starts_with('/') || name.starts_with("http://") || name.starts_with("https://")
            {
                format!(" action=\"{}\"", escape_attr(name))
            } else {
                format!(" action=\"/__lume/actions/{}\"", escape_attr(name))
            }
        })
        .unwrap_or_default();
    let form_action_attr = action
        .as_ref()
        .filter(|name| {
            !name.starts_with('/') && !name.starts_with("http://") && !name.starts_with("https://")
        })
        .map(|name| format!(" data-lume-form-action=\"{}\"", escape_attr(name)))
        .unwrap_or_default();
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    format!(
        "<form data-lume-id=\"{}\" method=\"{}\"{}{}>\n{}</form>\n",
        id,
        escape_attr(&method),
        action_attr,
        form_action_attr,
        indent(&children, 2)
    )
}

fn render_container(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
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
        .map(|view| render_view(view, ctx, program))
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

fn event_attr(element: &ElementNode, ctx: &mut Ctx, node_id: &str, program: &LumeProgram) -> String {
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

    if ctx.resume_mode {
        // In resume mode, emit data-lume-on and data-lume-state instead of data-lume-event
        let resume_idx = ctx.resume_event_idx;
        ctx.resume_event_idx += 1;
        let symbol_id = program
            .resume_graph
            .event_bindings
            .get(resume_idx)
            .map(|eb| eb.symbol_id.0.as_str())
            .unwrap_or("sym_unknown");
        let state_scope = program
            .resume_graph
            .event_bindings
            .get(resume_idx)
            .map(|eb| eb.state_scope_id.0.as_str())
            .unwrap_or("s0");
        let mut attr = format!(
            " data-lume-on=\"{}:{}\" data-lume-state=\"{}\"",
            escape_attr(&event.event),
            escape_attr(symbol_id),
            escape_attr(state_scope),
        );
        if !ctx.loop_params.is_empty() {
            attr.push_str(&format!(
                " data-lume-scope=\"{}\"",
                escape_attr(&encode_scope(ctx))
            ));
        }
        return attr;
    }

    let mut attr = format!(
        " data-lume-event=\"{}:{}\"",
        escape_attr(&event.event),
        event_id
    );
    if !ctx.loop_params.is_empty() {
        attr.push_str(&format!(
            " data-lume-scope=\"{}\"",
            escape_attr(&encode_scope(ctx))
        ));
    }
    attr
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

fn render_template_initial(template: &str, ctx: &Ctx) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(after);
            return out;
        };
        out.push_str(
            &eval_raw(after[..end].trim(), ctx)
                .map(|value| value.to_text())
                .unwrap_or_default(),
        );
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<Value>),
    Null,
}

impl Value {
    fn is_truthy(&self) -> bool {
        match self {
            Value::String(value) => !value.is_empty(),
            Value::Number(value) => *value != 0.0,
            Value::Bool(value) => *value,
            Value::Array(value) => !value.is_empty(),
            Value::Null => false,
        }
    }

    fn to_text(&self) -> String {
        match self {
            Value::String(value) => value.clone(),
            Value::Number(value) if value.fract() == 0.0 => format!("{}", *value as i64),
            Value::Number(value) => value.to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Array(_) | Value::Null => String::new(),
        }
    }

    fn to_attr(&self) -> String {
        self.to_text()
    }
}

fn initial_state(program: &LumeProgram) -> HashMap<String, Value> {
    program
        .states()
        .map(|state| (state.name.clone(), parse_value(state.init.raw.trim())))
        .collect()
}

fn eval_expr(expr: &Expr, ctx: &Ctx) -> Value {
    eval_raw(expr.raw.trim(), ctx).unwrap_or(Value::Null)
}

fn eval_raw(raw: &str, ctx: &Ctx) -> Option<Value> {
    if let Some(value) = ctx.locals.get(raw).or_else(|| ctx.state.get(raw)) {
        return Some(value.clone());
    }
    for op in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((left, right)) = split_binary(raw, op) {
            let left = eval_raw(left, ctx)?;
            let right = eval_raw(right, ctx)?;
            return Some(Value::Bool(compare_values(&left, op, &right)));
        }
    }
    Some(parse_value(raw))
}

fn split_binary<'a>(raw: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let index = raw.find(op)?;
    let left = raw[..index].trim();
    let right = raw[index + op.len()..].trim();
    (!left.is_empty() && !right.is_empty()).then_some((left, right))
}

fn compare_values(left: &Value, op: &str, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => match op {
            ">=" => left >= right,
            "<=" => left <= right,
            "==" => left == right,
            "!=" => left != right,
            ">" => left > right,
            "<" => left < right,
            _ => false,
        },
        _ => match op {
            "==" => left.to_text() == right.to_text(),
            "!=" => left.to_text() != right.to_text(),
            _ => false,
        },
    }
}

fn parse_value(raw: &str) -> Value {
    let raw = raw.trim();
    if is_string_literal(raw) {
        return Value::String(raw[1..raw.len().saturating_sub(1)].to_string());
    }
    if raw == "true" {
        return Value::Bool(true);
    }
    if raw == "false" {
        return Value::Bool(false);
    }
    if raw == "null" {
        return Value::Null;
    }
    if raw.starts_with('[') && raw.ends_with(']') {
        return Value::Array(parse_array_items(&raw[1..raw.len() - 1]));
    }
    raw.parse::<f64>().map(Value::Number).unwrap_or(Value::Null)
}

fn parse_array_items(raw: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut depth = 0usize;
    let chars = raw.char_indices().peekable();
    for (index, ch) in chars {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let item = raw[start..index].trim();
                if !item.is_empty() {
                    items.push(parse_value(item));
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    let item = raw[start..].trim();
    if !item.is_empty() {
        items.push(parse_value(item));
    }
    items
}

fn is_string_literal(raw: &str) -> bool {
    (raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\''))
}

fn encode_scope(ctx: &Ctx) -> String {
    let pairs = ctx
        .loop_params
        .iter()
        .filter_map(|param| {
            ctx.locals
                .get(param)
                .map(|value| format!("\"{}\":{}", escape_json(param), value.to_json()))
        })
        .collect::<Vec<_>>()
        .join(",");
    percent_encode(&format!("{{{pairs}}}"))
}

impl Value {
    fn to_json(&self) -> String {
        match self {
            Value::String(value) => format!("\"{}\"", escape_json(value)),
            Value::Number(value) if value.fract() == 0.0 => format!("{}", *value as i64),
            Value::Number(value) => value.to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Value::to_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Value::Null => "null".into(),
        }
    }
}

fn escape_json(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn percent_encode(input: &str) -> String {
    input
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
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

    #[test]
    fn server_renders_initial_state_templates() {
        let source = r#"
component App {
  state count: i32 = 7
  state name: String = "Lume"

  view {
    Column {
      Text("Count: {count}")
      Input(value=name)
      Image(src="/avatars/{name}.png", alt="{name}")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("Count: 7"));
        assert!(html.html.contains("value=\"Lume\""));
        assert!(html.html.contains("src=\"/avatars/Lume.png\" alt=\"Lume\""));
    }

    #[test]
    fn server_renders_initial_if_and_for_branches() {
        let source = r#"
component App {
  state count: i32 = 1
  state items: Array = ["A", "B"]

  view {
    Column {
      if count > 0 {
        Text("Visible {count}")
      } else {
        Text("Hidden")
      }

      for item, index in items {
        Button("Pick {item}") {
          on click {
            count += index
          }
        }
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("Visible 1"));
        assert!(!html.html.contains("Hidden"));
        assert!(html.html.contains(">Pick A</button>"));
        assert!(html.html.contains(">Pick B</button>"));
        assert!(html
            .html
            .contains("data-lume-scope=\"%7B%22item%22%3A%22A%22%2C%22index%22%3A0%7D\""));
        assert!(html
            .html
            .contains("data-lume-scope=\"%7B%22item%22%3A%22B%22%2C%22index%22%3A1%7D\""));
    }

    #[test]
    fn server_renders_link_and_form_elements() {
        let source = r#"
server action save(message: String): String {
  return message
}

component App {
  view {
    Column {
      Link("Home", to="/")
      Form action=save method="post" {
        Input(name="message", label="Message", type="text")
        Button("Save", type="submit")
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("<a data-lume-id="));
        assert!(html.html.contains("href=\"/\" data-lume-link=\"true\""));
        assert!(html.html.contains("action=\"/__lume/actions/save\""));
        assert!(html.html.contains("data-lume-form-action=\"save\""));
        assert!(html.html.contains("name=\"message\""));
        assert!(html.html.contains("type=\"submit\""));
    }

    #[test]
    fn resume_mode_adds_boundary_marker_and_state_script() {
        let source = r#"
component App {
  state count: i32 = 0

  action increment() {
    count += 1
  }

  view {
    Column {
      Text("Count: {count}")
      Button("増やす") {
        on click {
          increment()
        }
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = super::generate_with_resume(&ir, true);
        // Resume boundary marker on root element
        assert!(html.html.contains("data-lume-r=\"b0\""));
        // Serialized state script block
        assert!(html.html.contains("application/lume-state"));
        assert!(html.html.contains("\"count\":0"));
        // data-lume-on attribute (resume event binding)
        assert!(html.html.contains("data-lume-on=\"click:sym_app_increment\""));
        // data-lume-state attribute
        assert!(html.html.contains("data-lume-state=\"s0\""));
    }
}
