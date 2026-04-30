use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target};
use inkwell::types::IntType;
use inkwell::values::{FunctionValue, IntValue};
use inkwell::OptimizationLevel;
use lume_ast::{ServerActionDecl, Stmt};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct LlvmBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlvmActionModule {
    pub symbol: String,
    pub ir: String,
    pub param_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlvmJitOutput {
    pub value: i64,
    pub ir: String,
}

impl LlvmBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn is_available(&self) -> bool {
        Target::initialize_native(&InitializationConfig::default()).is_ok()
    }

    pub fn compile_server_action_i64(
        &self,
        action: &ServerActionDecl,
    ) -> Result<LlvmActionModule, String> {
        compile_server_action_i64(action)
    }

    pub fn execute_server_action_i64(
        &self,
        action: &ServerActionDecl,
        args: &[i64],
    ) -> Result<LlvmJitOutput, String> {
        execute_server_action_i64(action, args)
    }
}

pub fn compile_server_action_i64(action: &ServerActionDecl) -> Result<LlvmActionModule, String> {
    let context = Context::create();
    let (module, function, symbol) = build_action_module(&context, action)?;
    Ok(LlvmActionModule {
        symbol,
        ir: module.print_to_string().to_string(),
        param_count: function.count_params() as usize,
    })
}

pub fn execute_server_action_i64(
    action: &ServerActionDecl,
    args: &[i64],
) -> Result<LlvmJitOutput, String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|err| format!("failed to initialize native LLVM target: {err}"))?;
    let context = Context::create();
    let (module, function, symbol) = build_action_module(&context, action)?;
    if function.count_params() as usize != args.len() {
        return Err(format!(
            "server action `{}` expected {} args, got {}",
            action.name,
            function.count_params(),
            args.len()
        ));
    }
    let ir = module.print_to_string().to_string();
    let engine = module
        .create_jit_execution_engine(OptimizationLevel::Aggressive)
        .map_err(|err| format!("failed to create LLVM JIT execution engine: {err}"))?;
    let value = unsafe { call_jit_function(&engine, &symbol, args)? };
    Ok(LlvmJitOutput { value, ir })
}

fn build_action_module<'ctx>(
    context: &'ctx Context,
    action: &ServerActionDecl,
) -> Result<(Module<'ctx>, FunctionValue<'ctx>, String), String> {
    validate_i64_action(action)?;
    let symbol = format!("lume_action_{}", sanitize_symbol(&action.name));
    let module = context.create_module(&symbol);
    let function = build_i64_function(context, &module, action, &symbol)?;
    Ok((module, function, symbol))
}

fn build_i64_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    action: &ServerActionDecl,
    symbol: &str,
) -> Result<FunctionValue<'ctx>, String> {
    let i64_type = context.i64_type();
    let param_types = action
        .params
        .iter()
        .map(|_| i64_type.into())
        .collect::<Vec<_>>();
    let fn_type = i64_type.fn_type(&param_types, false);
    let function = module.add_function(symbol, fn_type, None);
    let block = context.append_basic_block(function, "entry");
    let builder = context.create_builder();
    builder.position_at_end(block);
    let locals = action
        .params
        .iter()
        .zip(function.get_param_iter())
        .map(|(param, value)| {
            let value = value.into_int_value();
            value.set_name(&param.name);
            (param.name.clone(), value)
        })
        .collect::<HashMap<_, _>>();
    let mut ctx = CodegenCtx {
        builder: &builder,
        i64_type,
        locals,
        next_temp: 0,
    };
    let expr = return_expr(action)?;
    let value = ctx.lower_expr(expr)?;
    builder
        .build_return(Some(&value))
        .map_err(|err| format!("failed to build return: {err}"))?;
    if function.verify(true) {
        Ok(function)
    } else {
        Err(format!(
            "LLVM verification failed for server action `{}`",
            action.name
        ))
    }
}

struct CodegenCtx<'builder, 'ctx> {
    builder: &'builder Builder<'ctx>,
    i64_type: IntType<'ctx>,
    locals: HashMap<String, IntValue<'ctx>>,
    next_temp: usize,
}

impl<'ctx> CodegenCtx<'_, 'ctx> {
    fn lower_expr(&mut self, raw: &str) -> Result<IntValue<'ctx>, String> {
        let raw = strip_parens(raw.trim());
        if raw.is_empty() {
            return Err("empty return expression".into());
        }
        if let Ok(number) = raw.parse::<i64>() {
            return Ok(self.i64_type.const_int(number as u64, true));
        }
        if let Some(value) = self.locals.get(raw) {
            return Ok(*value);
        }
        if let Some((left, op, right)) = split_binary(raw, &['+', '-']) {
            return self.lower_binary(left, op, right);
        }
        if let Some((left, op, right)) = split_binary(raw, &['*', '/']) {
            return self.lower_binary(left, op, right);
        }
        Err(format!("unsupported LLVM JIT expression `{raw}`"))
    }

    fn lower_binary(
        &mut self,
        left: &str,
        op: char,
        right: &str,
    ) -> Result<IntValue<'ctx>, String> {
        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;
        let name = self.next_temp();
        match op {
            '+' => self.builder.build_int_add(left, right, &name),
            '-' => self.builder.build_int_sub(left, right, &name),
            '*' => self.builder.build_int_mul(left, right, &name),
            '/' => self.builder.build_int_signed_div(left, right, &name),
            _ => return Err(format!("unsupported LLVM JIT operator `{op}`")),
        }
        .map_err(|err| format!("failed to build integer operation `{op}`: {err}"))
    }

    fn next_temp(&mut self) -> String {
        let temp = format!("t{}", self.next_temp);
        self.next_temp += 1;
        temp
    }
}

unsafe fn call_jit_function(
    engine: &inkwell::execution_engine::ExecutionEngine<'_>,
    symbol: &str,
    args: &[i64],
) -> Result<i64, String> {
    match args {
        [] => {
            type JitFn = unsafe extern "C" fn() -> i64;
            let function = engine
                .get_function::<JitFn>(symbol)
                .map_err(|err| format!("failed to find JIT symbol `{symbol}`: {err:?}"))?;
            Ok(function.call())
        }
        [a] => {
            type JitFn = unsafe extern "C" fn(i64) -> i64;
            let function = engine
                .get_function::<JitFn>(symbol)
                .map_err(|err| format!("failed to find JIT symbol `{symbol}`: {err:?}"))?;
            Ok(function.call(*a))
        }
        [a, b] => {
            type JitFn = unsafe extern "C" fn(i64, i64) -> i64;
            let function = engine
                .get_function::<JitFn>(symbol)
                .map_err(|err| format!("failed to find JIT symbol `{symbol}`: {err:?}"))?;
            Ok(function.call(*a, *b))
        }
        [a, b, c] => {
            type JitFn = unsafe extern "C" fn(i64, i64, i64) -> i64;
            let function = engine
                .get_function::<JitFn>(symbol)
                .map_err(|err| format!("failed to find JIT symbol `{symbol}`: {err:?}"))?;
            Ok(function.call(*a, *b, *c))
        }
        [a, b, c, d] => {
            type JitFn = unsafe extern "C" fn(i64, i64, i64, i64) -> i64;
            let function = engine
                .get_function::<JitFn>(symbol)
                .map_err(|err| format!("failed to find JIT symbol `{symbol}`: {err:?}"))?;
            Ok(function.call(*a, *b, *c, *d))
        }
        _ => Err("LLVM JIT currently supports up to four i64 parameters".into()),
    }
}

fn validate_i64_action(action: &ServerActionDecl) -> Result<(), String> {
    if !is_integer_type(&action.return_ty) {
        return Err(format!(
            "server action `{}` does not return an integer type",
            action.name
        ));
    }
    if action
        .params
        .iter()
        .any(|param| !is_integer_type(&param.ty))
    {
        return Err(format!(
            "server action `{}` has non-integer JIT parameters",
            action.name
        ));
    }
    Ok(())
}

fn return_expr(action: &ServerActionDecl) -> Result<&str, String> {
    action
        .body
        .statements
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Expr(expr) => expr.raw.trim().strip_prefix("return").map(str::trim),
            _ => None,
        })
        .ok_or_else(|| format!("server action `{}` has no return expression", action.name))
}

fn is_integer_type(ty: &str) -> bool {
    matches!(
        ty.trim().trim_matches(['"', '\'']),
        "i64" | "i32" | "u64" | "u32" | "Int"
    )
}

fn sanitize_symbol(name: &str) -> String {
    let mut symbol = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            symbol.push(ch);
        } else {
            symbol.push('_');
        }
    }
    if symbol.chars().next().is_none_or(|ch| ch.is_ascii_digit()) {
        symbol.insert(0, '_');
    }
    symbol
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

#[cfg(test)]
mod tests {
    use super::{compile_server_action_i64, execute_server_action_i64, LlvmBackend};
    use lume_ast::Decl;
    use lume_parser::parse;

    #[test]
    fn backend_reports_native_target_availability_without_panicking() {
        let _ = LlvmBackend::new().is_available();
    }

    #[test]
    fn generates_i64_add_function_with_inkwell() {
        let action = add_action();
        let module = compile_server_action_i64(action).expect("llvm module");
        assert_eq!(module.symbol, "lume_action_add");
        assert!(module.ir.contains("define i64 @lume_action_add"));
        assert!(module.ir.contains("add i64 %a, %b"));
        assert!(module.ir.contains("ret i64"));
    }

    #[test]
    fn executes_i64_add_function_with_inkwell_jit() {
        if !LlvmBackend::new().is_available() {
            return;
        }
        let output = execute_server_action_i64(add_action(), &[2, 5]).expect("jit output");
        assert_eq!(output.value, 7);
        assert!(output.ir.contains("define i64 @lume_action_add"));
    }

    fn add_action() -> &'static lume_ast::ServerActionDecl {
        let source = r#"
server action add(a: i64, b: i64): i64 {
  return a + b
}

component App {
  view {
    Text("ok")
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let Decl::ServerAction(action) = &program.declarations[0] else {
            panic!("expected server action");
        };
        Box::leak(Box::new(action.clone()))
    }
}
