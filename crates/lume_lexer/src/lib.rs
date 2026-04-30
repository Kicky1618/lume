use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Number(String),
    String(String),
    Keyword(String),
    Symbol(char),
    Operator(String),
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub line_break_before: bool,
}

const KEYWORDS: &[&str] = &[
    "app",
    "module",
    "package",
    "import",
    "export",
    "type",
    "component",
    "page",
    "route",
    "layout",
    "router",
    "server",
    "metadata",
    "search",
    "form",
    "validate",
    "query",
    "mutation",
    "action",
    "effect",
    "state",
    "derived",
    "view",
    "style",
    "theme",
    "slot",
    "if",
    "else",
    "for",
    "in",
    "match",
    "case",
    "default",
    "return",
    "true",
    "false",
    "null",
    "undefined",
    "async",
    "await",
    "unsafe",
    "ffi",
    "struct",
    "enum",
    "trait",
    "opaque",
    "i18n",
    "from",
    "on",
    "key",
];

pub fn lex(source: &str) -> (Vec<Token>, Diagnostics) {
    let mut lexer = Lexer {
        source,
        pos: 0,
        diagnostics: Diagnostics::default(),
        tokens: Vec::new(),
        line_break_pending: false,
    };
    lexer.run();
    (lexer.tokens, lexer.diagnostics)
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    diagnostics: Diagnostics,
    tokens: Vec<Token>,
    line_break_pending: bool,
}

impl Lexer<'_> {
    fn run(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                if ch == '\n' || ch == '\r' {
                    self.line_break_pending = true;
                }
                self.bump();
                continue;
            }
            if ch == '/' && self.peek_next() == Some('/') {
                self.skip_line_comment();
                continue;
            }
            if ch == '/' && self.peek_next() == Some('*') {
                self.skip_block_comment();
                continue;
            }
            let start = self.pos;
            let line_break_before = self.line_break_pending;
            self.line_break_pending = false;
            match ch {
                '"' | '\'' => self.string(ch, start, line_break_before),
                '0'..='9' => self.number(start, line_break_before),
                c if is_ident_start(c) => self.ident(start, line_break_before),
                '{' | '}' | '(' | ')' | '[' | ']' | ',' | ':' | ';' | '.' => {
                    self.bump();
                    self.tokens.push(Token {
                        kind: TokenKind::Symbol(ch),
                        span: Span::new(start, self.pos),
                        line_break_before,
                    });
                }
                '+' | '-' | '*' | '/' | '=' | '!' | '<' | '>' | '&' | '|' | '?' => {
                    self.operator(start, line_break_before)
                }
                _ => {
                    self.bump();
                    self.diagnostics.push(Diagnostic::error(
                        "LUME1001",
                        format!("unexpected character `{ch}`"),
                        Some(Span::new(start, self.pos)),
                    ));
                }
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.pos, self.pos),
            line_break_before: self.line_break_pending,
        });
    }

    fn ident(&mut self, start: usize, line_break_before: bool) {
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        let text = &self.source[start..self.pos];
        let kind = if KEYWORDS.contains(&text) {
            TokenKind::Keyword(text.to_string())
        } else {
            TokenKind::Ident(text.to_string())
        };
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.pos),
            line_break_before,
        });
    }

    fn number(&mut self, start: usize, line_break_before: bool) {
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        {
            self.bump();
        }
        self.tokens.push(Token {
            kind: TokenKind::Number(self.source[start..self.pos].to_string()),
            span: Span::new(start, self.pos),
            line_break_before,
        });
    }

    fn string(&mut self, quote: char, start: usize, line_break_before: bool) {
        self.bump();
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == quote {
                self.bump();
                self.tokens.push(Token {
                    kind: TokenKind::String(value),
                    span: Span::new(start, self.pos),
                    line_break_before,
                });
                return;
            }
            if ch == '\\' {
                self.bump();
                if let Some(escaped) = self.bump() {
                    value.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\'' => '\'',
                        '\\' => '\\',
                        other => other,
                    });
                }
            } else {
                value.push(ch);
                self.bump();
            }
        }
        self.diagnostics.push(Diagnostic::error(
            "LUME1002",
            "unterminated string literal",
            Some(Span::new(start, self.pos)),
        ));
    }

    fn operator(&mut self, start: usize, line_break_before: bool) {
        self.line_break_pending = false;
        let first = self.bump().unwrap();
        let mut op = String::from(first);
        while let Some(next) = self.peek() {
            let trial = format!("{op}{next}");
            if matches!(
                trial.as_str(),
                "+=" | "-="
                    | "=="
                    | "!="
                    | "==="
                    | "!=="
                    | "<="
                    | ">="
                    | "&&"
                    | "||"
                    | "??"
                    | "?."
                    | "=>"
            ) {
                op.push(next);
                self.bump();
            } else {
                break;
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Operator(op),
            span: Span::new(start, self.pos),
            line_break_before,
        });
    }

    fn skip_line_comment(&mut self) {
        while self.peek().is_some_and(|ch| ch != '\n') {
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) {
        let start = self.pos;
        let mut saw_line_break = false;
        self.bump();
        self.bump();
        while let Some(ch) = self.bump() {
            if ch == '\n' || ch == '\r' {
                saw_line_break = true;
            }
            if ch == '*' && self.peek() == Some('/') {
                self.bump();
                if saw_line_break {
                    self.line_break_pending = true;
                }
                return;
            }
        }
        if saw_line_break {
            self.line_break_pending = true;
        }
        self.diagnostics.push(Diagnostic::error(
            "LUME1003",
            "unterminated block comment",
            Some(Span::new(start, self.pos)),
        ));
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch == '-' || ch.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::{lex, TokenKind};

    #[test]
    fn marks_tokens_after_newlines() {
        let source = "result = await prepareRender(prompt, width, height)\nstatus = result";
        let (tokens, diagnostics) = lex(source);
        assert!(
            !diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            diagnostics.as_slice()
        );
        let status = tokens
            .iter()
            .find(|token| matches!(&token.kind, TokenKind::Ident(value) if value == "status"))
            .expect("expected status token");
        assert!(status.line_break_before);
    }
}
