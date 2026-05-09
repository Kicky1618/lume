use lume_ast::{ServerActionDecl, Stmt};
use lume_backend_jit::JitBackend;
use std::collections::{BTreeMap, HashMap};

pub const RUNTIME_NAME: &str = "lume-runtime-server";

#[derive(Clone, Debug)]
pub struct ServerRuntime {
    actions: HashMap<String, ServerActionDecl>,
    jit: JitBackend,
}

#[derive(Clone, Debug)]
pub struct ActionRequestContext {
    pub csrf_token: Option<String>,
    pub expected_csrf_token: Option<String>,
    pub authenticated: bool,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub rate_limit_exceeded: bool,
}

impl Default for ActionRequestContext {
    fn default() -> Self {
        Self {
            csrf_token: Some("test-csrf-token".into()),
            expected_csrf_token: Some("test-csrf-token".into()),
            authenticated: false,
            roles: Vec::new(),
            permissions: Vec::new(),
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
    Object(BTreeMap<String, ActionValue>),
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
    pub revalidate: Vec<ActionValue>,
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
        if generic_inner(&action.return_ty, "Stream").is_some() {
            response.push_str(",\"stream\":true");
        }
        if let Some(transaction) = action_transaction(action) {
            response.push_str(&format!(
                ",\"transaction\":\"{}\"",
                escape_json(&transaction)
            ));
        }
        if !result.revalidate.is_empty() {
            response.push_str(&format!(
                ",\"revalidate\":[{}]",
                result
                    .revalidate
                    .iter()
                    .map(ActionValue::to_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        response.push('}');
        Ok(response)
    }

    pub fn call_sse_with_context(
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
        Ok(action_sse_response(&result))
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
        let locals = action_locals(action, &args);
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
                        revalidate: action_revalidations(action, &locals),
                    });
                }
            }
        }
        let value = execute_interpreted(action, &args)?;
        validate_return(action, &value)?;
        Ok(ActionResult {
            value,
            runtime: ActionRuntime::Interpreter,
            revalidate: action_revalidations(action, &locals),
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
            Self::Object(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(key, value)| format!("\"{}\":{}", escape_json(key), value.to_json()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    fn to_text(&self) -> String {
        match self {
            Self::Null | Self::Array(_) | Self::Object(_) => String::new(),
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
            Self::Object(_) => "Object",
        }
    }
}

fn action_sse_response(result: &ActionResult) -> String {
    let mut out = String::new();
    for chunk in action_stream_chunks(&result.value) {
        out.push_str("event: chunk\n");
        out.push_str("data: ");
        out.push_str(&chunk.to_json());
        out.push_str("\n\n");
    }
    out.push_str("event: done\n");
    out.push_str("data: {\"runtime\":\"");
    out.push_str(result.runtime.as_str());
    out.push('"');
    if !result.revalidate.is_empty() {
        out.push_str(",\"revalidate\":[");
        out.push_str(
            &result
                .revalidate
                .iter()
                .map(ActionValue::to_json)
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push(']');
    }
    out.push_str("}\n\n");
    out
}

fn action_stream_chunks(value: &ActionValue) -> Vec<ActionValue> {
    match value {
        ActionValue::Array(items) => items.clone(),
        ActionValue::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

fn action_runtime(action: &ServerActionDecl) -> Option<String> {
    modifier_value(action, "runtime")
}

fn enforce_action_guards(
    action: &ServerActionDecl,
    context: &ActionRequestContext,
) -> Result<(), ActionError> {
    if let Some(auth) = modifier_value(action, "auth") {
        let requirement = parse_auth_requirement(&auth);
        if requirement.kind != AuthKind::Optional && !context.authenticated {
            return Err(ActionError {
                status: 401,
                message: format!("Server Action `{}` requires auth", action.name),
            });
        }
        match requirement.kind {
            AuthKind::Role => {
                if !context.roles.iter().any(|role| role == &requirement.value) {
                    return Err(ActionError {
                        status: 403,
                        message: format!(
                            "Server Action `{}` requires role `{}`",
                            action.name, requirement.value
                        ),
                    });
                }
            }
            AuthKind::Can => {
                if !context
                    .permissions
                    .iter()
                    .any(|permission| permission == &requirement.value)
                {
                    return Err(ActionError {
                        status: 403,
                        message: format!(
                            "Server Action `{}` requires permission `{}`",
                            action.name, requirement.value
                        ),
                    });
                }
            }
            AuthKind::Required | AuthKind::Optional => {}
        }
    }
    if modifier_value(action, "csrf")
        .map(|value| value != "false")
        .unwrap_or(true)
        && (context.csrf_token.is_none() || context.csrf_token != context.expected_csrf_token)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthKind {
    Required,
    Optional,
    Role,
    Can,
}

#[derive(Clone, Debug)]
struct AuthRequirement {
    kind: AuthKind,
    value: String,
}

fn parse_auth_requirement(value: &str) -> AuthRequirement {
    let compact = value.split_whitespace().collect::<String>();
    if compact == "optional" {
        return AuthRequirement {
            kind: AuthKind::Optional,
            value: String::new(),
        };
    }
    if let Some(value) = compact.strip_prefix("role=") {
        return AuthRequirement {
            kind: AuthKind::Role,
            value: value.trim_matches(['"', '\'']).to_string(),
        };
    }
    if let Some(value) = compact.strip_prefix("can=") {
        return AuthRequirement {
            kind: AuthKind::Can,
            value: value.trim_matches(['"', '\'']).to_string(),
        };
    }
    AuthRequirement {
        kind: AuthKind::Required,
        value: String::new(),
    }
}

fn validate_action_rules(
    action: &ServerActionDecl,
    args: &[ActionValue],
) -> Result<(), ActionError> {
    let Some(rules) = action
        .modifiers
        .iter()
        .find(|modifier| modifier.name == "validate")
        .and_then(|modifier| modifier.value.as_deref())
    else {
        return Ok(());
    };
    let locals = action_locals(action, args);
    let validation_rules = parse_validation_rules(rules);
    if !validation_rules.is_empty() {
        for rule in validation_rules {
            let value = resolve_validation_field(&rule.field, &locals);
            for check in rule.checks {
                if !validation_check_passes(&check, value) {
                    return Err(ActionError {
                        status: 400,
                        message: format!(
                            "Server Action `{}` validation failed for `{}`",
                            action.name, rule.field
                        ),
                    });
                }
            }
        }
        return Ok(());
    }
    for (param, value) in action.params.iter().zip(args) {
        if matches!(value, ActionValue::Null)
            || matches!(value, ActionValue::String(text) if text.trim().is_empty())
        {
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

#[derive(Clone, Debug)]
struct ValidationRule {
    field: String,
    checks: Vec<String>,
}

fn parse_validation_rules(raw: &str) -> Vec<ValidationRule> {
    let tokens = raw.split_whitespace().collect::<Vec<_>>();
    let mut rules = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        let field = tokens[index].trim_end_matches(':');
        if !field.contains('.') && !field.ends_with(':') {
            index += 1;
            continue;
        }
        index += 1;
        let mut checks = Vec::new();
        while index < tokens.len() {
            let token = tokens[index].trim_end_matches(':');
            if token.contains('.') || tokens[index].ends_with(':') {
                break;
            }
            checks.push(tokens[index].to_string());
            index += 1;
        }
        if !checks.is_empty() {
            rules.push(ValidationRule {
                field: field.to_string(),
                checks,
            });
        }
    }
    rules
}

fn resolve_validation_field<'a>(
    field: &str,
    locals: &'a HashMap<String, ActionValue>,
) -> Option<&'a ActionValue> {
    let mut parts = field.split('.');
    let first = parts.next()?;
    let mut current = locals.get(first)?;
    for part in parts {
        let ActionValue::Object(object) = current else {
            return None;
        };
        current = object.get(part)?;
    }
    Some(current)
}

fn validation_check_passes(check: &str, value: Option<&ActionValue>) -> bool {
    match check {
        "required" => value.is_some_and(|value| {
            !matches!(value, ActionValue::Null)
                && !matches!(value, ActionValue::String(text) if text.trim().is_empty())
        }),
        "email" => value.is_some_and(|value| {
            matches!(value, ActionValue::String(text) if text.contains('@') && text.contains('.'))
        }),
        other if other.starts_with("minLength(") => {
            let Some(min) = numeric_rule_arg(other) else {
                return false;
            };
            value.is_some_and(|value| matches!(value, ActionValue::String(text) if text.len() >= min))
        }
        other if other.starts_with("maxLength(") => {
            let Some(max) = numeric_rule_arg(other) else {
                return false;
            };
            value.is_some_and(|value| matches!(value, ActionValue::String(text) if text.len() <= max))
        }
        _ => true,
    }
}

fn numeric_rule_arg(rule: &str) -> Option<usize> {
    rule.split_once('(')?
        .1
        .trim_end_matches(')')
        .parse::<usize>()
        .ok()
}

fn action_revalidations(
    action: &ServerActionDecl,
    locals: &HashMap<String, ActionValue>,
) -> Vec<ActionValue> {
    let mut values = Vec::new();
    for name in ["revalidate", "invalidates"] {
        if let Some(value) = modifier_value(action, name) {
            values.extend(action_revalidation_values(&value, locals));
        }
    }
    for stmt in &action.body.statements {
        if let Stmt::Expr(expr) = stmt {
            let raw = expr.raw.trim();
            if let Some(inner) = raw
                .strip_prefix("revalidate(")
                .and_then(|value| value.strip_suffix(')'))
            {
                values.extend(action_revalidation_values(inner, locals));
            }
        }
    }
    values
}

fn action_revalidation_values(
    raw: &str,
    locals: &HashMap<String, ActionValue>,
) -> Vec<ActionValue> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    let value = eval_expr(raw, locals)
        .unwrap_or_else(|_| ActionValue::String(raw.trim_matches(['"', '\'']).to_string()));
    match value {
        ActionValue::Array(items)
            if !items.is_empty()
                && items
                    .iter()
                    .all(|item| matches!(item, ActionValue::Array(_))) =>
        {
            items
        }
        other => vec![other],
    }
}

fn action_max_body_size(action: &ServerActionDecl) -> Result<Option<usize>, ActionError> {
    modifier_value(action, "maxBodySize")
        .map(|value| {
            parse_byte_size(&value).ok_or_else(|| ActionError {
                status: 500,
                message: format!(
                    "Server Action `{}` has invalid maxBodySize `{}`",
                    action.name, value
                ),
            })
        })
        .transpose()
}

fn action_transaction(action: &ServerActionDecl) -> Option<String> {
    modifier_presence_value(action, "transaction", "true")
}

fn modifier_value(action: &ServerActionDecl, name: &str) -> Option<String> {
    action_modifier(action, name)
        .and_then(|modifier| modifier.value.as_deref())
        .map(normalize_modifier_value)
}

fn modifier_presence_value(action: &ServerActionDecl, name: &str, default: &str) -> Option<String> {
    action_modifier(action, name).map(|modifier| {
        modifier
            .value
            .as_deref()
            .map(normalize_modifier_value)
            .unwrap_or_else(|| default.into())
    })
}

fn action_modifier<'a>(
    action: &'a ServerActionDecl,
    name: &str,
) -> Option<&'a lume_ast::ServerModifier> {
    action
        .modifiers
        .iter()
        .find(|modifier| modifier.name == name)
}

fn normalize_modifier_value(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).to_string()
}

fn parse_byte_size(value: &str) -> Option<usize> {
    let compact = value
        .trim()
        .trim_matches(['"', '\''])
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '_')
        .collect::<String>();
    if compact.is_empty() {
        return None;
    }
    let split = compact
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(compact.len());
    let number = compact[..split].parse::<usize>().ok()?;
    let unit = compact[split..].to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1,
        "KB" | "KIB" => 1024,
        "MB" | "MIB" => 1024 * 1024,
        "GB" | "GIB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

fn all_i64_args(args: &[ActionValue]) -> Option<Vec<i64>> {
    args.iter().map(ActionValue::as_i64).collect()
}

fn action_locals(action: &ServerActionDecl, args: &[ActionValue]) -> HashMap<String, ActionValue> {
    action
        .params
        .iter()
        .zip(args.iter())
        .map(|(param, value)| (param.name.clone(), value.clone()))
        .collect()
}

fn execute_interpreted(
    action: &ServerActionDecl,
    args: &[ActionValue],
) -> Result<ActionValue, ActionError> {
    let mut locals = action_locals(action, args);
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
                if raw.starts_with("revalidate(") {
                    continue;
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
    if let Some(value) = eval_object(raw, locals)? {
        return Ok(value);
    }
    if let Some(value) = eval_path(raw, locals)? {
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

fn eval_object(
    raw: &str,
    locals: &HashMap<String, ActionValue>,
) -> Result<Option<ActionValue>, String> {
    if !(raw.starts_with('{') && raw.ends_with('}')) {
        return Ok(None);
    }
    let inner = &raw[1..raw.len() - 1];
    let mut fields = BTreeMap::new();
    if inner.trim().is_empty() {
        return Ok(Some(ActionValue::Object(fields)));
    }
    for field in split_top_level_commas(inner) {
        let Some((key, value)) = split_top_level_once(field, ':') else {
            return Err(format!("invalid Server Action object field `{field}`"));
        };
        let key = key.trim().trim_matches(['"', '\'']);
        if key.is_empty() {
            return Err("Server Action object field has an empty key".into());
        }
        fields.insert(key.to_string(), eval_expr(value.trim(), locals)?);
    }
    Ok(Some(ActionValue::Object(fields)))
}

fn eval_path(
    raw: &str,
    locals: &HashMap<String, ActionValue>,
) -> Result<Option<ActionValue>, String> {
    if !raw.contains('.') {
        return Ok(None);
    }
    let mut parts = raw.split('.');
    let Some(first) = parts.next() else {
        return Ok(None);
    };
    let Some(mut value) = locals.get(first.trim()).cloned() else {
        return Ok(None);
    };
    for part in parts {
        let key = part.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Ok(None);
        }
        match value {
            ActionValue::Object(fields) => {
                value = fields.get(key).cloned().ok_or_else(|| {
                    format!("Server Action object has no field `{key}` in `{raw}`")
                })?;
            }
            _ => {
                return Err(format!(
                    "Server Action value `{key}` is not an object field"
                ))
            }
        }
    }
    Ok(Some(value))
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
        ActionValue::Object(value) => !value.is_empty(),
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
    if ty.ends_with('?') {
        return matches!(value, ActionValue::Null)
            || value_matches_type(value, ty.trim_end_matches('?'));
    }
    if let Some(inner) = ty.strip_suffix("[]") {
        return matches!(
            value,
            ActionValue::Array(items) if items.iter().all(|item| value_matches_type(item, inner))
        );
    }
    if let Some(inner) = generic_inner(ty, "Array") {
        return matches!(
            value,
            ActionValue::Array(items) if items.iter().all(|item| value_matches_type(item, inner))
        );
    }
    if let Some(inner) = generic_inner(ty, "Optional") {
        return matches!(value, ActionValue::Null) || value_matches_type(value, inner);
    }
    if let Some(inner) = generic_inner(ty, "Stream") {
        return match value {
            ActionValue::Array(items) => items.iter().all(|item| value_matches_type(item, inner)),
            other => value_matches_type(other, inner),
        };
    }
    if let Some(inner) = generic_inner(ty, "Result") {
        let args = split_generic_args(inner);
        return args.len() == 2 && value_matches_result(value, args[0], args[1]);
    }
    if let Some(inner) = generic_inner(ty, "Union") {
        let args = split_generic_args(inner);
        return args.iter().any(|arg| value_matches_type(value, arg));
    }
    match ty {
        "Any" | "Unknown" => true,
        "Void" | "Null" => matches!(value, ActionValue::Null),
        "Bool" | "bool" => matches!(value, ActionValue::Bool(_)),
        "String" | "str" => matches!(value, ActionValue::String(_)),
        "URL" => matches!(value, ActionValue::String(_)),
        "Array" => matches!(value, ActionValue::Array(_)),
        "Object" => matches!(value, ActionValue::Object(_)),
        "File" => is_file_value(value),
        "FormData" => matches!(value, ActionValue::Object(_) | ActionValue::Null),
        "i64" | "i32" | "u64" | "u32" | "Int" | "Number" | "Float" | "f64" | "f32" => {
            matches!(value, ActionValue::Number(_))
        }
        _ => true,
    }
}

fn value_matches_result(value: &ActionValue, ok_ty: &str, err_ty: &str) -> bool {
    let ActionValue::Object(fields) = value else {
        return false;
    };
    let Some(ActionValue::Bool(ok)) = fields.get("ok") else {
        return false;
    };
    if *ok {
        fields
            .get("value")
            .map(|value| value_matches_type(value, ok_ty))
            .unwrap_or_else(|| matches!(ok_ty.trim(), "Void" | "Null"))
    } else {
        fields
            .get("error")
            .map(|value| value_matches_type(value, err_ty))
            .unwrap_or(true)
    }
}

fn is_file_value(value: &ActionValue) -> bool {
    let ActionValue::Object(fields) = value else {
        return false;
    };
    matches!(fields.get("__lumeFile"), Some(ActionValue::Bool(true)))
        && matches!(fields.get("name"), Some(ActionValue::String(_)))
        && matches!(fields.get("size"), Some(ActionValue::Number(_)))
}

fn generic_inner<'a>(ty: &'a str, name: &str) -> Option<&'a str> {
    ty.strip_prefix(name)?
        .strip_prefix('<')?
        .strip_suffix('>')
        .map(str::trim)
}

fn split_generic_args(raw: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(raw[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    args.push(raw[start..].trim());
    args.into_iter().filter(|arg| !arg.is_empty()).collect()
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
    Object(BTreeMap<String, ActionValue>),
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
            Some(b'{') => self.parse_object_fields().map(ActionValue::Object),
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
        self.parse_object_fields().map(ParsedJson::Object)
    }

    fn parse_object_fields(&mut self) -> Result<BTreeMap<String, ActionValue>, String> {
        self.expect_byte(b'{')?;
        let mut fields = BTreeMap::new();
        self.skip_ws();
        if self.eat_byte(b'}') {
            return Ok(fields);
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
        Ok(fields)
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
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => depth = depth.saturating_sub(1),
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
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => depth = depth.saturating_sub(1),
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
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

fn split_top_level_once(raw: &str, delimiter: char) -> Option<(&str, &str)> {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => {
                return Some((&raw[..index], &raw[index + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
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
    use super::{ActionRequestContext, ActionRuntime, ActionValue, ServerRuntime};
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

    #[test]
    fn returns_structured_action_revalidations() {
        let runtime = runtime_for(
            r#"
server action save(id: String): String invalidates ["note", id] {
  revalidate("/notes")
  return id
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let result = runtime
            .call("save", vec![ActionValue::String("n1".into())])
            .expect("action result");
        assert_eq!(
            result.revalidate,
            vec![
                ActionValue::Array(vec![
                    ActionValue::String("note".into()),
                    ActionValue::String("n1".into()),
                ]),
                ActionValue::String("/notes".into()),
            ]
        );
        let body = runtime
            .call_json("save", r#"{"args":["n1"]}"#)
            .expect("json response");
        assert!(body.contains(r#""revalidate":[["note","n1"],"/notes"]"#));
    }

    #[test]
    fn accepts_object_action_arguments() {
        let runtime = runtime_for(
            r#"
server action echo(input: Object): Object {
  return input
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let body = runtime
            .call_json("echo", r#"{"args":[{"title":"Hello","count":2}]}"#)
            .expect("json response");
        assert_eq!(
            body,
            r#"{"value":{"count":2,"title":"Hello"},"runtime":"interpreter"}"#
        );
    }

    #[test]
    fn returns_result_objects_and_reads_file_metadata() {
        let runtime = runtime_for(
            r#"
server action upload(file: File): Result<URL, ActionError> maxBodySize 1MB {
  return { ok: true, value: file.name }
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let body = runtime
            .call_json(
                "upload",
                r#"{"args":[{"__lumeFile":true,"name":"avatar.png","type":"image/png","size":42}]}"#,
            )
            .expect("json response");
        assert_eq!(
            body,
            r#"{"value":{"ok":true,"value":"avatar.png"},"runtime":"interpreter"}"#
        );
    }

    #[test]
    fn enforces_required_auth_and_allows_optional_auth() {
        let runtime = runtime_for(
            r#"
server action privateEcho(message: String): String auth required {
  return message
}

server action publicEcho(message: String): String auth optional {
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
            .call("privateEcho", vec![ActionValue::String("secret".into())])
            .expect_err("auth error");
        assert_eq!(err.status, 401);
        let result = runtime
            .call("publicEcho", vec![ActionValue::String("hello".into())])
            .expect("optional auth result");
        assert_eq!(result.value, ActionValue::String("hello".into()));
    }

    #[test]
    fn enforces_csrf_token_presence_and_match() {
        let runtime = runtime_for(
            r#"
server action save(message: String): String csrf true {
  return message
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let missing = ActionRequestContext {
            csrf_token: None,
            expected_csrf_token: Some("expected".into()),
            ..Default::default()
        };
        assert_eq!(
            runtime
                .call_with_context("save", vec![ActionValue::String("draft".into())], &missing)
                .expect_err("missing csrf")
                .status,
            403
        );
        let mismatch = ActionRequestContext {
            csrf_token: Some("wrong".into()),
            expected_csrf_token: Some("expected".into()),
            ..Default::default()
        };
        assert_eq!(
            runtime
                .call_with_context("save", vec![ActionValue::String("draft".into())], &mismatch)
                .expect_err("csrf mismatch")
                .status,
            403
        );
        let matched = ActionRequestContext {
            csrf_token: Some("expected".into()),
            expected_csrf_token: Some("expected".into()),
            ..Default::default()
        };
        assert_eq!(
            runtime
                .call_with_context("save", vec![ActionValue::String("draft".into())], &matched)
                .expect("csrf ok")
                .value,
            ActionValue::String("draft".into())
        );
    }

    #[test]
    fn enforces_auth_role_and_permission_requirements() {
        let runtime = runtime_for(
            r#"
server action deleteUser(id: String): String auth role="admin" {
  return id
}

server action updatePost(id: String): String auth can="post:update" {
  return id
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let authenticated = ActionRequestContext {
            authenticated: true,
            ..Default::default()
        };
        assert_eq!(
            runtime
                .call_with_context(
                    "deleteUser",
                    vec![ActionValue::String("u1".into())],
                    &authenticated
                )
                .expect_err("role error")
                .status,
            403
        );
        let admin = ActionRequestContext {
            authenticated: true,
            roles: vec!["admin".into()],
            permissions: vec!["post:update".into()],
            ..Default::default()
        };
        assert_eq!(
            runtime
                .call_with_context("deleteUser", vec![ActionValue::String("u1".into())], &admin)
                .expect("role result")
                .value,
            ActionValue::String("u1".into())
        );
        assert_eq!(
            runtime
                .call_with_context("updatePost", vec![ActionValue::String("p1".into())], &admin)
                .expect("permission result")
                .value,
            ActionValue::String("p1".into())
        );
    }

    #[test]
    fn enforces_validation_dsl_rules() {
        let runtime = runtime_for(
            r#"
server action createUser(input: Object): Object
  validate {
    input.email: required email
    input.password: required minLength(8)
  }
{
  return input
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let err = runtime
            .call_json(
                "createUser",
                r#"{"args":[{"email":"bad","password":"short"}]}"#,
            )
            .expect_err("validation error");
        assert_eq!(err.status, 400);
        let body = runtime
            .call_json(
                "createUser",
                r#"{"args":[{"email":"a@example.com","password":"long-enough"}]}"#,
            )
            .expect("valid input");
        assert!(body.contains(r#""email":"a@example.com""#));
    }

    #[test]
    fn exposes_transaction_metadata_in_json_response() {
        let runtime = runtime_for(
            r#"
server action save(message: String): String transaction {
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
            .call_json("save", r#"{"args":["ok"]}"#)
            .expect("json response");
        assert!(body.contains(r#""transaction":"true""#));
    }

    #[test]
    fn streams_action_arrays_as_sse_chunks() {
        let runtime = runtime_for(
            r#"
server action chunks(prompt: String): Stream<String> {
  return ["Hello", prompt]
}

component App {
  view {
    Text("ok")
  }
}
"#,
        );
        let sse = runtime
            .call_sse_with_context("chunks", r#"{"args":["world"]}"#, &Default::default())
            .expect("sse response");
        assert!(sse.contains("event: chunk\ndata: \"Hello\""));
        assert!(sse.contains("event: chunk\ndata: \"world\""));
        assert!(sse.contains("event: done"));
        let json = runtime
            .call_json("chunks", r#"{"args":["world"]}"#)
            .expect("json response");
        assert!(json.contains(r#""stream":true"#));
    }

    fn runtime_for(source: &str) -> ServerRuntime {
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        ServerRuntime::new(ir.server_actions)
    }
}
