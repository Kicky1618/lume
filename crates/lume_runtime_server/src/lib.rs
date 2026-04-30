use lume_ast::{ServerActionDecl, Stmt};
use lume_backend_jit::JitBackend;
use std::collections::HashMap;

pub const RUNTIME_NAME: &str = "lume-runtime-server";

#[derive(Clone, Debug)]
pub struct ServerRuntime {
    actions: HashMap<String, ServerActionDecl>,
    jit: JitBackend,
}

#[derive(Clone, Debug)]
pub struct ActionRequestContext {
    pub csrf_token: Option<String>,
    pub authenticated: bool,
    pub rate_limit_exceeded: bool,
}

impl Default for ActionRequestContext {
    fn default() -> Self {
        Self {
            csrf_token: Some("dev-csrf-token".into()),
            authenticated: false,
            rate_limit_exceeded: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionValue {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<ActionValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionError {
    pub status: u16,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionResult {
    pub value: ActionValue,
    pub runtime: ActionRuntime,
    pub revalidate: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionRuntime {
    Interpreter,
    LlvmJit,
}

impl ServerRuntime {
    pub fn new(actions: Vec<ServerActionDecl>) -> Self {
        Self {
            actions: actions
                .into_iter()
                .map(|action| (action.name.clone(), action))
                .collect(),
            jit: JitBackend::new(),
        }
    }

    pub fn call_json(&self, id: &str, body: &str) -> Result<String, ActionError> {
        self.call_json_with_context(id, body, &ActionRequestContext::default())
    }

    pub fn call_json_with_context(
        &self,
        id: &str,
        body: &str,
        context: &ActionRequestContext,
    ) -> Result<String, ActionError> {
        let action = self.action(id)?;
        enforce_action_guards(action, context)?;
        if let Some(max_body_size) = action_max_body_size(action)? {
            if body.len() > max_body_size {
                return Err(ActionError {
                    status: 413,
                    message: format!(
                        "Server Action `{}` request body exceeds maxBodySize {}",
                        action.name, max_body_size
                    ),
                });
            }
        }
        let args = parse_call_args(body)?;
        let result = self.call_with_context(id, args, context)?;
        let mut response = format!(
            "{{\"value\":{},\"runtime\":\"{}\"",
            result.value.to_json(),
            result.runtime.as_str()
        );
        if !result.revalidate.is_empty() {
            response.push_str(&format!(
                ",\"revalidate\":[{}]",
                result
                    .revalidate
                    .iter()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        response.push('}');
        Ok(response)
    }

    pub fn call(&self, id: &str, args: Vec<ActionValue>) -> Result<ActionResult, ActionError> {
        self.call_with_context(id, args, &ActionRequestContext::default())
    }

    pub fn call_with_context(
        &self,
        id: &str,
        args: Vec<ActionValue>,
        context: &ActionRequestContext,
    ) -> Result<ActionResult, ActionError> {
        let action = self.action(id)?;
        enforce_action_guards(action, context)?;
        self.validate_args(action, &args)?;
        validate_action_rules(action, &args)?;
        if action_runtime(action).as_deref() == Some("jit") {
            if let Some(numeric_args) = all_i64_args(&args) {
                if self.jit.is_available() {
                    let output =
                        self.jit
                            .execute_i64(action, &numeric_args)
                            .map_err(|err| ActionError {
                                status: 500,
                                message: err.message,
                            })?;
                    return Ok(ActionResult {
                        value: ActionValue::Number(output.value),
                        runtime: ActionRuntime::LlvmJit,
                        revalidate: action_revalidations(action),
                    });
                }
            }
        }
        let value = execute_interpreted(action, &args)?;
        validate_return(action, &value)?;
        Ok(ActionResult {
            value,
            runtime: ActionRuntime::Interpreter,
            revalidate: action_revalidations(action),
        })
    }

    fn action(&self, id: &str) -> Result<&ServerActionDecl, ActionError> {
        self.actions.get(id).ok_or_else(|| ActionError {
            status: 404,
            message: format!("unknown Server Action `{id}`"),
        })
    }

    fn validate_args(
        &self,
        action: &ServerActionDecl,
        args: &[ActionValue],
    ) -> Result<(), ActionError> {
        if action.params.len() != args.len() {
            return Err(ActionError {
                status: 400,
                message: format!(
                    "Server Action `{}` expected {} args, got {}",
                    action.name,
                    action.params.len(),
                    args.len()
                ),
            });
        }
        for (param, value) in action.params.iter().zip(args) {
            if !value_matches_type(value, &param.ty) {
                return Err(ActionError {
                    status: 400,
                    message: format!(
                        "Server Action `{}` argument `{}` expected {}, got {}",
                        action.name,
                        param.name,
                        param.ty,
                        value.kind()
                    ),
                });
            }
        }
        Ok(())
    }
}

impl ActionRuntime {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Interpreter => "interpreter",
            Self::LlvmJit => "llvm-jit",
        }
    }
}

impl ActionValue {
    fn to_json(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::String(value) => format!("\"{}\"", escape_json(value)),
            Self::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(ActionValue::to_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    fn to_text(&self) -> String {
        match self {
            Self::Null | Self::Array(_) => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::String(value) => value.clone(),
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Number(_) => "Number",
            Self::String(_) => "String",
            Self::Array(_) => "Array",
        }
    }
}

fn action_runtime(action: &ServerActionDecl) -> Option<String> {
    modifier_value(action, "runtime")
}

fn enforce_action_guards(
    action: &ServerActionDecl,
    context: &ActionRequestContext,
) -> Result<(), ActionError> {
    if modifier_value(action, "auth").is_some_and(|value| value != "optional")
        && !context.authenticated
    {
        return Err(ActionError {
            status: 401,
            message: format!("Server Action `{}` requires auth", action.name),
        });
    }
    if modifier_value(action, "csrf")
        .map(|value| value != "false")
        .unwrap_or(true)
        && context.csrf_token.as_deref() != Some("dev-csrf-token")
    {
        return Err(ActionError {
            status: 403,
            message: format!("Server Action `{}` failed csrf validation", action.name),
        });
    }
    if modifier_value(action, "rateLimit").is_some() && context.rate_limit_exceeded {
        return Err(ActionError {
            status: 429,
            message: format!("Server Action `{}` exceeded rateLimit", action.name),
        });
    }
    Ok(())
}

fn validate_action_rules(action: &ServerActionDecl, args: &[ActionValue]) -> Result<(), ActionError> {
    if !action.modifiers.iter().any(|modifier| modifier.name == "validate") {
        return Ok(());
    }
    for (param, value) in action.params.iter().zip(args) {
        if matches!(value, ActionValue::Null) || matches!(value, ActionValue::String(text) if text.trim().is_empty()) {
            return Err(ActionError {
                status: 400,
                message: format!(
                    "Server Action `{}` validation failed for `{}`",
                    action.name, param.name
                ),
            });
        }
    }
    Ok(())
}

fn action_revalidations(action: &ServerActionDecl) -> Vec<String> {
    let mut values = Vec::new();
    for name in ["revalidate", "invalidates"] {
        if let Some(value) = modifier_value(action, name) {
            values.push(value);
        }
    }
    for stmt in &action.body.statements {
        if let Stmt::Expr(expr) = stmt {
            let raw = expr.raw.trim();
            if let Some(inner) = raw
                .strip_prefix("revalidate(")
                .and_then(|value| value.strip_suffix(')'))
            {
                values.push(inner.trim().trim_matches(['"', '\'']).to_string());
            }
        }
    }
    values
}

fn action_max_body_size(action: &ServerActionDecl) -> Result<Option<usize>, ActionError> {
    modifier_value(action, "maxBodySize")
        .map(|value| {
            value.parse::<usize>().map_err(|_| ActionError {
                status: 500,
                message: format!(
                    "Server Action `{}` has invalid maxBodySize `{}`",
                    action.name, value
                ),
            })
        })
        .transpose()
}

fn modifier_value(action: &ServerActionDecl, name: &str) -> Option<String> {
    action
        .modifiers
        .iter()
        .find(|modifier| modifier.name == name)
        .and_then(|modifier| modifier.value.as_deref())
        .map(normalize_modifier_value)
}

fn normalize_modifier_value(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).to_string()
}

fn all_i64_args(args: &[ActionValue]) -> Option<Vec<i64>> {
    args.iter().map(ActionValue::as_i64).collect()
}

fn execute_interpreted(
    action: &ServerActionDecl,
    args: &[ActionValue],
) -> Result<ActionValue, ActionError> {
    let mut locals = action
        .params
        .iter()
        .zip(args.iter())
        .map(|(param, value)| (param.name.clone(), value.clone()))
        .collect::<HashMap<_, _>>();
    for stmt in &action.body.statements {
        match stmt {
            Stmt::Assign {
                target, op, expr, ..
            } => {
                let value = eval_expr(&expr.raw, &locals).map_err(action_eval_error)?;
                let next = match op {
                    lume_ast::AssignOp::Set => value,
                    lume_ast::AssignOp::Add => {
                        let current = locals.get(target).cloned().unwrap_or(ActionValue::Null);
                        eval_action_binary(current, '+', value).map_err(action_eval_error)?
                    }
                    lume_ast::AssignOp::Sub => {
                        let current = locals.get(target).cloned().unwrap_or(ActionValue::Null);
                        eval_action_binary(current, '-', value).map_err(action_eval_error)?
                    }
                };
                locals.insert(target.clone(), next);
            }
            Stmt::Expr(expr) => {
                let raw = expr.raw.trim();
                if let Some(return_expr) = raw.strip_prefix("return").map(str::trim) {
                    return eval_expr(return_expr, &locals).map_err(action_eval_error);
                }
                eval_expr(raw, &locals).map_err(action_eval_error)?;
            }
        }
    }
    Ok(ActionValue::Null)
}

fn action_eval_error(message: String) -> ActionError {
    ActionError {
        status: 500,
        message,
    }
}

fn eval_expr(raw: &str, locals: &HashMap<String, ActionValue>) -> Result<ActionValue, String> {
    let raw = strip_parens(raw.trim());
    if raw.is_empty() {
        return Ok(ActionValue::Null);
    }
    if let Some(value) = locals.get(raw) {
        return Ok(value.clone());
    }
    if is_string_literal(raw) {
        return Ok(ActionValue::String(unquote(raw)));
    }
    if raw == "true" {
        return Ok(ActionValue::Bool(true));
    }
    if raw == "false" {
        return Ok(ActionValue::Bool(false));
    }
    if raw == "null" {
        return Ok(ActionValue::Null);
    }
    if let Ok(number) = raw.parse::<i64>() {
        return Ok(ActionValue::Number(number));
    }
    if let Some(value) = eval_array(raw, locals)? {
        return Ok(value);
    }
    if let Some((left, _, right)) = split_binary_words(raw, &["||"]) {
        let left = eval_expr(left, locals)?;
        if truthy(&left) {
            return Ok(ActionValue::Bool(true));
        }
        return Ok(ActionValue::Bool(truthy(&eval_expr(right, locals)?)));
    }
    if let Some((left, _, right)) = split_binary_words(raw, &["&&"]) {
        let left = eval_expr(left, locals)?;
        if !truthy(&left) {
            return Ok(ActionValue::Bool(false));
        }
        return Ok(ActionValue::Bool(truthy(&eval_expr(right, locals)?)));
    }
    if let Some(rest) = raw.strip_prefix('!') {
        return Ok(ActionValue::Bool(!truthy(&eval_expr(rest, locals)?)));
    }
    if let Some((left, op, right)) = split_comparison(raw) {
        return eval_comparison(left, op, right, locals);
    }
    if let Some((left, op, right)) = split_binary(raw, &['+', '-']) {
        return eval_binary(left, op, right, locals);
    }
    if let Some((left, op, right)) = split_binary(raw, &['*', '/']) {
        return eval_binary(left, op, right, locals);
    }
    Err(format!("unsupported Server Action expression `{raw}`"))
}

fn eval_binary(
    left: &str,
    op: char,
    right: &str,
    locals: &HashMap<String, ActionValue>,
) -> Result<ActionValue, String> {
    let left = eval_expr(left, locals)?;
    let right = eval_expr(right, locals)?;
    eval_action_binary(left, op, right)
}

fn eval_action_binary(
    left: ActionValue,
    op: char,
    right: ActionValue,
) -> Result<ActionValue, String> {
    match op {
        '+' => match (left, right) {
            (ActionValue::Number(left), ActionValue::Number(right)) => {
                Ok(ActionValue::Number(left + right))
            }
            (left, right) => Ok(ActionValue::String(format!(
                "{}{}",
                left.to_text(),
                right.to_text()
            ))),
        },
        '-' => Ok(ActionValue::Number(number(left)? - number(right)?)),
        '*' => Ok(ActionValue::Number(number(left)? * number(right)?)),
        '/' => {
            let right = number(right)?;
            if right == 0 {
                Err("division by zero in Server Action".into())
            } else {
                Ok(ActionValue::Number(number(left)? / right))
            }
        }
        _ => Err(format!("unsupported Server Action operator `{op}`")),
    }
}

fn eval_comparison(
    left: &str,
    op: &str,
    right: &str,
    locals: &HashMap<String, ActionValue>,
) -> Result<ActionValue, String> {
    let left = eval_expr(left, locals)?;
    let right = eval_expr(right, locals)?;
    let value = match op {
        "==" => left == right,
        "!=" => left != right,
        ">" => number(left)? > number(right)?,
        ">=" => number(left)? >= number(right)?,
        "<" => number(left)? < number(right)?,
        "<=" => number(left)? <= number(right)?,
        _ => return Err(format!("unsupported Server Action comparison `{op}`")),
    };
    Ok(ActionValue::Bool(value))
}

fn eval_array(
    raw: &str,
    locals: &HashMap<String, ActionValue>,
) -> Result<Option<ActionValue>, String> {
    if !(raw.starts_with('[') && raw.ends_with(']')) {
        return Ok(None);
    }
    let inner = &raw[1..raw.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Some(ActionValue::Array(Vec::new())));
    }
    split_top_level_commas(inner)
        .into_iter()
        .map(|item| eval_expr(item, locals))
        .collect::<Result<Vec<_>, _>>()
        .map(ActionValue::Array)
        .map(Some)
}

fn number(value: ActionValue) -> Result<i64, String> {
    match value {
        ActionValue::Number(value) => Ok(value),
        _ => Err("numeric Server Action expression received a non-number".into()),
    }
}

fn truthy(value: &ActionValue) -> bool {
    match value {
        ActionValue::Null => false,
        ActionValue::Bool(value) => *value,
        ActionValue::Number(value) => *value != 0,
        ActionValue::String(value) => !value.is_empty(),
        ActionValue::Array(value) => !value.is_empty(),
    }
}

fn validate_return(action: &ServerActionDecl, value: &ActionValue) -> Result<(), ActionError> {
    if value_matches_type(value, &action.return_ty) {
        return Ok(());
    }
    Err(ActionError {
        status: 500,
        message: format!(
            "Server Action `{}` returned {}, expected {}",
            action.name,
            value.kind(),
            action.return_ty
        ),
    })
}

fn value_matches_type(value: &ActionValue, ty: &str) -> bool {
    let ty = ty.trim().trim_matches(['"', '\'']);
    match ty {
        "Any" | "Unknown" => true,
        "Void" | "Null" => matches!(value, ActionValue::Null),
        "Bool" | "bool" => matches!(value, ActionValue::Bool(_)),
        "String" | "str" => matches!(value, ActionValue::String(_)),
        "Array" => matches!(value, ActionValue::Array(_)),
        "i64" | "i32" | "u64" | "u32" | "Int" | "Number" => {
            matches!(value, ActionValue::Number(_))
        }
        _ => true,
    }
}

fn parse_call_args(body: &str) -> Result<Vec<ActionValue>, ActionError> {
    let value = JsonParser::new(body)
        .parse()
        .map_err(|message| ActionError {
            status: 400,
            message,
        })?;
    let fields = match value {
        ParsedJson::Object(fields) => fields,
        ParsedJson::Value(_) => {
            return Err(ActionError {
                status: 400,
                message: "Server Action request body must be a JSON object".into(),
            });
        }
    };
    match fields.get("args") {
        Some(ActionValue::Array(args)) => Ok(args.clone()),
        _ => Err(ActionError {
            status: 400,
            message: "Server Action request body must contain an `args` array".into(),
        }),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ParsedJson {
    Value(ActionValue),
    Object(HashMap<String, ActionValue>),
}

struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<ParsedJson, String> {
        self.skip_ws();
        let value = self.parse_json()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err("unexpected trailing JSON data".into());
        }
        Ok(value)
    }

    fn parse_json(&mut self) -> Result<ParsedJson, String> {
        match self.peek() {
            Some(b'{') => self.parse_object(),
            _ => self.parse_value().map(ParsedJson::Value),
        }
    }

    fn parse_value(&mut self) -> Result<ActionValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => self.parse_string().map(ActionValue::String),
            Some(b'[') => self.parse_array(),
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(ActionValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(ActionValue::Bool(false))
            }
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(ActionValue::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err("expected JSON value".into()),
        }
    }

    fn parse_object(&mut self) -> Result<ParsedJson, String> {
        self.expect_byte(b'{')?;
        let mut fields = HashMap::new();
        self.skip_ws();
        if self.eat_byte(b'}') {
            return Ok(ParsedJson::Object(fields));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b':')?;
            let value = self.parse_value()?;
            fields.insert(key, value);
            self.skip_ws();
            if self.eat_byte(b'}') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(ParsedJson::Object(fields))
    }

    fn parse_array(&mut self) -> Result<ActionValue, String> {
        self.expect_byte(b'[')?;
        let mut values = Vec::new();
        self.skip_ws();
        if self.eat_byte(b']') {
            return Ok(ActionValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.eat_byte(b']') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(ActionValue::Array(values))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.bump() {
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(escaped) = self.bump() else {
                        return Err("unterminated JSON escape".into());
                    };
                    out.push(match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{0008}',
                        b'f' => '\u{000c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        _ => return Err("unsupported JSON escape".into()),
                    });
                }
                other => out.push(other as char),
            }
        }
        Err("unterminated JSON string".into())
    }

    fn parse_number(&mut self) -> Result<ActionValue, String> {
        let start = self.pos;
        self.eat_byte(b'-');
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| "invalid JSON number".to_string())?;
        let value = raw
            .parse::<i64>()
            .map_err(|_| format!("unsupported JSON number `{raw}`"))?;
        Ok(ActionValue::Number(value))
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), String> {
        for byte in expected {
            self.expect_byte(*byte)?;
        }
        Ok(())
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), String> {
        if self.eat_byte(expected) {
            Ok(())
        } else {
            Err(format!("expected JSON byte `{}`", expected as char))
        }
    }

    fn eat_byte(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }
}

fn split_binary<'a>(raw: &'a str, ops: &[char]) -> Option<(&'a str, char, &'a str)> {
    let mut depth = 0usize;
    let mut quote = None;
    for (index, ch) in raw.char_indices().rev() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            ')' | ']' => depth += 1,
            '(' | '[' => depth = depth.saturating_sub(1),
            op if depth == 0 && ops.contains(&op) && index > 0 => {
                let left = raw[..index].trim();
                let right = raw[index + op.len_utf8()..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, op, right));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_comparison<'a>(raw: &'a str) -> Option<(&'a str, &'static str, &'a str)> {
    split_binary_words(raw, &["==", "!=", ">=", "<=", ">", "<"])
}

fn split_binary_words<'a>(
    raw: &'a str,
    ops: &[&'static str],
) -> Option<(&'a str, &'static str, &'a str)> {
    let mut depth = 0usize;
    let mut quote = None;
    for (index, ch) in raw.char_indices().rev() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            ')' | ']' => depth += 1,
            '(' | '[' => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                for op in ops {
                    if raw[index..].starts_with(op) {
                        let left = raw[..index].trim();
                        let right = raw[index + op.len()..].trim();
                        if !left.is_empty() && !right.is_empty() {
                            return Some((left, *op, right));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_commas(raw: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    for (index, ch) in raw.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(raw[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    items.push(raw[start..].trim());
    items
}

fn strip_parens(raw: &str) -> &str {
    let mut out = raw;
    loop {
        let trimmed = out.trim();
        if trimmed.starts_with('(') && trimmed.ends_with(')') && wraps_entire_expr(trimmed) {
            out = &trimmed[1..trimmed.len() - 1];
        } else {
            return trimmed;
        }
    }
}

fn wraps_entire_expr(raw: &str) -> bool {
    let mut depth = 0usize;
    let mut quote = None;
    for (index, ch) in raw.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && index < raw.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn is_string_literal(raw: &str) -> bool {
    (raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\''))
}

fn unquote(raw: &str) -> String {
    raw[1..raw.len().saturating_sub(1)].to_string()
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::{ActionRuntime, ActionValue, ServerRuntime};
    use lume_hir::lower;
    use lume_ir::build;
    use lume_parser::parse;

    #[test]
    fn executes_server_action_with_interpreter() {
        let runtime = runtime_for(
            r#"
server action add(a: i64, b: i64): i64 {
  return a + b
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let result = runtime
            .call("add", vec![ActionValue::Number(2), ActionValue::Number(5)])
            .expect("action result");
        assert_eq!(result.value, ActionValue::Number(7));
        assert_eq!(result.runtime, ActionRuntime::Interpreter);
    }

    #[test]
    fn handles_json_action_calls() {
        let runtime = runtime_for(
            r#"
server action echo(message: String): String {
  return message
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let body = runtime
            .call_json("echo", r#"{"args":["hello"]}"#)
            .expect("json response");
        assert_eq!(body, r#"{"value":"hello","runtime":"interpreter"}"#);
    }

    #[test]
    fn validates_server_action_argument_types() {
        let runtime = runtime_for(
            r#"
server action negate(active: Bool): Bool {
  return !active
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let err = runtime
            .call("negate", vec![ActionValue::String("nope".into())])
            .expect_err("type error");
        assert_eq!(err.status, 400);
        assert!(err.message.contains("argument `active` expected Bool"));
    }

    #[test]
    fn executes_assignments_comparisons_and_arrays() {
        let runtime = runtime_for(
            r#"
server action summary(count: i64, label: String): Array {
  count += 2;
  label = label + " saved";
  return [count, label, count >= 3]
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let result = runtime
            .call(
                "summary",
                vec![ActionValue::Number(1), ActionValue::String("Draft".into())],
            )
            .expect("action result");
        assert_eq!(
            result.value,
            ActionValue::Array(vec![
                ActionValue::Number(3),
                ActionValue::String("Draft saved".into()),
                ActionValue::Bool(true),
            ])
        );
    }

    #[test]
    fn rejects_json_bodies_over_action_max_body_size() {
        let runtime = runtime_for(
            r#"
server action echo(message: String): String maxBodySize 12 {
  return message
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let err = runtime
            .call_json("echo", r#"{"args":["this body is too long"]}"#)
            .expect_err("body limit");
        assert_eq!(err.status, 413);
    }

    fn runtime_for(source: &str) -> ServerRuntime {
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        ServerRuntime::new(ir.server_actions)
    }
}
