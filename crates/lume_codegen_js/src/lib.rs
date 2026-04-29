use lume_ast::{Arg, AssignOp, ElementNode, Expr, Stmt, ViewBlock, ViewNode};
use lume_codegen_css::{layout_class, style_class_for, style_ref_class};
use lume_codegen_html::{EventBinding, HtmlOutput};
use lume_ir::LumeProgram;
use std::collections::{BTreeSet, HashSet};

pub fn generate(program: &LumeProgram, html: &HtmlOutput) -> String {
    let state_names = program
        .states()
        .map(|state| state.name.clone())
        .collect::<HashSet<_>>();
    let dynamic_view = program.view().is_some_and(view_has_dynamic);
    let mut js = String::new();
    js.push_str("const fallbackState = {\n");
    for state in program.states() {
        js.push_str(&format!(
            "  {}: {},\n",
            state.name,
            js_expr(&state.init, &HashSet::new(), &HashSet::new())
        ));
    }
    js.push_str("};\n\n");
    js.push_str("const state = {};\n\n");
    js.push_str("async function restoreInitialState() {\n");
    js.push_str("  Object.assign(state, fallbackState);\n");
    js.push_str("  try {\n");
    js.push_str("    const response = await fetch(\"/assets/lume.manifest.json\");\n");
    js.push_str("    if (!response.ok) return;\n");
    js.push_str("    const manifest = await response.json();\n");
    js.push_str("    for (const item of manifest.state || []) {\n");
    js.push_str("      state[item.name] = item.initial;\n");
    js.push_str("    }\n");
    js.push_str("  } catch (_) {}\n");
    js.push_str("}\n\n");
    js.push_str("const root = document.getElementById(\"lume-root\");\n");
    if dynamic_view {
        js.push_str("\nfunction escapeHtml(value) {\n");
        js.push_str("  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');\n");
        js.push_str("}\n\n");
        js.push_str("function escapeAttr(value) {\n");
        js.push_str("  return escapeHtml(value).replaceAll('\"', '&quot;');\n");
        js.push_str("}\n\n");
        js.push_str("function encodeScope(scope) {\n");
        js.push_str("  return encodeURIComponent(JSON.stringify(scope));\n");
        js.push_str("}\n\n");
        if let Some(view) = program.view() {
            let mut ctx = RenderCtx::default();
            js.push_str("function render_app() {\n");
            js.push_str("  return ");
            js.push_str(&render_view_expr(
                view,
                &mut ctx,
                &HashSet::new(),
                &state_names,
            ));
            js.push_str(";\n");
            js.push_str("}\n\n");
        }
        js.push_str("function render_all() {\n");
        js.push_str("  const focus = captureFocus();\n");
        js.push_str("  root.innerHTML = render_app();\n");
        js.push_str("  restoreFocus(focus);\n");
        js.push_str("}\n\n");
        js.push_str("function captureFocus() {\n");
        js.push_str("  const active = document.activeElement;\n");
        js.push_str("  if (!active || !root.contains(active) || !active.dataset.lumeFocusKey) return null;\n");
        js.push_str("  return {\n");
        js.push_str("    key: active.dataset.lumeFocusKey,\n");
        js.push_str("    start: active.selectionStart,\n");
        js.push_str("    end: active.selectionEnd,\n");
        js.push_str("    direction: active.selectionDirection\n");
        js.push_str("  };\n");
        js.push_str("}\n\n");
        js.push_str("function restoreFocus(focus) {\n");
        js.push_str("  if (!focus) return;\n");
        js.push_str("  const next = Array.from(root.querySelectorAll(\"[data-lume-focus-key]\")).find(node => node.dataset.lumeFocusKey === focus.key);\n");
        js.push_str("  if (!next) return;\n");
        js.push_str("  next.focus();\n");
        js.push_str("  if (typeof next.setSelectionRange === \"function\" && focus.start !== null && focus.end !== null) {\n");
        js.push_str(
            "    next.setSelectionRange(focus.start, focus.end, focus.direction || \"none\");\n",
        );
        js.push_str("  }\n");
        js.push_str("}\n\n");
    } else {
        js.push_str("const nodes = {\n");
        for binding in &html.bindings {
            js.push_str(&format!(
                "  {}: root.querySelector('[data-lume-id=\"{}\"]'),\n",
                binding.node_id, binding.node_id
            ));
        }
        for event in &html.events {
            js.push_str(&format!(
                "  {}: root.querySelector('[data-lume-id=\"{}\"]'),\n",
                event.node_id, event.node_id
            ));
        }
        js.push_str("};\n\n");
        for binding in &html.bindings {
            let render_name = render_name(&binding.node_id);
            js.push_str(&format!("function {render_name}() {{\n"));
            js.push_str(&format!(
                "  nodes.{}.textContent = {};\n",
                binding.node_id,
                template_expr(&binding.template, &state_names)
            ));
            js.push_str("}\n\n");
        }
        js.push_str("function render_all() {\n");
        for binding in &html.bindings {
            js.push_str(&format!("  {}();\n", render_name(&binding.node_id)));
        }
        js.push_str("}\n\n");
    }
    js.push_str("const actions = {\n");
    for event in &html.events {
        js.push_str(&format!("  {}(event, target) {{\n", event.id));
        if !event.loop_params.is_empty() {
            js.push_str(
                "    const scope = JSON.parse(decodeURIComponent(target.getAttribute(\"data-lume-scope\") || \"%7B%7D\"));\n",
            );
            for param in &event.loop_params {
                js.push_str(&format!("    const {param} = scope.{param};\n"));
            }
        }
        for param in &event.params {
            if param == "value" {
                js.push_str("    const value = event.target.value;\n");
            }
        }
        let locals = event_locals(event);
        for stmt in &event.statements {
            js.push_str("    ");
            js.push_str(&stmt_js(stmt, &locals, &state_names));
            js.push('\n');
        }
        js.push_str("    render_all();\n");
        js.push_str("  },\n");
    }
    js.push_str("};\n\n");
    let event_names = html
        .events
        .iter()
        .map(|event| event.event.as_str())
        .collect::<BTreeSet<_>>();
    for event_name in event_names {
        js.push_str(&format!(
            "root.addEventListener(\"{event_name}\", event => {{\n"
        ));
        js.push_str("  const target = event.target.closest(\"[data-lume-event]\");\n");
        js.push_str("  if (!target) return;\n");
        js.push_str("  const eventSpec = target.getAttribute(\"data-lume-event\");\n");
        for event in html.events.iter().filter(|event| event.event == event_name) {
            js.push_str(&format!(
                "  if (eventSpec === \"{}:{}\") actions[{}](event, target);\n",
                event.event, event.id, event.id
            ));
        }
        js.push_str("});\n\n");
    }
    js.push_str("await restoreInitialState();\n");
    js.push_str("render_all();\n");
    js
}

#[derive(Default)]
struct RenderCtx {
    next_node: usize,
    next_event: usize,
    loop_params: Vec<String>,
}

fn render_view_expr(
    view: &ViewBlock,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let mut template = String::from("`");
    for node in &view.nodes {
        template.push_str(&render_node_template(node, ctx, locals, states));
    }
    template.push('`');
    template
}

fn render_node_template(
    node: &ViewNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    match node {
        ViewNode::Element(element) => render_element_template(element, ctx, locals, states),
        ViewNode::Text(text) => render_text_template(&text.value, locals, states),
        ViewNode::If(node) => {
            let then_html = render_view_expr(&node.then_block, ctx, locals, states);
            let else_html = node
                .else_block
                .as_ref()
                .map(|block| render_view_expr(block, ctx, locals, states))
                .unwrap_or_else(|| "``".into());
            format!(
                "${{{} ? {} : {}}}",
                js_expr(&node.condition, locals, states),
                then_html,
                else_html
            )
        }
        ViewNode::For(node) => {
            let index = node.index.clone().unwrap_or_else(|| "$index".into());
            let mut child_locals = locals.clone();
            child_locals.insert(node.item.clone());
            child_locals.insert(index.clone());
            ctx.loop_params.push(node.item.clone());
            ctx.loop_params.push(index.clone());
            let body = render_view_expr(&node.body, ctx, &child_locals, states);
            ctx.loop_params.pop();
            ctx.loop_params.pop();
            format!(
                "${{({} ?? []).map(({}, {}) => {}).join(\"\")}}",
                js_expr(&node.iterable, locals, states),
                node.item,
                index,
                body
            )
        }
        ViewNode::SlotUse { .. }
        | ViewNode::SlotFill { .. }
        | ViewNode::Match(_)
        | ViewNode::Event(_) => String::new(),
    }
}

fn render_element_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    match element.name.as_str() {
        "Text" => {
            let expr = first_arg(element).cloned().unwrap_or(Expr {
                raw: "\"\"".into(),
                span: element.span,
            });
            render_text_template(&expr, locals, states)
        }
        "Button" => render_button_template(element, ctx, locals, states),
        "Input" => render_input_template(element, ctx, locals, states),
        "Image" => render_image_template(element, ctx, locals, states),
        _ => render_container_template(element, ctx, locals, states),
    }
}

fn render_text_template(expr: &Expr, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    let template = expr.raw.trim().trim_matches('"');
    let mut html = String::from("<span>");
    html.push_str(&template_html(template, locals, states));
    html.push_str("</span>");
    html
}

fn render_button_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let label = first_arg(element)
        .map(|e| template_html(e.raw.trim().trim_matches('"'), locals, states))
        .unwrap_or_default();
    let event_attr = event_attr_template(element, ctx, locals, states);
    format!("<button{}>{}</button>", event_attr, label)
}

fn render_input_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let id = node_id(ctx);
    let mut html = format!("<input data-lume-id=\"{}\"", id);
    html.push_str(&focus_key_attr(&id, ctx, locals, states));
    html.push_str(&event_attr_template(element, ctx, locals, states));
    if let Some(value) = attr_value(element, "value") {
        html.push_str(&format!(
            " value=\"${{escapeAttr({} ?? \"\")}}\"",
            js_expr(value, locals, states)
        ));
    }
    if let Some(placeholder) = attr_value(element, "placeholder") {
        html.push_str(&format!(
            " placeholder=\"{}\"",
            escape_template(placeholder.raw.trim_matches('"'))
        ));
    }
    html.push('>');
    html
}

fn focus_key_attr(
    id: &str,
    ctx: &RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    if ctx.loop_params.is_empty() {
        return format!(" data-lume-focus-key=\"{id}\"");
    }
    let scope = ctx
        .loop_params
        .iter()
        .map(|param| format!("{param}: {}", js_expr_from_name(param, locals, states)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" data-lume-focus-key=\"${{escapeAttr(\"{id}:\" + encodeScope({{{scope}}}))}}\"")
}

fn render_image_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let id = node_id(ctx);
    let src = attr_value(element, "src")
        .map(|expr| attr_template_expr(expr, locals, states))
        .unwrap_or_default();
    let alt = attr_value(element, "alt")
        .map(|expr| attr_template_expr(expr, locals, states))
        .unwrap_or_default();
    format!(
        "<img data-lume-id=\"{}\" src=\"{}\" alt=\"{}\">",
        id, src, alt
    )
}

fn render_container_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let id = node_id(ctx);
    let class_attr = class_attr(element);
    let children = element
        .children
        .as_ref()
        .map(|view| {
            let expr = render_view_expr(view, ctx, locals, states);
            format!("${{{expr}}}")
        })
        .unwrap_or_default();
    format!(
        "<div{} data-lume-id=\"{}\">{}</div>",
        class_attr, id, children
    )
}

fn event_attr_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
) -> String {
    let Some(event) = element.children.as_ref().and_then(find_event) else {
        return String::new();
    };
    let event_id = ctx.next_event;
    ctx.next_event += 1;
    let mut attr = format!(" data-lume-event=\"{}:{}\"", event.event, event_id);
    if !ctx.loop_params.is_empty() {
        let scope = ctx
            .loop_params
            .iter()
            .map(|param| format!("{param}: {}", js_expr_from_name(param, locals, states)))
            .collect::<Vec<_>>()
            .join(", ");
        attr.push_str(&format!(
            " data-lume-scope=\"${{encodeScope({{{scope}}})}}\""
        ));
    }
    attr
}

fn class_attr(element: &ElementNode) -> String {
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
    if classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", classes.join(" "))
    }
}

fn attr_template_expr(expr: &Expr, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    let raw = expr.raw.trim();
    if is_string_literal(raw) {
        escape_template(raw.trim_matches('"'))
    } else {
        format!("${{escapeAttr({} ?? \"\")}}", js_expr(expr, locals, states))
    }
}

fn template_html(template: &str, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&escape_template(&rest[..start]));
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&escape_template(after));
            return out;
        };
        out.push_str("${escapeHtml(");
        out.push_str(&js_expr(
            &Expr {
                raw: after[..end].trim().into(),
                span: Default::default(),
            },
            locals,
            states,
        ));
        out.push_str(")}");
        rest = &after[end + 1..];
    }
    out.push_str(&escape_template(rest));
    out
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

fn find_event(view: &ViewBlock) -> Option<&lume_ast::EventNode> {
    view.nodes.iter().find_map(|node| match node {
        ViewNode::Event(event) => Some(event),
        _ => None,
    })
}

fn node_id(ctx: &mut RenderCtx) -> String {
    ctx.next_node += 1;
    format!("n{}", ctx.next_node)
}

fn view_has_dynamic(view: &ViewBlock) -> bool {
    view.nodes.iter().any(node_has_dynamic)
}

fn node_has_dynamic(node: &ViewNode) -> bool {
    match node {
        ViewNode::If(_) | ViewNode::For(_) => true,
        ViewNode::Element(element) => element.children.as_ref().is_some_and(view_has_dynamic),
        _ => false,
    }
}

fn stmt_js(stmt: &Stmt, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    match stmt {
        Stmt::Assign {
            target, op, expr, ..
        } => match op {
            AssignOp::Set => format!("state.{target} = {};", js_expr(expr, locals, states)),
            AssignOp::Add => format!("state.{target} += {};", js_expr(expr, locals, states)),
            AssignOp::Sub => format!("state.{target} -= {};", js_expr(expr, locals, states)),
        },
        Stmt::Expr(expr) => format!("{};", js_expr(expr, locals, states)),
    }
}

fn event_locals(event: &EventBinding) -> HashSet<String> {
    event
        .params
        .iter()
        .chain(event.loop_params.iter())
        .cloned()
        .collect()
}

fn js_expr_from_name(name: &str, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    js_expr(
        &Expr {
            raw: name.into(),
            span: Default::default(),
        },
        locals,
        states,
    )
}

fn js_expr(expr: &Expr, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    let raw = expr.raw.trim();
    if raw.is_empty() {
        "undefined".into()
    } else if raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(raw, "true" | "false" | "null" | "undefined")
        && !raw.chars().next().unwrap().is_ascii_digit()
    {
        if locals.contains(raw) {
            raw.to_string()
        } else {
            format!("state.{raw}")
        }
    } else {
        rewrite_expr(raw, locals, states)
    }
}

fn template_expr(template: &str, states: &HashSet<String>) -> String {
    let mut out = String::from("`");
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&escape_template(&rest[..start]));
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&escape_template(after));
            out.push('`');
            return out;
        };
        out.push_str("${");
        out.push_str(&js_expr(
            &Expr {
                raw: after[..end].trim().into(),
                span: Default::default(),
            },
            &HashSet::new(),
            states,
        ));
        out.push('}');
        rest = &after[end + 1..];
    }
    out.push_str(&escape_template(rest));
    out.push('`');
    out
}

fn escape_template(input: &str) -> String {
    input.replace('`', "\\`").replace("${", "\\${")
}

fn rewrite_expr(raw: &str, locals: &HashSet<String>, states: &HashSet<String>) -> String {
    let mut out = String::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' || ch == '\'' {
            out.push(ch);
            while let Some((_, inner)) = chars.next() {
                out.push(inner);
                if inner == '\\' {
                    if let Some((_, escaped)) = chars.next() {
                        out.push(escaped);
                    }
                } else if inner == ch {
                    break;
                }
            }
            continue;
        }
        if ch == '_' || ch.is_ascii_alphabetic() {
            let start = idx;
            let mut end = idx + ch.len_utf8();
            while let Some((next_idx, next)) = chars.peek().copied() {
                if next == '_' || next.is_ascii_alphanumeric() {
                    chars.next();
                    end = next_idx + next.len_utf8();
                } else {
                    break;
                }
            }
            let ident = &raw[start..end];
            let previous = raw[..start].chars().rev().find(|c| !c.is_whitespace());
            if previous == Some('.')
                || locals.contains(ident)
                || !states.contains(ident)
                || matches!(
                    ident,
                    "true" | "false" | "null" | "undefined" | "Math" | "String" | "Number"
                )
            {
                out.push_str(ident);
            } else {
                out.push_str("state.");
                out.push_str(ident);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn is_string_literal(raw: &str) -> bool {
    (raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\''))
}

fn render_name(node_id: &str) -> String {
    format!("render_{node_id}")
}

#[cfg(test)]
mod tests {
    use super::generate;
    use lume_codegen_html::generate as generate_html;
    use lume_hir::lower;
    use lume_ir::build;
    use lume_parser::parse;

    #[test]
    fn generates_input_event_listener_and_value_param() {
        let source = r#"
component App {
  state name: String = ""

  view {
    Column {
      Input(value=name, label="Name") {
        on input(value) {
          name = value
        }
      }
      Text("Hello {name}")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("root.addEventListener(\"input\""));
        assert!(js.contains("const value = event.target.value;"));
        assert!(js.contains("state.name = value;"));
        assert!(js.contains("`Hello ${state.name}`"));
        assert!(js.contains("await restoreInitialState();"));
        assert!(js.contains("fetch(\"/assets/lume.manifest.json\")"));
    }

    #[test]
    fn generates_dynamic_if_and_for_renderer() {
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

      for item in items {
        Text("Item {item}")
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("function render_app()"));
        assert!(js.contains("root.innerHTML = render_app();"));
        assert!(js.contains("${state.count>0 ?"));
        assert!(js.contains("(state.items ?? []).map((item, $index) =>"));
        assert!(js.contains("Item ${escapeHtml(item)}"));
    }

    #[test]
    fn captures_loop_scope_for_events() {
        let source = r#"
component App {
  state selected: String = ""
  state items: Array = ["A", "B"]

  view {
    Column {
      for item, index in items {
        Button("Pick {item}") {
          on click {
            selected = item
          }
        }
      }
      Text("Selected {selected}")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("function encodeScope(scope)"));
        assert!(js.contains("data-lume-scope=\"${encodeScope({item: item, index: index})}\""));
        assert!(js.contains("const scope = JSON.parse(decodeURIComponent"));
        assert!(js.contains("const item = scope.item;"));
        assert!(js.contains("state.selected = item;"));
        assert!(js.contains("Pick ${escapeHtml(item)}"));
    }

    #[test]
    fn preserves_focus_for_dynamic_inputs() {
        let source = r#"
component App {
  state show: Bool = true
  state name: String = ""

  view {
    Column {
      if show {
        Input(value=name, label="Name") {
          on input(value) {
            name = value
          }
        }
      }
      Text("Hello {name}")
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("const focus = captureFocus();"));
        assert!(js.contains("restoreFocus(focus);"));
        assert!(js.contains("function captureFocus()"));
        assert!(js.contains("function restoreFocus(focus)"));
        assert!(js.contains("data-lume-focus-key=\"n"));
    }

    #[test]
    fn scopes_focus_keys_inside_loops() {
        let source = r#"
component App {
  state items: Array = ["A", "B"]

  view {
    Column {
      for item, index in items {
        Input(value=item, label="Item")
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("function encodeScope(scope)"));
        assert!(js.contains("data-lume-focus-key=\"${escapeAttr(\""));
        assert!(js.contains("+ encodeScope({item: item, index: index})"));
    }
}
