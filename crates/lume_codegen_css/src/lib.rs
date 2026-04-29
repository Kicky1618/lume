use lume_ast::{Attribute, ElementNode, Expr, StyleDecl, ThemeDecl, ViewBlock, ViewNode};
use lume_ir::LumeProgram;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn generate(program: &LumeProgram) -> String {
    let mut spaces = BTreeSet::new();
    let mut rules = BTreeMap::<String, Vec<(String, String)>>::new();
    let tokens = theme_tokens(&program.themes);
    if let Some(view) = program.view() {
        collect_view(view, &tokens, &mut spaces, &mut rules);
    }
    collect_style_spaces(&program.styles, &tokens, &mut spaces);

    let mut css = String::new();
    css.push_str(":root {\n");
    for theme in &program.themes {
        for token in &theme.tokens {
            css.push_str(&format!(
                "  --lume-{}-{}: {};\n",
                token.category,
                token.name,
                css_token_value(&token.category, &token.value)
            ));
        }
    }
    for space in &spaces {
        css.push_str(&format!("  --lume-space-{space}: {space}px;\n"));
    }
    css.push_str("}\n\n");
    css.push_str("body {\n  margin: 0;\n  font-family: system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif;\n  color: #111827;\n  background: #ffffff;\n}\n\n");
    css.push_str(".l-box { box-sizing: border-box; }\n");
    css.push_str(".l-row { display: flex; flex-direction: row; }\n");
    css.push_str(".l-col { display: flex; flex-direction: column; }\n");
    css.push_str(".l-grid { display: grid; }\n");
    css.push_str(".l-stack { display: grid; }\n.l-stack > * { grid-area: 1 / 1; }\n\n");
    for style in &program.styles {
        emit_style_decl(style, &tokens, &mut spaces, &mut css);
    }
    for (class, props) in rules {
        css.push_str(&format!(".{class} {{\n"));
        for (name, value) in props {
            css.push_str(&format!("  {name}: {value};\n"));
        }
        css.push_str("}\n\n");
    }
    css
}

fn collect_view(
    view: &ViewBlock,
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
    rules: &mut BTreeMap<String, Vec<(String, String)>>,
) {
    for node in &view.nodes {
        match node {
            ViewNode::Element(element) => collect_element(element, tokens, spaces, rules),
            ViewNode::If(node) => {
                collect_view(&node.then_block, tokens, spaces, rules);
                if let Some(block) = &node.else_block {
                    collect_view(block, tokens, spaces, rules);
                }
            }
            ViewNode::For(node) => collect_view(&node.body, tokens, spaces, rules),
            _ => {}
        }
    }
}

fn collect_element(
    element: &ElementNode,
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
    rules: &mut BTreeMap<String, Vec<(String, String)>>,
) {
    let mut props = Vec::new();
    for attr in &element.attrs {
        if let Some((css_name, value)) = style_prop(attr, tokens, spaces) {
            props.push((css_name, value));
        }
    }
    if !props.is_empty() {
        rules.insert(style_class_for(element), props);
    }
    if let Some(children) = &element.children {
        collect_view(children, tokens, spaces, rules);
    }
}

pub fn layout_class(name: &str) -> Option<&'static str> {
    match name {
        "Box" => Some("l-box"),
        "Row" => Some("l-row"),
        "Column" => Some("l-col"),
        "Grid" => Some("l-grid"),
        "Stack" => Some("l-stack"),
        _ => None,
    }
}

pub fn style_class_for(element: &ElementNode) -> String {
    format!("l-s{}", element.span.start)
}

pub fn style_ref_class(name: &str) -> String {
    format!("l-style-{}", sanitize_class(name))
}

fn style_prop(
    attr: &Attribute,
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
) -> Option<(String, String)> {
    let value = attr.value.as_ref()?;
    let css_name = css_property_name(&attr.name)?;
    let value = css_value(&attr.name, value, tokens, spaces);
    Some((css_name.into(), value))
}

fn css_value(
    name: &str,
    expr: &Expr,
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
) -> String {
    let raw = expr.raw.trim_matches('"');
    if name == "columns" && raw.chars().all(|c| c.is_ascii_digit()) {
        return format!("repeat({raw}, minmax(0, 1fr))");
    }
    if let Some(var) = token_var_for_property(name, raw, tokens) {
        return format!("var({var})");
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        spaces.insert(raw.to_string());
        format!("var(--lume-space-{raw})")
    } else {
        match raw {
            "center" => "center".into(),
            "between" => "space-between".into(),
            other => other.into(),
        }
    }
}

fn emit_style_decl(
    style: &StyleDecl,
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
    css: &mut String,
) {
    css.push_str(&format!(".{} {{\n", style_ref_class(&style.name)));
    for property in style
        .properties
        .iter()
        .filter(|property| property.pseudo.is_none() && property.media.is_none())
    {
        if let Some(css_name) = css_property_name(&property.name) {
            css.push_str(&format!(
                "  {css_name}: {};\n",
                css_value(&property.name, &property.value, tokens, spaces)
            ));
        }
    }
    css.push_str("}\n\n");
    for pseudo in ["hover", "active", "disabled"] {
        let props = style
            .properties
            .iter()
            .filter(|property| property.pseudo.as_deref() == Some(pseudo))
            .collect::<Vec<_>>();
        if props.is_empty() {
            continue;
        }
        let selector = match pseudo {
            "hover" => ":hover",
            "active" => ":active",
            "disabled" => ":disabled",
            _ => "",
        };
        css.push_str(&format!(
            ".{}{} {{\n",
            style_ref_class(&style.name),
            selector
        ));
        for property in props {
            if let Some(css_name) = css_property_name(&property.name) {
                css.push_str(&format!(
                    "  {css_name}: {};\n",
                    css_value(&property.name, &property.value, tokens, spaces)
                ));
            }
        }
        css.push_str("}\n\n");
    }
    for media in style
        .properties
        .iter()
        .filter_map(|property| property.media.as_deref())
        .collect::<BTreeSet<_>>()
    {
        let Some(query) = media_query(media) else {
            continue;
        };
        css.push_str(&format!("@media {query} {{\n"));
        css.push_str(&format!("  .{} {{\n", style_ref_class(&style.name)));
        for property in style
            .properties
            .iter()
            .filter(|property| property.media.as_deref() == Some(media))
        {
            if let Some(css_name) = css_property_name(&property.name) {
                css.push_str(&format!(
                    "    {css_name}: {};\n",
                    css_value(&property.name, &property.value, tokens, spaces)
                ));
            }
        }
        css.push_str("  }\n}\n\n");
    }
}

fn collect_style_spaces(
    styles: &[StyleDecl],
    tokens: &HashMap<String, String>,
    spaces: &mut BTreeSet<String>,
) {
    for style in styles {
        for property in &style.properties {
            let _ = css_value(&property.name, &property.value, tokens, spaces);
        }
    }
}

fn css_property_name(name: &str) -> Option<&'static str> {
    match name {
        "gap" => Some("gap"),
        "padding" => Some("padding"),
        "margin" => Some("margin"),
        "width" => Some("width"),
        "height" => Some("height"),
        "align" => Some("align-items"),
        "justify" => Some("justify-content"),
        "columns" => Some("grid-template-columns"),
        "background" => Some("background"),
        "color" => Some("color"),
        "radius" => Some("border-radius"),
        "fontSize" => Some("font-size"),
        "weight" => Some("font-weight"),
        _ => None,
    }
}

fn theme_tokens(themes: &[ThemeDecl]) -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    for theme in themes {
        for token in &theme.tokens {
            tokens.insert(
                format!("{}:{}", token.category, token.name),
                format!("--lume-{}-{}", token.category, token.name),
            );
        }
    }
    tokens
}

fn token_var_for_property<'a>(
    property: &str,
    name: &str,
    tokens: &'a HashMap<String, String>,
) -> Option<&'a String> {
    let preferred = match property {
        "padding" | "margin" | "gap" | "width" | "height" | "fontSize" => {
            ["space", "breakpoint", "radius", "color"]
        }
        "radius" => ["radius", "space", "breakpoint", "color"],
        "background" | "color" => ["color", "space", "radius", "breakpoint"],
        _ => ["space", "color", "radius", "breakpoint"],
    };
    preferred
        .iter()
        .find_map(|category| tokens.get(&format!("{category}:{name}")))
}

fn css_token_value(category: &str, value: &Expr) -> String {
    let raw = value.raw.trim_matches('"');
    if matches!(category, "space" | "radius" | "breakpoint")
        && raw.chars().all(|ch| ch.is_ascii_digit())
    {
        format!("{raw}px")
    } else {
        raw.to_string()
    }
}

fn sanitize_class(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn media_query(name: &str) -> Option<&'static str> {
    match name {
        "sm" => Some("(min-width: 640px)"),
        "md" => Some("(min-width: 768px)"),
        "lg" => Some("(min-width: 1024px)"),
        "xl" => Some("(min-width: 1280px)"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::generate;
    use lume_hir::lower;
    use lume_ir::build;
    use lume_parser::parse;

    #[test]
    fn lowers_theme_tokens_and_style_declarations() {
        let source = r##"
theme default {
  color surface = "#ffffff"
  color text = "#111827"
  space md = 16
  radius md = 12
}

style card {
  padding: md
  background: surface
  color: text
  radius: md

  hover {
    background: text
  }

  at md {
    padding: 24
  }
}

component App {
  view {
    Box style=card {
      Text("Hello")
    }
  }
}
"##;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let css = generate(&ir);
        assert!(css.contains("--lume-color-surface: #ffffff;"));
        assert!(css.contains("--lume-space-md: 16px;"));
        assert!(css.contains(".l-style-card"));
        assert!(css.contains("padding: var(--lume-space-md);"));
        assert!(css.contains("background: var(--lume-color-surface);"));
        assert!(css.contains("border-radius: var(--lume-radius-md);"));
        assert!(css.contains(".l-style-card:hover"));
        assert!(css.contains("@media (min-width: 768px)"));
    }
}
