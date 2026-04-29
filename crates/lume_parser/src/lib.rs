use lume_ast::*;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_lexer::{lex, Token, TokenKind};
use lume_span::Span;

pub fn parse(source: &str) -> (Program, Diagnostics) {
    let (tokens, mut diagnostics) = lex(source);
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Diagnostics::default(),
    };
    let program = parser.program();
    diagnostics.extend(parser.diagnostics.into_vec());
    (program, diagnostics)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Diagnostics,
}

impl Parser {
    fn program(&mut self) -> Program {
        let start = self.current().span.start;
        let mut declarations = Vec::new();
        while !self.at_eof() {
            match self.decl() {
                Some(decl) => declarations.push(decl),
                None => self.synchronize_top_level(),
            }
        }
        let end = self.current().span.end;
        Program {
            declarations,
            span: Span::new(start, end),
        }
    }

    fn decl(&mut self) -> Option<Decl> {
        if self.eat_keyword("module") {
            let name = self.ident_or_keyword()?;
            self.eat_keyword("strict");
            return Some(Decl::Module(name));
        }
        if self.eat_keyword("import") {
            return self.import_decl();
        }
        if self.eat_keyword("export") {
            let inner = self.decl()?;
            return Some(Decl::Export(Box::new(inner)));
        }
        if self.eat_keyword("component") {
            return self.component_like("component").map(Decl::Component);
        }
        if self.eat_keyword("page") {
            return self.component_like("page").map(Decl::Page);
        }
        if self.eat_keyword("layout") {
            return self.component_like("layout").map(Decl::Layout);
        }
        if self.eat_keyword("route") {
            return self.route_decl().map(Decl::Route);
        }
        if self.eat_keyword("style") {
            return self.style_decl().map(Decl::Style);
        }
        if self.eat_keyword("theme") {
            return self.theme_decl().map(Decl::Theme);
        }
        if self.eat_keyword("type") {
            return Some(Decl::Type(self.reserved_named("type")));
        }
        if self.eat_keyword("app") {
            return Some(Decl::App(self.reserved_named("app")));
        }
        if self.eat_keyword("server") {
            self.eat_keyword("action");
            return Some(Decl::ServerAction(self.reserved_named("server action")));
        }
        if self.eat_keyword("form") {
            return Some(Decl::Form(self.reserved_named("form")));
        }
        if self.eat_keyword("ffi")
            || self.eat_keyword("struct")
            || self.eat_keyword("enum")
            || self.eat_keyword("opaque")
        {
            return Some(Decl::Ffi(self.reserved_named("ffi")));
        }
        self.error_here("LUME2001", "expected top-level declaration");
        None
    }

    fn import_decl(&mut self) -> Option<Decl> {
        let mut items = Vec::new();
        if self.eat_symbol('{') {
            while !self.at_eof() && !self.eat_symbol('}') {
                if let Some(item) = self.ident_or_keyword() {
                    items.push(item);
                }
                self.eat_symbol(',');
            }
        } else if let Some(item) = self.ident_or_keyword() {
            items.push(item);
        }
        self.expect_keyword("from");
        let from = self.string()?;
        Some(Decl::Import { items, from })
    }

    fn component_like(&mut self, _kind: &str) -> Option<ComponentDecl> {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword()?;
        let params = if self.check_symbol('(') {
            self.params()
        } else {
            Vec::new()
        };
        self.expect_symbol('{');
        let mut items = Vec::new();
        while !self.at_eof() && !self.eat_symbol('}') {
            if self.eat_keyword("state") {
                if let Some(state) = self.state_decl() {
                    items.push(ComponentItem::State(state));
                }
            } else if self.eat_keyword("derived") {
                let span = self.previous().span;
                if let Some(name) = self.ident_or_keyword() {
                    self.expect_operator("=");
                    let expr = self.expr_until(&["}", "state", "derived", "action", "view"]);
                    items.push(ComponentItem::Derived { name, expr, span });
                }
            } else if self.eat_keyword("async") || self.check_keyword("action") {
                let is_async = self.previous_is_keyword("async");
                if is_async {
                    self.expect_keyword("action");
                } else {
                    self.eat_keyword("action");
                }
                if let Some(action) = self.action_decl(is_async) {
                    items.push(ComponentItem::Action(action));
                }
            } else if self.eat_keyword("view") {
                if let Some(view) = self.view_block() {
                    items.push(ComponentItem::View(view));
                }
            } else {
                let name = self.ident_or_keyword();
                let span = self.previous().span;
                if self.check_symbol('{') {
                    self.skip_balanced_block();
                } else {
                    self.advance();
                }
                items.push(ComponentItem::Reserved(ReservedDecl {
                    kind: "component item".into(),
                    name,
                    span,
                }));
            }
        }
        Some(ComponentDecl {
            name,
            params,
            items,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn state_decl(&mut self) -> Option<StateDecl> {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword()?;
        self.expect_symbol(':');
        let ty = self.ident_or_keyword()?;
        self.expect_operator("=");
        let init = self.expr_until(&["}", "state", "derived", "action", "view"]);
        Some(StateDecl {
            name,
            ty,
            init,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn action_decl(&mut self, is_async: bool) -> Option<ActionDecl> {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword()?;
        let params = if self.check_symbol('(') {
            self.params()
        } else {
            Vec::new()
        };
        let body = self.block()?;
        Some(ActionDecl {
            name,
            params,
            body,
            is_async,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        self.expect_symbol('(');
        while !self.at_eof() && !self.eat_symbol(')') {
            let start = self.current().span.start;
            let Some(name) = self.ident_or_keyword() else {
                break;
            };
            self.expect_symbol(':');
            let ty = self.ident_or_keyword().unwrap_or_else(|| "Unknown".into());
            let default = if self.eat_operator("=") {
                Some(self.expr_until(&[",", ")"]))
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                default,
                span: Span::new(start, self.previous().span.end),
            });
            self.eat_symbol(',');
        }
        params
    }

    fn block(&mut self) -> Option<Block> {
        let start = self.current().span.start;
        self.expect_symbol('{');
        let mut statements = Vec::new();
        while !self.at_eof() && !self.eat_symbol('}') {
            if let Some(stmt) = self.stmt() {
                statements.push(stmt);
            } else {
                self.advance();
            }
            self.eat_symbol(';');
        }
        Some(Block {
            statements,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn stmt(&mut self) -> Option<Stmt> {
        let start = self.current().span.start;
        if let Some(target) = self.ident_or_keyword() {
            if self.eat_operator("+=") {
                let expr = self.expr_until(&[";", "}"]);
                return Some(Stmt::Assign {
                    target,
                    op: AssignOp::Add,
                    expr,
                    span: Span::new(start, self.previous().span.end),
                });
            }
            if self.eat_operator("-=") {
                let expr = self.expr_until(&[";", "}"]);
                return Some(Stmt::Assign {
                    target,
                    op: AssignOp::Sub,
                    expr,
                    span: Span::new(start, self.previous().span.end),
                });
            }
            if self.eat_operator("=") {
                let expr = self.expr_until(&[";", "}"]);
                return Some(Stmt::Assign {
                    target,
                    op: AssignOp::Set,
                    expr,
                    span: Span::new(start, self.previous().span.end),
                });
            }
            let mut raw = target;
            raw.push_str(&self.collect_raw_until(&[";", "}"]));
            return Some(Stmt::Expr(Expr {
                raw: raw.trim().into(),
                span: Span::new(start, self.previous().span.end),
            }));
        }
        None
    }

    fn view_block(&mut self) -> Option<ViewBlock> {
        let start = self.current().span.start;
        self.expect_symbol('{');
        let mut nodes = Vec::new();
        while !self.at_eof() && !self.eat_symbol('}') {
            if let Some(node) = self.view_node() {
                nodes.push(node);
            } else {
                self.advance();
            }
        }
        Some(ViewBlock {
            nodes,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn view_node(&mut self) -> Option<ViewNode> {
        if self.eat_keyword("if") {
            return self.if_node().map(ViewNode::If);
        }
        if self.eat_keyword("for") {
            return self.for_node().map(ViewNode::For);
        }
        if self.eat_keyword("slot") {
            let span = self.previous().span;
            if self.eat_symbol(':') {
                let name = self.ident_or_keyword()?;
                let body = self.view_block()?;
                return Some(ViewNode::SlotFill { name, span, body });
            }
            let name = self.ident_or_keyword();
            return Some(ViewNode::SlotUse { name, span });
        }
        if self.eat_keyword("on") {
            return self.event_node().map(ViewNode::Event);
        }
        if self.eat_keyword("match") {
            let span = self.previous().span;
            self.skip_balanced_block();
            return Some(ViewNode::Match(ReservedDecl {
                kind: "match".into(),
                name: None,
                span,
            }));
        }
        self.element_node().map(ViewNode::Element)
    }

    fn if_node(&mut self) -> Option<IfNode> {
        let start = self.previous().span.start;
        let condition = self.expr_until(&["{"]);
        let then_block = self.view_block()?;
        let else_block = if self.eat_keyword("else") {
            if self.eat_keyword("if") {
                let nested = self.if_node()?;
                Some(ViewBlock {
                    nodes: vec![ViewNode::If(nested.clone())],
                    span: nested.span,
                })
            } else {
                self.view_block()
            }
        } else {
            None
        };
        Some(IfNode {
            condition,
            then_block,
            else_block,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn for_node(&mut self) -> Option<ForNode> {
        let start = self.previous().span.start;
        let item = self.ident_or_keyword()?;
        let index = if self.eat_symbol(',') {
            self.ident_or_keyword()
        } else {
            None
        };
        self.expect_keyword("in");
        let iterable = self.expr_until(&["key", "{"]);
        let key = if self.eat_keyword("key") {
            self.expect_operator("=");
            Some(self.expr_until(&["{"]))
        } else {
            None
        };
        let body = self.view_block()?;
        Some(ForNode {
            item,
            index,
            iterable,
            key,
            body,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn event_node(&mut self) -> Option<EventNode> {
        let start = self.previous().span.start;
        let event = self.ident_or_keyword()?;
        let params = if self.check_symbol('(') {
            self.expect_symbol('(');
            let mut params = Vec::new();
            while !self.at_eof() && !self.eat_symbol(')') {
                if let Some(param) = self.ident_or_keyword() {
                    params.push(param);
                }
                self.eat_symbol(',');
            }
            params
        } else {
            Vec::new()
        };
        let body = self.block()?;
        Some(EventNode {
            event,
            params,
            body,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn element_node(&mut self) -> Option<ElementNode> {
        let start = self.current().span.start;
        let name = self.ident_or_keyword()?;
        let args = if self.check_symbol('(') {
            self.arg_list()
        } else {
            Vec::new()
        };
        let mut attrs = Vec::new();
        while self.looks_like_attr() {
            let attr_start = self.current().span.start;
            let Some(attr_name) = self.ident_or_keyword() else {
                break;
            };
            let value = if self.eat_operator("=") {
                Some(self.expr_atom())
            } else {
                None
            };
            attrs.push(Attribute {
                name: attr_name,
                value,
                span: Span::new(attr_start, self.previous().span.end),
            });
        }
        let children = if self.check_symbol('{') {
            self.view_block()
        } else {
            None
        };
        Some(ElementNode {
            name,
            args,
            attrs,
            children,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn arg_list(&mut self) -> Vec<Arg> {
        let mut args = Vec::new();
        self.expect_symbol('(');
        while !self.at_eof() && !self.eat_symbol(')') {
            if self.is_identish() && self.peek_operator("=") {
                let name = self.ident_or_keyword().unwrap();
                self.expect_operator("=");
                args.push(Arg::Named(name, self.expr_until(&[",", ")"])));
            } else {
                args.push(Arg::Positional(self.expr_until(&[",", ")"])));
            }
            self.eat_symbol(',');
        }
        args
    }

    fn style_decl(&mut self) -> Option<StyleDecl> {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword()?;
        self.expect_symbol('{');
        let mut properties = Vec::new();
        while !self.at_eof() && !self.eat_symbol('}') {
            let prop_start = self.current().span.start;
            if let Some(name) = self.ident_or_keyword() {
                if self.eat_symbol(':') {
                    let value = self.expr_atom();
                    properties.push(StyleProperty {
                        name,
                        value,
                        pseudo: None,
                        media: None,
                        span: Span::new(prop_start, self.previous().span.end),
                    });
                } else if self.check_symbol('{') {
                    let block_name = name;
                    self.expect_symbol('{');
                    while !self.at_eof() && !self.eat_symbol('}') {
                        let nested_start = self.current().span.start;
                        if let Some(name) = self.ident_or_keyword() {
                            if self.eat_symbol(':') {
                                let value = self.expr_atom();
                                properties.push(StyleProperty {
                                    name,
                                    value,
                                    pseudo: Some(block_name.clone()),
                                    media: None,
                                    span: Span::new(nested_start, self.previous().span.end),
                                });
                            }
                        } else {
                            self.advance();
                        }
                    }
                } else if name == "at" {
                    let Some(media) = self.ident_or_keyword() else {
                        continue;
                    };
                    self.expect_symbol('{');
                    while !self.at_eof() && !self.eat_symbol('}') {
                        let nested_start = self.current().span.start;
                        if let Some(name) = self.ident_or_keyword() {
                            if self.eat_symbol(':') {
                                let value = self.expr_atom();
                                properties.push(StyleProperty {
                                    name,
                                    value,
                                    pseudo: None,
                                    media: Some(media.clone()),
                                    span: Span::new(nested_start, self.previous().span.end),
                                });
                            }
                        } else {
                            self.advance();
                        }
                    }
                }
            } else {
                self.advance();
            }
        }
        Some(StyleDecl {
            name,
            properties,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn theme_decl(&mut self) -> Option<ThemeDecl> {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword()?;
        self.expect_symbol('{');
        let mut tokens = Vec::new();
        while !self.at_eof() && !self.eat_symbol('}') {
            let token_start = self.current().span.start;
            let Some(category) = self.ident_or_keyword() else {
                self.advance();
                continue;
            };
            if self.check_symbol('{') {
                self.skip_balanced_block();
                continue;
            }
            let Some(name) = self.ident_or_keyword() else {
                self.advance();
                continue;
            };
            self.expect_operator("=");
            let value = self.expr_atom();
            tokens.push(ThemeToken {
                category,
                name,
                value,
                span: Span::new(token_start, self.previous().span.end),
            });
        }
        Some(ThemeDecl {
            name,
            tokens,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn route_decl(&mut self) -> Option<RouteDecl> {
        let start = self.previous().span.start;
        let path = self.string()?;
        let view = if self.check_symbol('{') {
            self.view_block()
        } else {
            None
        };
        Some(RouteDecl {
            path,
            view,
            span: Span::new(start, self.previous().span.end),
        })
    }

    fn reserved_named(&mut self, kind: &str) -> ReservedDecl {
        let start = self.previous().span.start;
        let name = self.ident_or_keyword();
        if self.check_symbol('{') {
            self.skip_balanced_block();
        }
        ReservedDecl {
            kind: kind.into(),
            name,
            span: Span::new(start, self.previous().span.end),
        }
    }

    fn expr_until(&mut self, stops: &[&str]) -> Expr {
        let start = self.current().span.start;
        let raw = self.collect_raw_until(stops);
        Expr {
            raw: raw.trim().to_string(),
            span: Span::new(start, self.previous().span.end),
        }
    }

    fn expr_atom(&mut self) -> Expr {
        let start = self.current().span.start;
        let mut raw = String::new();
        if self.eat_symbol('(') {
            raw.push('(');
            raw.push_str(&self.collect_raw_until(&[")"]));
            self.eat_symbol(')');
            raw.push(')');
        } else {
            raw.push_str(&token_text(self.current()));
            self.advance();
        }
        Expr {
            raw: raw.trim().to_string(),
            span: Span::new(start, self.previous().span.end),
        }
    }

    fn collect_raw_until(&mut self, stops: &[&str]) -> String {
        let mut raw = String::new();
        let mut depth = 0usize;
        while !self.at_eof() {
            if depth == 0 && self.is_stop(stops) {
                break;
            }
            match &self.current().kind {
                TokenKind::Symbol('(') | TokenKind::Symbol('[') => depth += 1,
                TokenKind::Symbol(')') | TokenKind::Symbol(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            if !raw.is_empty() && needs_space(&raw, self.current()) {
                raw.push(' ');
            }
            raw.push_str(&token_text(self.current()));
            self.advance();
        }
        raw
    }

    fn skip_balanced_block(&mut self) {
        if !self.eat_symbol('{') {
            return;
        }
        let mut depth = 1usize;
        while !self.at_eof() && depth > 0 {
            if self.eat_symbol('{') {
                depth += 1;
            } else if self.eat_symbol('}') {
                depth -= 1;
            } else {
                self.advance();
            }
        }
    }

    fn synchronize_top_level(&mut self) {
        while !self.at_eof() {
            if matches!(self.current().kind, TokenKind::Keyword(_)) {
                return;
            }
            self.advance();
        }
    }

    fn looks_like_attr(&self) -> bool {
        self.is_identish() && matches!(self.peek().kind, TokenKind::Operator(ref op) if op == "=")
    }

    fn is_stop(&self, stops: &[&str]) -> bool {
        stops.iter().any(|stop| match *stop {
            "}" => self.check_symbol('}'),
            "{" => self.check_symbol('{'),
            ")" => self.check_symbol(')'),
            "," => self.check_symbol(','),
            ";" => self.check_symbol(';'),
            kw => self.check_keyword(kw),
        })
    }

    fn expect_symbol(&mut self, symbol: char) -> bool {
        if self.eat_symbol(symbol) {
            true
        } else {
            self.error_here("LUME2002", format!("expected `{symbol}`"));
            false
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> bool {
        if self.eat_keyword(keyword) {
            true
        } else {
            self.error_here("LUME2003", format!("expected keyword `{keyword}`"));
            false
        }
    }

    fn expect_operator(&mut self, operator: &str) -> bool {
        if self.eat_operator(operator) {
            true
        } else {
            self.error_here("LUME2004", format!("expected operator `{operator}`"));
            false
        }
    }

    fn string(&mut self) -> Option<String> {
        if let TokenKind::String(value) = &self.current().kind {
            let value = value.clone();
            self.advance();
            Some(value)
        } else {
            self.error_here("LUME2005", "expected string literal");
            None
        }
    }

    fn ident_or_keyword(&mut self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Ident(value) | TokenKind::Keyword(value) => {
                let value = value.clone();
                self.advance();
                Some(value)
            }
            _ => {
                self.error_here("LUME2006", "expected identifier");
                None
            }
        }
    }

    fn is_identish(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_) | TokenKind::Keyword(_)
        )
    }

    fn eat_symbol(&mut self, symbol: char) -> bool {
        if self.check_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.check_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_operator(&mut self, operator: &str) -> bool {
        if matches!(&self.current().kind, TokenKind::Operator(op) if op == operator) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_symbol(&self, symbol: char) -> bool {
        matches!(self.current().kind, TokenKind::Symbol(ch) if ch == symbol)
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Keyword(value) if value == keyword)
    }

    fn previous_is_keyword(&self, keyword: &str) -> bool {
        self.pos > 0
            && matches!(&self.previous().kind, TokenKind::Keyword(value) if value == keyword)
    }

    fn peek_operator(&self, operator: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Operator(op) if op == operator)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos + 1)
            .unwrap_or_else(|| self.current())
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn error_here(&mut self, code: &'static str, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, message, Some(self.current().span)));
    }
}

fn token_text(token: &Token) -> String {
    match &token.kind {
        TokenKind::Ident(value)
        | TokenKind::Number(value)
        | TokenKind::Keyword(value)
        | TokenKind::Operator(value) => value.clone(),
        TokenKind::String(value) => format!("{value:?}"),
        TokenKind::Symbol(ch) => ch.to_string(),
        TokenKind::Eof => String::new(),
    }
}

fn needs_space(raw: &str, token: &Token) -> bool {
    let Some(last) = raw.chars().last() else {
        return false;
    };
    let next = token_text(token).chars().next().unwrap_or(' ');
    (last.is_alphanumeric() || last == '_' || last == '"')
        && (next.is_alphanumeric() || next == '_' || next == '"')
}

#[cfg(test)]
mod tests {
    use super::parse;
    use lume_ast::{ComponentItem, Decl, ViewNode};

    #[test]
    fn parses_counter_component() {
        let source = r#"
component App {
  state count: i32 = 0

  view {
    Column gap=12 padding=16 {
      Text("Count: {count}")
      Button("増やす") {
        on click {
          count += 1
        }
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(
            !diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            diagnostics.as_slice()
        );
        let Decl::Component(component) = &program.declarations[0] else {
            panic!("expected component");
        };
        assert_eq!(component.name, "App");
        assert!(component
            .items
            .iter()
            .any(|item| matches!(item, ComponentItem::State(state) if state.name == "count")));
    }

    #[test]
    fn parses_input_event_param() {
        let source = r#"
component App {
  state name: String = ""

  view {
    Input(value=name, label="Name") {
      on input(value) {
        name = value
      }
    }
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let Decl::Component(component) = &program.declarations[0] else {
            panic!("expected component");
        };
        let view = component.items.iter().find_map(|item| match item {
            ComponentItem::View(view) => Some(view),
            _ => None,
        });
        let Some(view) = view else {
            panic!("expected view");
        };
        let ViewNode::Element(input) = &view.nodes[0] else {
            panic!("expected input element");
        };
        let Some(children) = &input.children else {
            panic!("expected input children");
        };
        let ViewNode::Event(event) = &children.nodes[0] else {
            panic!("expected event");
        };
        assert_eq!(event.event, "input");
        assert_eq!(event.params, vec!["value"]);
    }
}
