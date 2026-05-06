use lume_ast::*;
use lume_codegen_css::{is_style_attr, layout_class, style_class_for, style_ref_class};
use lume_ir::LumeProgram;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HtmlOutput {
    pub html: String,
    pub bindings: Vec<Binding>,
    pub events: Vec<EventBinding>,
    pub serialized_state_json: String,
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
    generate_with_resume(program, true)
}

pub fn generate_with_resume(program: &LumeProgram, resume: bool) -> HtmlOutput {
    let serialized_state_json = serialized_state(program);
    let mut ctx = Ctx {
        next_node: 1,
        next_event: 0,
        bindings: Vec::new(),
        events: Vec::new(),
        loop_params: Vec::new(),
        locals: HashMap::new(),
        state: initial_state(program),
        component_stack: Vec::new(),
        resume,
    };
    let body = program
        .view()
        .map(|view| render_view(view, &mut ctx, program))
        .unwrap_or_default();
    let root_resume_attrs = if resume { " data-lume-r=\"b0\"" } else { "" };
    let html = format!(
        "<!doctype html>\n<html lang=\"ja\">\n  <head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title>Lume App</title>\n    <link rel=\"stylesheet\" href=\"/assets/style.css\">\n    <script type=\"module\" src=\"/assets/app.js\"></script>\n  </head>\n  <body>\n    <div id=\"lume-root\" data-lume-component=\"{}\" data-lume-id=\"c0\"{}>\n{}    </div>\n    <script type=\"application/lume-state\" id=\"lume-state-s0\">\n{}\n    </script>\n  </body>\n</html>\n",
        escape_attr(&program.component.name),
        root_resume_attrs,
        indent(&body, 6),
        serialized_state_json,
    );
    HtmlOutput {
        html,
        bindings: ctx.bindings,
        events: ctx.events,
        serialized_state_json,
    }
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
    resume: bool,
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
        "Text" => render_text_element(element, ctx),
        "Button" => render_button(element, ctx),
        "Input" => render_input(element, ctx),
        "TextArea" => render_text_area(element, ctx),
        "Image" => render_image(element, ctx),
        "Script" => render_script(element),
        "Canvas" => render_canvas(element, ctx),
        "NativeCanvas" => render_native_canvas(element, ctx),
        "GpuCanvas" => render_gpu_canvas(element, ctx),
        "Link" | "NavLink" | "Anchor" => render_link(element, ctx, program),
        "Form" => render_form(element, ctx, program),
        "Outlet" => render_outlet(element, ctx),
        "Router" => render_semantic_container(element, ctx, program, "div", "l-router", None),
        "Route" => render_semantic_container(element, ctx, program, "div", "l-route", None),
        "Modal" => render_dialog_like(element, ctx, program, true),
        "Dialog" => render_dialog_like(element, ctx, program, false),
        "Tabs" => {
            render_semantic_container(element, ctx, program, "div", "l-tabs", Some("tablist"))
        }
        "Table" => render_semantic_container(element, ctx, program, "table", "l-table", None),
        "Spacer" => render_spacer(element, ctx),
        "Field" => render_field(element, ctx, program),
        "VisuallyHidden" => render_visually_hidden(element, ctx, program),
        "FocusTrap" => {
            render_semantic_container(element, ctx, program, "div", "l-focus-trap", None)
        }
        "Landmark" => render_landmark(element, ctx, program),
        "Row" | "Column" | "Box" | "Container" | "Grid" | "Stack" => {
            render_container(element, ctx, program)
        }
        _ => render_container(element, ctx, program),
    }
}

fn render_canvas(element: &ElementNode, ctx: &mut Ctx) -> String {
    render_canvas_surface(element, ctx, String::new())
}

fn render_native_canvas(element: &ElementNode, ctx: &mut Ctx) -> String {
    render_canvas_surface(element, ctx, native_canvas_attrs(element, ctx))
}

fn render_gpu_canvas(element: &ElementNode, ctx: &mut Ctx) -> String {
    render_canvas_surface(element, ctx, gpu_canvas_attrs(element, ctx))
}

fn render_canvas_surface(element: &ElementNode, ctx: &mut Ctx, native_attrs: String) -> String {
    let id = node_id(ctx);
    let width = attr_value(element, "width")
        .map(|expr| eval_expr(expr, ctx).to_attr())
        .unwrap_or_else(|| "640".into());
    let height = attr_value(element, "height")
        .map(|expr| eval_expr(expr, ctx).to_attr())
        .unwrap_or_else(|| "360".into());
    let event_attr = event_attr(element, ctx, &id);
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
    render_text_node("span", expr, &[], ctx)
}

fn render_text_element(element: &ElementNode, ctx: &mut Ctx) -> String {
    let fallback = Expr {
        raw: "\"\"".into(),
        span: element.span,
    };
    let expr = first_arg(element).unwrap_or(&fallback);
    let tag = attr_value(element, "as")
        .map(|expr| text_tag(expr.raw.trim_matches('"')))
        .unwrap_or("span");
    let mut classes = style_classes(element);
    if let Some(style) = attr_value(element, "style") {
        classes.push(style_ref_class(style.raw.trim_matches('"')));
    }
    render_text_node(tag, expr, &classes, ctx)
}

fn render_text_node(tag: &str, expr: &Expr, classes: &[String], ctx: &mut Ctx) -> String {
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
    let class_attr = class_attr(classes);
    format!(
        "<{tag} data-lume-id=\"{}\"{}{}>{}</{tag}>\n",
        id,
        class_attr,
        bind_attr(&template, ctx.resume),
        escape_html(&render_template_initial(&template, ctx))
    )
}

fn text_tag(raw: &str) -> &'static str {
    match raw {
        "p" => "p",
        "label" => "label",
        "strong" => "strong",
        "em" => "em",
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "h4" => "h4",
        "h5" => "h5",
        "h6" => "h6",
        _ => "span",
    }
}

fn render_button(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let label = first_arg(element)
        .map(|e| render_template_initial(e.raw.trim().trim_matches('"'), ctx))
        .unwrap_or_default();
    let mut classes = style_classes(element);
    if let Some(style) = attr_value(element, "style") {
        classes.push(style_ref_class(style.raw.trim_matches('"')));
    }
    let class_attr = class_attr(&classes);
    let event_attr = event_attr(element, ctx, &id);
    let type_attr = attr_value(element, "type")
        .map(|e| format!(" type=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    format!(
        "<button data-lume-id=\"{}\"{}{}{}>{}</button>\n",
        id,
        class_attr,
        event_attr,
        type_attr,
        escape_html(&label)
    )
}

fn render_input(element: &ElementNode, ctx: &mut Ctx) -> String {
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
    let aria = aria_label_attr(element, ctx);
    let disabled = bool_attr(element, "disabled");
    let required = bool_attr(element, "required");
    let event_attr = event_attr(element, ctx, &id);
    let input = format!(
        "<input data-lume-id=\"{}\"{}{}{}{}{}{}{}{}>\n",
        id, event_attr, value, placeholder, name, input_type, aria, disabled, required
    );
    wrap_with_label(element, ctx, input)
}

fn render_text_area(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let value = attr_value(element, "value")
        .map(|e| eval_expr(e, ctx).to_text())
        .unwrap_or_default();
    let placeholder = attr_value(element, "placeholder")
        .map(|e| format!(" placeholder=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let name = attr_value(element, "name")
        .map(|e| format!(" name=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    let aria = aria_label_attr(element, ctx);
    let disabled = bool_attr(element, "disabled");
    let required = bool_attr(element, "required");
    let event_attr = event_attr(element, ctx, &id);
    let textarea = format!(
        "<textarea data-lume-id=\"{}\"{}{}{}{}{}{}>{}</textarea>\n",
        id,
        event_attr,
        placeholder,
        name,
        aria,
        disabled,
        required,
        escape_html(&value)
    );
    wrap_with_label(element, ctx, textarea)
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

fn render_script(element: &ElementNode) -> String {
    let Some(src) = attr_value(element, "src") else {
        return String::new();
    };
    let src = src.raw.trim().trim_matches(['"', '\'']);
    let ty = attr_value(element, "type")
        .map(|expr| expr.raw.trim().trim_matches(['"', '\'']))
        .unwrap_or("module");
    let integrity = script_string_attr(element, "integrity", "integrity");
    let crossorigin = script_string_attr(element, "crossorigin", "crossorigin")
        .or_else(|| script_string_attr(element, "crossOrigin", "crossorigin"))
        .unwrap_or_default();
    let referrer_policy = script_string_attr(element, "referrerPolicy", "referrerpolicy")
        .or_else(|| script_string_attr(element, "referrerpolicy", "referrerpolicy"))
        .unwrap_or_default();
    let nonce = script_string_attr(element, "nonce", "nonce");
    format!(
        "<script type=\"{}\" src=\"{}\"{}{}{}{}{}{}></script>\n",
        escape_attr(ty),
        escape_attr(src),
        bool_attr(element, "async"),
        bool_attr(element, "defer"),
        integrity.unwrap_or_default(),
        crossorigin,
        referrer_policy,
        nonce.unwrap_or_default()
    )
}

fn script_string_attr(element: &ElementNode, source_name: &str, html_name: &str) -> Option<String> {
    attr_value(element, source_name).map(|expr| {
        format!(
            " {html_name}=\"{}\"",
            escape_attr(expr.raw.trim().trim_matches(['"', '\'']))
        )
    })
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
    let enctype_attr = attr_value(element, "enctype")
        .map(|e| format!(" enctype=\"{}\"", escape_attr(e.raw.trim_matches('"'))))
        .or_else(|| {
            action
                .as_ref()
                .filter(|name| action_accepts_file_upload(program, name))
                .map(|_| " enctype=\"multipart/form-data\"".to_string())
        })
        .unwrap_or_default();
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    format!(
        "<form data-lume-id=\"{}\" method=\"{}\"{}{}{}>\n{}</form>\n",
        id,
        escape_attr(&method),
        action_attr,
        form_action_attr,
        enctype_attr,
        indent(&children, 2)
    )
}

fn action_accepts_file_upload(program: &LumeProgram, name: &str) -> bool {
    program
        .server_actions
        .iter()
        .find(|action| action.name == name)
        .is_some_and(|action| {
            action.params.iter().any(|param| {
                let ty = param.ty.trim();
                ty == "File"
                    || ty == "FormData"
                    || ty.ends_with("File[]")
                    || ty.contains("Array<File>")
            })
        })
}

fn render_outlet(_element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    format!(
        "<div data-lume-id=\"{}\" data-lume-outlet=\"true\"></div>\n",
        id
    )
}

fn render_dialog_like(
    element: &ElementNode,
    ctx: &mut Ctx,
    program: &LumeProgram,
    modal: bool,
) -> String {
    let id = node_id(ctx);
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    let title = attr_value(element, "title")
        .map(|expr| {
            format!(
                " aria-label=\"{}\"",
                escape_attr(&eval_expr(expr, ctx).to_attr())
            )
        })
        .unwrap_or_default();
    let open = bool_attr(element, "open");
    let modal_attr = if modal {
        " data-lume-modal=\"true\" aria-modal=\"true\""
    } else {
        ""
    };
    let class = if modal { "l-modal" } else { "l-dialog" };
    format!(
        "<dialog class=\"{}\" data-lume-id=\"{}\"{}{}{}>\n{}</dialog>\n",
        class,
        id,
        modal_attr,
        title,
        open,
        indent(&children, 2)
    )
}

fn render_semantic_container(
    element: &ElementNode,
    ctx: &mut Ctx,
    program: &LumeProgram,
    tag: &str,
    base_class: &str,
    role: Option<&str>,
) -> String {
    let id = node_id(ctx);
    let mut classes = vec![base_class.to_string()];
    classes.extend(style_classes(element));
    if let Some(style) = attr_value(element, "style") {
        classes.push(style_ref_class(style.raw.trim_matches('"')));
    }
    let role_attr = role
        .map(|role| format!(" role=\"{}\"", escape_attr(role)))
        .unwrap_or_default();
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    format!(
        "<{tag}{} data-lume-id=\"{}\"{}>\n{}</{tag}>\n",
        class_attr(&classes),
        id,
        role_attr,
        indent(&children, 2)
    )
}

fn render_spacer(element: &ElementNode, ctx: &mut Ctx) -> String {
    let id = node_id(ctx);
    let mut classes = vec!["l-spacer".to_string()];
    classes.extend(style_classes(element));
    let aria_hidden = " aria-hidden=\"true\"";
    format!(
        "<div{} data-lume-id=\"{}\"{}></div>\n",
        class_attr(&classes),
        id,
        aria_hidden
    )
}

fn render_field(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let label = attr_value(element, "label")
        .map(|expr| {
            format!(
                "<label class=\"l-field-label\">{}</label>\n",
                escape_html(&eval_expr(expr, ctx).to_text())
            )
        })
        .unwrap_or_default();
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    format!(
        "<div class=\"l-field\" data-lume-id=\"{}\">\n{}{}</div>\n",
        id,
        indent(&label, 2),
        indent(&children, 2)
    )
}

fn render_visually_hidden(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_else(|| {
            first_arg(element)
                .map(|expr| {
                    escape_html(&render_template_initial(
                        expr.raw.trim().trim_matches('"'),
                        ctx,
                    ))
                })
                .unwrap_or_default()
        });
    format!(
        "<span class=\"l-visually-hidden\" data-lume-id=\"{}\">{}</span>\n",
        id, children
    )
}

fn render_landmark(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let tag = attr_value(element, "as")
        .or_else(|| attr_value(element, "type"))
        .map(|expr| landmark_tag(expr.raw.trim_matches('"')))
        .unwrap_or("section");
    render_semantic_container(element, ctx, program, tag, "l-landmark", None)
}

fn render_container(element: &ElementNode, ctx: &mut Ctx, program: &LumeProgram) -> String {
    let id = node_id(ctx);
    let mut classes = Vec::new();
    if let Some(class) = layout_class(&element.name) {
        classes.push(class.to_string());
    }
    classes.extend(style_classes(element));
    if let Some(style) = attr_value(element, "style") {
        classes.push(style_ref_class(style.raw.trim_matches('"')));
    }
    let children = element
        .children
        .as_ref()
        .map(|view| render_view(view, ctx, program))
        .unwrap_or_default();
    format!(
        "<div{} data-lume-id=\"{}\">\n{}</div>\n",
        class_attr(&classes),
        id,
        indent(&children, 2)
    )
}

fn wrap_with_label(element: &ElementNode, ctx: &Ctx, control: String) -> String {
    let Some(label) = attr_value(element, "label") else {
        return control;
    };
    format!(
        "<label class=\"l-field-label\">{}\n{}</label>\n",
        escape_html(&render_template_initial(
            label.raw.trim().trim_matches('"'),
            ctx
        )),
        indent(&control, 2)
    )
}

fn aria_label_attr(element: &ElementNode, ctx: &Ctx) -> String {
    attr_value(element, "aria-label")
        .or_else(|| attr_value(element, "ariaLabel"))
        .map(|expr| {
            format!(
                " aria-label=\"{}\"",
                escape_attr(&render_template_initial(
                    expr.raw.trim().trim_matches('"'),
                    ctx
                ))
            )
        })
        .unwrap_or_default()
}

fn bool_attr(element: &ElementNode, name: &str) -> String {
    let has_named_attr = element
        .attrs
        .iter()
        .find(|attr| attr.name == name)
        .and_then(|attr| match attr.value.as_ref() {
            Some(value) if matches!(value.raw.trim(), "false" | "\"false\"") => None,
            _ => Some(format!(" {name}")),
        })
        .or_else(|| {
            element.args.iter().find_map(|arg| match arg {
                Arg::Named(arg_name, value)
                    if arg_name == name && !matches!(value.raw.trim(), "false" | "\"false\"") =>
                {
                    Some(format!(" {name}"))
                }
                Arg::Positional(value) if value.raw.trim() == name => Some(format!(" {name}")),
                _ => None,
            })
        });
    has_named_attr.unwrap_or_default()
}

fn style_classes(element: &ElementNode) -> Vec<String> {
    let has_style_attr = element.attrs.iter().any(|attr| is_style_attr(&attr.name))
        || element.args.iter().any(|arg| match arg {
            Arg::Named(name, _) => is_style_attr(name),
            Arg::Positional(_) => false,
        });
    if has_style_attr {
        vec![style_class_for(element)]
    } else {
        Vec::new()
    }
}

fn class_attr(classes: &[String]) -> String {
    if classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", classes.join(" "))
    }
}

fn landmark_tag(raw: &str) -> &'static str {
    match raw {
        "header" | "banner" => "header",
        "nav" | "navigation" => "nav",
        "main" => "main",
        "aside" | "complementary" => "aside",
        "footer" | "contentinfo" => "footer",
        _ => "section",
    }
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
    let mut attr = format!(
        " data-lume-event=\"{}:{}\"",
        escape_attr(&event.event),
        event_id
    );
    if ctx.resume {
        attr.push_str(&format!(
            " data-lume-on=\"{}:sym_event_{}\" data-lume-state=\"s0\"",
            escape_attr(&event.event),
            event_id
        ));
    }
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

fn bind_attr(template: &str, resume: bool) -> String {
    let deps = interpolation_deps(template);
    if deps.is_empty() {
        String::new()
    } else if resume {
        let scoped = deps
            .iter()
            .map(|dep| format!("s0.{dep}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(" data-lume-bind=\"{}\"", escape_attr(&scoped))
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

fn serialized_state(program: &LumeProgram) -> String {
    let mut fields = Vec::new();
    for state in program.states() {
        let value = parse_value(state.init.raw.trim()).to_json();
        fields.push(format!("\"{}\":{}", escape_json(&state.name), value));
    }
    format!("{{{}}}", fields.join(","))
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
    use super::{generate, generate_with_resume};
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
      Text("Hello", color=ink, weight=700)
      Button("Save", style=card)
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("l-style-card"));
        assert!(html.html.contains("class=\"l-s"));
        assert!(html.html.contains("<button data-lume-id=\""));
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
    fn omits_resume_markers_when_resume_is_disabled() {
        let source = r#"
component App {
  state count: i32 = 0

  view {
    Button("Add") {
      on click {
        count += 1
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_with_resume(&ir, false);
        assert!(!html.html.contains("data-lume-r=\"b0\""));
        assert!(!html.html.contains("data-lume-on=\"click:sym_event_0\""));
        assert!(html.html.contains("data-lume-event=\"click:0\""));
    }

    #[test]
    fn emits_resumable_state_binding_marker() {
        let source = r#"
component App {
  state count: i32 = 0

  view {
    Text("Count: {count}")
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_with_resume(&ir, true);
        assert!(html.html.contains("data-lume-bind=\"s0.count\""));
        assert!(html.html.contains("id=\"lume-state-s0\""));
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
    fn server_renders_file_action_forms_with_multipart_enctype() {
        let source = r#"
server action upload(file: File): URL {
  return file.name
}

component App {
  view {
    Form action=upload method="post" {
      Input(name="file", label="File", type="file")
      Button("Upload", type="submit")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("enctype=\"multipart/form-data\""));
    }

    #[test]
    fn renders_documented_standard_elements() {
        let source = r#"
component App {
  state note: String = "Hello"

  view {
    Column {
      Container {
        Router {
          Route {
            Text("Routed")
          }
        }
      }
      Text("Title", as="h1", size=24, weight=700)
      TextArea(value=note, label="Note")
      Script(src="/assets/widget.js", defer)
      Modal(title="Details", open) {
        Dialog(title="Inner") {
          VisuallyHidden("Hidden title")
          Field(label="Name") {
            Input(label="Name")
          }
        }
      }
      Tabs {
        Button("First")
      }
      Table {
        Text("Cell")
      }
      Spacer height=12
      FocusTrap {
        Button("Close")
      }
      Landmark(as="main") {
        Outlet()
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate(&ir);
        assert!(html.html.contains("<h1"));
        assert!(html.html.contains("class=\"l-container\""));
        assert!(html
            .html
            .contains("<script type=\"module\" src=\"/assets/widget.js\" defer></script>"));
        assert!(html.html.contains("class=\"l-router\""));
        assert!(html.html.contains("class=\"l-route\""));
        assert!(html.html.contains("<textarea"));
        assert!(html.html.contains("<dialog class=\"l-modal\""));
        assert!(html.html.contains("<dialog class=\"l-dialog\""));
        assert!(html.html.contains("class=\"l-visually-hidden\""));
        assert!(html.html.contains("class=\"l-field\""));
        assert!(html.html.contains("class=\"l-tabs\""));
        assert!(html.html.contains("class=\"l-table\""));
        assert!(html.html.contains("class=\"l-spacer"));
        assert!(html.html.contains("class=\"l-focus-trap\""));
        assert!(html.html.contains("<main class=\"l-landmark\""));
        assert!(html.html.contains("data-lume-outlet=\"true\""));
    }
}
