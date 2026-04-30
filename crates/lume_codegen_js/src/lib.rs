use lume_ast::{Arg, AssignOp, ComponentItem, ElementNode, Expr, Stmt, ViewBlock, ViewNode};
use lume_codegen_css::{layout_class, style_class_for, style_ref_class};
use lume_codegen_html::{EventBinding, HtmlOutput};
use lume_ir::LumeProgram;
use std::collections::{BTreeSet, HashSet};

pub fn generate(program: &LumeProgram, html: &HtmlOutput) -> String {
    let state_names = program
        .states()
        .map(|state| state.name.clone())
        .collect::<HashSet<_>>();
    let dynamic_view = !program.routes.is_empty()
        || program
            .view()
            .is_some_and(|view| view_has_dynamic(view, program));
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
    if !program.server_actions.is_empty() {
        js.push_str("const lumeCsrfToken = document.querySelector('meta[name=\"lume-csrf\"]')?.content || \"dev-csrf-token\";\n\n");
        js.push_str("const serverActionParams = {\n");
        for action in &program.server_actions {
            let params = action
                .params
                .iter()
                .map(|param| format!("{:?}", param.name))
                .collect::<Vec<_>>()
                .join(", ");
            js.push_str(&format!("  {:?}: [{}],\n", action.name, params));
        }
        js.push_str("};\n\n");
        js.push_str("async function callServerAction(id, args) {\n");
        js.push_str(
            "  const response = await fetch(`/__lume/actions/${encodeURIComponent(id)}`, {\n",
        );
        js.push_str("    method: \"POST\",\n");
        js.push_str("    headers: { \"content-type\": \"application/json\", \"x-lume-csrf\": lumeCsrfToken },\n");
        js.push_str("    body: JSON.stringify({ args })\n");
        js.push_str("  });\n");
        js.push_str("  const payload = await response.json().catch(() => ({}));\n");
        js.push_str(
            "  if (!response.ok) throw new Error(payload.error || `Server Action ${id} failed`);\n",
        );
        js.push_str("  for (const key of payload.revalidate || []) lumeQueryCache.invalidate(key);\n");
        js.push_str("  return payload.value;\n");
        js.push_str("}\n\n");
        for action in &program.server_actions {
            js.push_str(&format!(
                "async function {}(...args) {{\n  return callServerAction({:?}, args);\n}}\n\n",
                action.name, action.name
            ));
        }
    }
    if !program.queries.is_empty() || !program.server_actions.is_empty() {
        js.push_str("const lumeQueryCache = {\n");
        js.push_str("  values: new Map(),\n");
        js.push_str("  key(value) { return JSON.stringify(value ?? []); },\n");
        js.push_str("  async query(key, loader) {\n");
        js.push_str("    const id = this.key(key);\n");
        js.push_str("    if (this.values.has(id)) return this.values.get(id);\n");
        js.push_str("    const entry = { loading: true, error: null, data: null, refetch: async () => loader() };\n");
        js.push_str("    this.values.set(id, entry);\n");
        js.push_str("    try { entry.data = await loader(); } catch (error) { entry.error = error; throw error; } finally { entry.loading = false; }\n");
        js.push_str("    return entry;\n");
        js.push_str("  },\n");
        js.push_str("  invalidate(key) { this.values.delete(this.key(key)); }\n");
        js.push_str("};\n\n");
        for query in program.queries.iter().filter(|query| !query.is_server) {
            let key = query
                .key
                .as_ref()
                .map(|expr| expr.raw.as_str())
                .unwrap_or_else(|| query.name.as_str());
            js.push_str(&format!(
                "async function query_{}() {{\n  return lumeQueryCache.query({}, async () => fetch({}).then(response => response.json()));\n}}\n\n",
                query.name,
                js_raw_array_or_string(key),
                query_source_url(&query.source)
            ));
        }
    }
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
            if !program.routes.is_empty() {
                js.push_str(&route_runtime(program, &mut ctx, &state_names));
            }
            js.push_str("function render_app() {\n");
            js.push_str("  return ");
            js.push_str(&render_view_expr(
                view,
                &mut ctx,
                &HashSet::new(),
                &state_names,
                program,
            ));
            js.push_str(";\n");
            js.push_str("}\n\n");
        }
        js.push_str("function render_all() {\n");
        js.push_str("  const focus = captureFocus();\n");
        js.push_str("  root.innerHTML = render_app();\n");
        if !program.routes.is_empty() {
            js.push_str("  updateNavLinks();\n");
        }
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
        js.push_str(&format!("  async {}(event, target) {{\n", event.id));
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
                "  if (eventSpec === \"{}:{}\") void actions[{}](event, target);\n",
                event.event, event.id, event.id
            ));
        }
        js.push_str("});\n\n");
    }
    if !program.server_actions.is_empty() {
        js.push_str("root.addEventListener(\"submit\", event => {\n");
        js.push_str("  const form = event.target.closest(\"form[data-lume-form-action]\");\n");
        js.push_str("  if (!form) return;\n");
        js.push_str("  event.preventDefault();\n");
        js.push_str("  const id = form.dataset.lumeFormAction;\n");
        js.push_str("  const data = new FormData(form);\n");
        js.push_str("  const args = (serverActionParams[id] || []).map(name => {\n");
        js.push_str("    const value = data.get(name);\n");
        js.push_str("    if (value === null) return null;\n");
        js.push_str("    if (/^-?\\d+$/.test(value)) return Number(value);\n");
        js.push_str("    if (value === \"true\") return true;\n");
        js.push_str("    if (value === \"false\") return false;\n");
        js.push_str("    return value;\n");
        js.push_str("  });\n");
        js.push_str("  void callServerAction(id, args).then(value => {\n");
        js.push_str("    form.dispatchEvent(new CustomEvent(\"lume:success\", { bubbles: true, detail: { value } }));\n");
        js.push_str("    render_all();\n");
        js.push_str("  }).catch(error => {\n");
        js.push_str("    form.dispatchEvent(new CustomEvent(\"lume:error\", { bubbles: true, detail: { error } }));\n");
        js.push_str("  });\n");
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
    component_stack: Vec<String>,
}

fn render_view_expr(
    view: &ViewBlock,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
    let mut template = String::from("`");
    for node in &view.nodes {
        template.push_str(&render_node_template(node, ctx, locals, states, program));
    }
    template.push('`');
    template
}

fn render_node_template(
    node: &ViewNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
    match node {
        ViewNode::Element(element) => {
            render_element_template(element, ctx, locals, states, program)
        }
        ViewNode::Text(text) => render_text_template(&text.value, locals, states),
        ViewNode::If(node) => {
            let then_html = render_view_expr(&node.then_block, ctx, locals, states, program);
            let else_html = node
                .else_block
                .as_ref()
                .map(|block| render_view_expr(block, ctx, locals, states, program))
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
            let body = render_view_expr(&node.body, ctx, &child_locals, states, program);
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
    program: &LumeProgram,
) -> String {
    if program.component_named(&element.name).is_some() {
        return render_component_template(element, ctx, locals, states, program);
    }
    match element.name.as_str() {
        "Outlet" => "${render_route()}".into(),
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
        "Link" | "NavLink" | "Anchor" => {
            render_link_template(element, ctx, locals, states, program)
        }
        "Form" => render_form_template(element, ctx, locals, states, program),
        _ => render_container_template(element, ctx, locals, states, program),
    }
}

fn render_component_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
    let Some(component) = program.component_named(&element.name) else {
        return String::new();
    };
    if ctx.component_stack.contains(&component.name) {
        return String::new();
    }
    let Some(view) = program.expand_component_view(component, element) else {
        return String::new();
    };
    ctx.component_stack.push(component.name.clone());
    let html = render_view_expr(&view, ctx, locals, states, program);
    ctx.component_stack.pop();
    format!("${{{html}}}")
}

fn route_runtime(program: &LumeProgram, ctx: &mut RenderCtx, states: &HashSet<String>) -> String {
    let routes = program
        .routes
        .iter()
        .filter(|route| route.view.is_some())
        .map(|route| {
            let segments = route
                .segments
                .iter()
                .map(route_segment_js)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "  {{ renderer: {}, segments: [{}] }}",
                route_renderer_name(&route.id),
                segments
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let renderers = program
        .routes
        .iter()
        .filter_map(|route| {
            let view = route.view.as_ref()?;
            let mut locals = HashSet::new();
            locals.insert("params".into());
            let body = render_view_expr(view, ctx, &locals, states, program);
            Some(format!(
                "function {}(params) {{\n  return {};\n}}\n",
                route_renderer_name(&route.id),
                body
            ))
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "const routeTable = [\n{}\n];\n\n{}\nfunction normalizeRoutePath(path) {{\n  if (!path || path === \"/\") return \"/\";\n  return path.endsWith(\"/\") ? path.slice(0, -1) : path;\n}}\n\nfunction matchRoute(route, path) {{\n  const parts = path === \"/\" ? [] : path.replace(/^\\//, \"\").split(\"/\");\n  const params = {{}};\n  let index = 0;\n  for (const segment of route.segments) {{\n    if (segment.kind === \"static\") {{\n      if (parts[index] !== segment.value) return null;\n      index += 1;\n    }} else if (segment.kind === \"dynamic\") {{\n      const value = parts[index];\n      if (value === undefined) return null;\n      if (segment.type !== \"String\" && !/^-?\\d+$/.test(value)) return null;\n      params[segment.name] = value;\n      index += 1;\n    }} else if (segment.kind === \"catchAll\") {{\n      params[segment.name] = parts.slice(index).join(\"/\");\n      index = parts.length;\n      break;\n    }}\n  }}\n  return index === parts.length ? params : null;\n}}\n\nfunction render_route() {{\n  const path = normalizeRoutePath(window.location.pathname);\n  for (const route of routeTable) {{\n    const params = matchRoute(route, path);\n    if (params) return route.renderer(params);\n  }}\n  return \"\";\n}}\n\nfunction navigate(to) {{\n  const url = new URL(to, window.location.href);\n  if (url.origin !== window.location.origin) {{\n    window.location.href = url.href;\n    return;\n  }}\n  if (url.pathname === window.location.pathname && url.search === window.location.search) return;\n  history.pushState(null, \"\", url.pathname + url.search + url.hash);\n  render_all();\n}}\n\nwindow.addEventListener(\"popstate\", () => render_all());\n\nroot.addEventListener(\"click\", event => {{\n  const link = event.target.closest(\"a[data-lume-link]\");\n  if (!link || event.defaultPrevented || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || link.target) return;\n  const url = new URL(link.getAttribute(\"href\") || \"\", window.location.href);\n  if (url.origin !== window.location.origin) return;\n  event.preventDefault();\n  navigate(url.pathname + url.search + url.hash);\n}});\n\nfunction updateNavLinks() {{\n  const current = normalizeRoutePath(window.location.pathname);\n  for (const link of root.querySelectorAll(\"a[data-lume-navlink]\")) {{\n    const href = normalizeRoutePath(new URL(link.getAttribute(\"href\") || \"/\", window.location.href).pathname);\n    const active = href === current;\n    link.toggleAttribute(\"aria-current\", active);\n    link.classList.toggle(\"is-active\", active);\n  }}\n}}\n\n",
        routes, renderers
    )
}

fn route_segment_js(segment: &lume_ir::RouteSegment) -> String {
    match segment {
        lume_ir::RouteSegment::Static(value) => {
            format!("{{ kind: \"static\", value: {:?} }}", value)
        }
        lume_ir::RouteSegment::Dynamic { name, ty } => format!(
            "{{ kind: \"dynamic\", name: {:?}, type: {:?} }}",
            name,
            ty.as_deref().unwrap_or("String")
        ),
        lume_ir::RouteSegment::CatchAll { name } => {
            format!("{{ kind: \"catchAll\", name: {:?} }}", name)
        }
    }
}

fn route_renderer_name(id: &str) -> String {
    format!("render_route_{}", id.replace('-', "_"))
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
    let type_attr = attr_value(element, "type")
        .map(|e| format!(" type=\"{}\"", escape_template(e.raw.trim_matches('"'))))
        .unwrap_or_default();
    format!("<button{}{}>{}</button>", event_attr, type_attr, label)
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
    if let Some(name) = attr_value(element, "name") {
        html.push_str(&format!(
            " name=\"{}\"",
            escape_template(name.raw.trim_matches('"'))
        ));
    }
    if let Some(input_type) = attr_value(element, "type") {
        html.push_str(&format!(
            " type=\"{}\"",
            escape_template(input_type.raw.trim_matches('"'))
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

fn render_link_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
    let id = node_id(ctx);
    let href = attr_value(element, "to")
        .or_else(|| attr_value(element, "href"))
        .map(|expr| attr_template_expr(expr, locals, states))
        .unwrap_or_else(|| "#".into());
    let children = element
        .children
        .as_ref()
        .map(|view| {
            let expr = render_view_expr(view, ctx, locals, states, program);
            format!("${{{expr}}}")
        })
        .unwrap_or_else(|| {
            first_arg(element)
                .map(|e| template_html(e.raw.trim().trim_matches('"'), locals, states))
                .unwrap_or_default()
        });
    let nav_attr = if element.name == "NavLink" {
        " data-lume-navlink=\"true\""
    } else {
        ""
    };
    format!(
        "<a data-lume-id=\"{}\" href=\"{}\" data-lume-link=\"true\"{}>{}</a>",
        id, href, nav_attr, children
    )
}

fn render_form_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
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
                format!(" action=\"{}\"", escape_template(name))
            } else {
                format!(" action=\"/__lume/actions/{}\"", escape_template(name))
            }
        })
        .unwrap_or_default();
    let form_action_attr = action
        .as_ref()
        .filter(|name| {
            !name.starts_with('/') && !name.starts_with("http://") && !name.starts_with("https://")
        })
        .map(|name| format!(" data-lume-form-action=\"{}\"", escape_template(name)))
        .unwrap_or_default();
    let children = element
        .children
        .as_ref()
        .map(|view| {
            let expr = render_view_expr(view, ctx, locals, states, program);
            format!("${{{expr}}}")
        })
        .unwrap_or_default();
    format!(
        "<form data-lume-id=\"{}\" method=\"{}\"{}{}>{}</form>",
        id,
        escape_template(&method),
        action_attr,
        form_action_attr,
        children
    )
}

fn render_container_template(
    element: &ElementNode,
    ctx: &mut RenderCtx,
    locals: &HashSet<String>,
    states: &HashSet<String>,
    program: &LumeProgram,
) -> String {
    let id = node_id(ctx);
    let class_attr = class_attr(element);
    let children = element
        .children
        .as_ref()
        .map(|view| {
            let expr = render_view_expr(view, ctx, locals, states, program);
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

fn view_has_dynamic(view: &ViewBlock, program: &LumeProgram) -> bool {
    view.nodes
        .iter()
        .any(|node| node_has_dynamic(node, program))
}

fn node_has_dynamic(node: &ViewNode, program: &LumeProgram) -> bool {
    match node {
        ViewNode::If(_) | ViewNode::For(_) => true,
        ViewNode::Element(element) => {
            if let Some(component) = program.component_named(&element.name) {
                return component.items.iter().any(|item| match item {
                    ComponentItem::View(view) => view_has_dynamic(view, program),
                    _ => false,
                });
            }
            element
                .children
                .as_ref()
                .is_some_and(|view| view_has_dynamic(view, program))
        }
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

fn js_raw_array_or_string(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with('[') || is_string_literal(raw) {
        raw.to_string()
    } else {
        format!("{:?}", raw)
    }
}

fn query_source_url(expr: &Expr) -> String {
    let raw = expr.raw.trim();
    if let Some(start) = raw.find('"') {
        if let Some(end) = raw[start + 1..].find('"') {
            return format!("{:?}", &raw[start + 1..start + 1 + end]);
        }
    }
    format!("{:?}", raw)
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

    #[test]
    fn generates_server_action_client_stubs() {
        let source = r#"
server action add(amount: i64): i64 {
  return amount
}

component App {
  state count: i64 = 0

  view {
    Button("Save") {
      on click {
        add(count)
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
        assert!(js.contains("async function callServerAction(id, args)"));
        assert!(js.contains("async function add(...args)"));
        assert!(js.contains("return callServerAction(\"add\", args);"));
        assert!(js.contains("void actions[0](event, target);"));
        assert!(js.contains("add(state.count);"));
    }

    #[test]
    fn generates_outlet_route_renderer() {
        let source = r#"
layout Shell {
  view {
    Column {
      Text("Shell")
      Outlet()
    }
  }
}

component Home {
  view {
    Text("Home")
  }
}

component Login {
  view {
    Text("Login required")
  }
}

component User(id: String) {
  view {
    Text("User {id}")
  }
}

route "/" layout=Shell {
  index {
    Home()
  }

  route "users" {
    route "login" {
      Login()
    }

    route ":id<String>" {
      User(id=params.id)
    }
  }
}

component App {
  view {
    Shell()
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("function render_route()"));
        assert!(js.contains("render_route_users_login"));
        assert!(js.contains("Login required"));
        assert!(js.contains("User ${escapeHtml(params.id)}"));
        assert!(js.contains("${render_route()}"));
    }

    #[test]
    fn generates_link_navigation_and_form_action_helpers() {
        let source = r#"
server action save(message: String): String {
  return message
}

component Home {
  view {
    Column {
      NavLink("Users", to="/users")
      Form action=save method="post" {
        Input(name="message", label="Message")
        Button("Save", type="submit")
      }
    }
  }
}

route "/" {
  index {
    Home()
  }

  route "users" {
    Text("Users")
  }
}

component App {
  view {
    Column {
      Link("Home", to="/")
      Outlet()
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let html = generate_html(&ir);
        let js = generate(&ir, &html);
        assert!(js.contains("function navigate(to)"));
        assert!(js.contains("root.addEventListener(\"click\""));
        assert!(js.contains("data-lume-link=\"true\""));
        assert!(js.contains("data-lume-navlink=\"true\""));
        assert!(js.contains("serverActionParams"));
        assert!(js.contains("form[data-lume-form-action]"));
        assert!(js.contains("data-lume-form-action=\"save\""));
    }
}
