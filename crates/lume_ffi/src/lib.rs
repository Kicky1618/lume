use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiRegistry {
    pub modules: Vec<FfiModule>,
    pub structs: Vec<FfiStruct>,
    pub enums: Vec<FfiEnum>,
    pub opaques: Vec<FfiOpaque>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiModule {
    pub name: String,
    pub language: String,
    pub library: Option<String>,
    pub header: Option<String>,
    pub sources: Vec<String>,
    pub runtime: Vec<FfiRuntime>,
    pub safety: SafetyLevel,
    pub thread_safe: Option<bool>,
    pub lock: Option<String>,
    pub structs: Vec<FfiStruct>,
    pub enums: Vec<FfiEnum>,
    pub opaques: Vec<FfiOpaque>,
    pub functions: Vec<FfiFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiStruct {
    pub name: String,
    pub fields: Vec<FfiField>,
    pub repr: Repr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiField {
    pub name: String,
    pub ty: FfiType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiEnum {
    pub name: String,
    pub variants: Vec<String>,
    pub repr: Repr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiOpaque {
    pub name: String,
    pub ownership: Ownership,
    pub lifetime: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiFunction {
    pub name: String,
    pub params: Vec<FfiField>,
    pub return_ty: FfiType,
    pub ownership: Ownership,
    pub callback: bool,
    pub free: Option<String>,
    pub throws: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FfiType {
    Void,
    Primitive(String),
    Cstring,
    Utf8String,
    Utf16String,
    Bytes,
    Ptr(Box<FfiType>),
    ConstPtr(Box<FfiType>),
    Borrowed(Box<FfiType>),
    Owned(Box<FfiType>),
    View(Box<FfiType>),
    Handle(String),
    Struct(String),
    Opaque(String),
    Callback(String),
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ownership {
    Borrowed,
    Owned,
    Caller,
    Callee,
    View,
    Opaque(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FfiRuntime {
    Native,
    Jit,
    Wasm,
    Server,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SafetyLevel {
    Safe,
    Unsafe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repr {
    C,
    Packed,
    Align(u32),
    Int(String),
    Custom(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub diagnostics: Vec<FfiDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiDiagnostic {
    pub severity: FfiSeverity,
    pub code: &'static str,
    pub message: String,
    pub symbol: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FfiSeverity {
    Error,
    Warning,
}

impl FfiRegistry {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            opaques: Vec::new(),
        }
    }

    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        validate_unique_named(
            "module",
            self.modules.iter().map(|item| item.name.as_str()),
            &mut report,
        );
        validate_unique_named(
            "struct",
            self.structs.iter().map(|item| item.name.as_str()),
            &mut report,
        );
        validate_unique_named(
            "enum",
            self.enums.iter().map(|item| item.name.as_str()),
            &mut report,
        );
        validate_unique_named(
            "opaque",
            self.opaques.iter().map(|item| item.name.as_str()),
            &mut report,
        );

        let symbols = self.symbol_table();
        for item in &self.structs {
            item.validate_with_symbols(&symbols, &mut report);
        }
        for item in &self.enums {
            item.validate(&mut report);
        }
        for item in &self.opaques {
            item.validate(&mut report);
        }
        for module in &self.modules {
            module.validate_with_symbols(&symbols, &mut report);
        }
        report
    }

    pub fn validate_result(&self) -> Result<(), ValidationReport> {
        let report = self.validate();
        if report.has_errors() {
            Err(report)
        } else {
            Ok(())
        }
    }

    fn symbol_table(&self) -> SymbolTable {
        SymbolTable {
            structs: self.structs.iter().map(|item| item.name.clone()).collect(),
            enums: self.enums.iter().map(|item| item.name.clone()).collect(),
            opaques: self.opaques.iter().map(|item| item.name.clone()).collect(),
            callbacks: HashSet::new(),
        }
    }
}

impl FfiModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            language: "c".into(),
            library: None,
            header: None,
            sources: Vec::new(),
            runtime: vec![FfiRuntime::Native],
            safety: SafetyLevel::Safe,
            thread_safe: None,
            lock: None,
            structs: Vec::new(),
            enums: Vec::new(),
            opaques: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut report = ValidationReport::default();
        self.validate_with_symbols(&SymbolTable::default(), &mut report);
        if let Some(error) = report.first_error() {
            Err(error.message.clone())
        } else {
            Ok(())
        }
    }

    pub fn validate_report(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        self.validate_with_symbols(&SymbolTable::default(), &mut report);
        report
    }

    fn validate_with_symbols(&self, symbols: &SymbolTable, report: &mut ValidationReport) {
        validate_identifier("module", &self.name, report);
        if !matches!(self.language.as_str(), "c" | "cpp" | "c++") {
            report.error(
                "LUME2701",
                format!("unsupported ffi language `{}`", self.language),
                Some(self.name.clone()),
            );
        }
        if self.library.is_none() && self.sources.is_empty() {
            report.warning(
                "LUME2702",
                format!(
                    "ffi module `{}` has neither `library` nor `sources`; symbol resolution is deferred",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
        if matches!(self.language.as_str(), "cpp" | "c++") && self.header.is_none() {
            report.warning(
                "LUME2703",
                format!("ffi module `{}` uses C++ without a header", self.name),
                Some(self.name.clone()),
            );
        }
        if self.thread_safe == Some(false) && self.lock.is_none() {
            report.warning(
                "LUME2704",
                format!(
                    "ffi module `{}` is not threadSafe and has no `lock` policy",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
        validate_unique_named(
            "function",
            self.functions.iter().map(|item| item.name.as_str()),
            report,
        );
        let mut local_symbols = symbols.clone();
        local_symbols
            .structs
            .extend(self.structs.iter().map(|item| item.name.clone()));
        local_symbols
            .enums
            .extend(self.enums.iter().map(|item| item.name.clone()));
        local_symbols
            .opaques
            .extend(self.opaques.iter().map(|item| item.name.clone()));
        local_symbols.callbacks.extend(
            self.functions
                .iter()
                .filter_map(|item| item.callback.then(|| item.name.clone())),
        );
        for item in &self.structs {
            item.validate_with_symbols(&local_symbols, report);
        }
        for item in &self.enums {
            item.validate(report);
        }
        for item in &self.opaques {
            item.validate(report);
        }
        for function in &self.functions {
            function.validate(&local_symbols, report);
        }
    }
}

impl FfiStruct {
    fn validate_with_symbols(&self, symbols: &SymbolTable, report: &mut ValidationReport) {
        validate_identifier("struct", &self.name, report);
        validate_unique_named(
            "field",
            self.fields.iter().map(|item| item.name.as_str()),
            report,
        );
        if self.fields.is_empty() {
            report.warning(
                "LUME2710",
                format!("ffi struct `{}` has no fields", self.name),
                Some(self.name.clone()),
            );
        }
        for field in &self.fields {
            validate_identifier("field", &field.name, report);
            validate_type(&field.ty, symbols, TypePosition::Param, report);
        }
    }
}

impl FfiEnum {
    fn validate(&self, report: &mut ValidationReport) {
        validate_identifier("enum", &self.name, report);
        validate_unique_named("variant", self.variants.iter().map(String::as_str), report);
        if self.variants.is_empty() {
            report.error(
                "LUME2711",
                format!("ffi enum `{}` has no variants", self.name),
                Some(self.name.clone()),
            );
        }
    }
}

impl FfiOpaque {
    fn validate(&self, report: &mut ValidationReport) {
        validate_identifier("opaque", &self.name, report);
        if matches!(self.ownership, Ownership::View) && self.lifetime.is_none() {
            report.warning(
                "LUME2712",
                format!(
                    "ffi opaque `{}` has view ownership without a lifetime",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
    }
}

impl FfiFunction {
    fn validate(&self, symbols: &SymbolTable, report: &mut ValidationReport) {
        validate_identifier("function", &self.name, report);
        validate_unique_named(
            "parameter",
            self.params.iter().map(|item| item.name.as_str()),
            report,
        );
        for param in &self.params {
            validate_identifier("parameter", &param.name, report);
            validate_type(&param.ty, symbols, TypePosition::Param, report);
        }
        validate_type(&self.return_ty, symbols, TypePosition::Return, report);
        if self.callback && matches!(self.ownership, Ownership::Owned) {
            report.error(
                "LUME2720",
                format!(
                    "ffi callback `{}` cannot transfer owned callback memory",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
        if self.return_ty.requires_free() && self.free.is_none() {
            report.error(
                "LUME2721",
                format!(
                    "ffi function `{}` returns owned memory but has no free function",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
        if matches!(self.return_ty, FfiType::View(_)) {
            report.warning(
                "LUME2722",
                format!(
                    "ffi function `{}` returns a View; callers must not let it escape the synchronous action",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
        if self.throws.as_deref() == Some("cpp_exception") {
            report.error(
                "LUME2723",
                format!(
                    "ffi function `{}` cannot throw C++ exceptions across the boundary",
                    self.name
                ),
                Some(self.name.clone()),
            );
        }
    }
}

impl FfiType {
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim().trim_matches(['"', '\'']);
        let raw = strip_type_modifiers(raw);
        match raw {
            "" | "Void" | "void" => Self::Void,
            "cstring" => Self::Cstring,
            "Utf8String" => Self::Utf8String,
            "Utf16String" => Self::Utf16String,
            "Bytes" => Self::Bytes,
            primitive if is_primitive(primitive) => Self::Primitive(primitive.to_string()),
            _ => {
                if let Some(inner) = generic_arg(raw, "Ptr") {
                    Self::Ptr(Box::new(Self::parse(inner)))
                } else if let Some(inner) = generic_arg(raw, "ConstPtr") {
                    Self::ConstPtr(Box::new(Self::parse(inner)))
                } else if let Some(inner) = generic_arg(raw, "Borrowed") {
                    Self::Borrowed(Box::new(Self::parse(inner)))
                } else if let Some(inner) = generic_arg(raw, "Owned") {
                    Self::Owned(Box::new(Self::parse(inner)))
                } else if let Some(inner) = generic_arg(raw, "View") {
                    Self::View(Box::new(Self::parse(inner)))
                } else if let Some(inner) = generic_arg(raw, "Handle") {
                    Self::Handle(inner.trim().to_string())
                } else if let Some(inner) = generic_arg(raw, "Struct") {
                    Self::Struct(inner.trim().to_string())
                } else if let Some(inner) = generic_arg(raw, "Opaque") {
                    Self::Opaque(inner.trim().to_string())
                } else {
                    Self::Named(raw.to_string())
                }
            }
        }
    }

    pub fn is_pointer_like(&self) -> bool {
        matches!(self, Self::Ptr(_) | Self::ConstPtr(_))
    }

    pub fn is_server_serializable(&self) -> bool {
        matches!(
            self,
            Self::Void
                | Self::Primitive(_)
                | Self::Cstring
                | Self::Utf8String
                | Self::Utf16String
                | Self::Bytes
                | Self::Owned(_)
                | Self::Named(_)
        )
    }

    fn requires_free(&self) -> bool {
        matches!(self, Self::Owned(_) | Self::Handle(_))
    }
}

impl Ownership {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("borrowed").trim().trim_matches(['"', '\'']) {
            "borrowed" => Self::Borrowed,
            "owned" => Self::Owned,
            "caller" => Self::Caller,
            "callee" => Self::Callee,
            "view" => Self::View,
            other => Self::Opaque(other.to_string()),
        }
    }
}

impl FfiRuntime {
    pub fn parse(value: &str) -> Self {
        match value.trim().trim_matches(['"', '\'']) {
            "native" => Self::Native,
            "jit" => Self::Jit,
            "wasm" => Self::Wasm,
            "server" => Self::Server,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl SafetyLevel {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("safe").trim().trim_matches(['"', '\'']) {
            "unsafe" => Self::Unsafe,
            _ => Self::Safe,
        }
    }
}

impl Repr {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("C").trim().trim_matches(['"', '\'']) {
            "C" | "c" => Self::C,
            "packed" => Self::Packed,
            value if value.starts_with("align=") => value[6..]
                .parse()
                .map(Self::Align)
                .unwrap_or_else(|_| Self::Custom(value.to_string())),
            repr @ ("i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64") => {
                Self::Int(repr.to_string())
            }
            other => Self::Custom(other.to_string()),
        }
    }
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == FfiSeverity::Error)
    }

    pub fn first_error(&self) -> Option<&FfiDiagnostic> {
        self.diagnostics
            .iter()
            .find(|item| item.severity == FfiSeverity::Error)
    }

    fn error(&mut self, code: &'static str, message: String, symbol: Option<String>) {
        self.diagnostics.push(FfiDiagnostic {
            severity: FfiSeverity::Error,
            code,
            message,
            symbol,
        });
    }

    fn warning(&mut self, code: &'static str, message: String, symbol: Option<String>) {
        self.diagnostics.push(FfiDiagnostic {
            severity: FfiSeverity::Warning,
            code,
            message,
            symbol,
        });
    }
}

#[derive(Clone, Debug, Default)]
struct SymbolTable {
    structs: HashSet<String>,
    enums: HashSet<String>,
    opaques: HashSet<String>,
    callbacks: HashSet<String>,
}

#[derive(Clone, Copy)]
enum TypePosition {
    Param,
    Return,
}

fn validate_type(
    ty: &FfiType,
    symbols: &SymbolTable,
    position: TypePosition,
    report: &mut ValidationReport,
) {
    match ty {
        FfiType::Primitive(name) if matches!(name.as_str(), "Int" | "Float" | "Number") => {
            report.warning(
                "LUME2730",
                format!("use explicit FFI width instead of `{name}` at the boundary"),
                Some(name.clone()),
            );
        }
        FfiType::Ptr(inner) | FfiType::ConstPtr(inner) => {
            validate_type(inner, symbols, position, report);
            if matches!(position, TypePosition::Return) {
                report.warning(
                    "LUME2731",
                    "raw pointer return crosses the FFI boundary; prefer Owned<T> or Handle<T>"
                        .into(),
                    None,
                );
            }
        }
        FfiType::Borrowed(inner) | FfiType::Owned(inner) | FfiType::View(inner) => {
            validate_type(inner, symbols, position, report);
        }
        FfiType::Handle(name) | FfiType::Opaque(name) => {
            if !symbols.opaques.contains(name) {
                report.warning(
                    "LUME2732",
                    format!("ffi opaque type `{name}` has no declaration"),
                    Some(name.clone()),
                );
            }
        }
        FfiType::Struct(name) => {
            if !symbols.structs.contains(name) {
                report.warning(
                    "LUME2733",
                    format!("ffi struct `{name}` has no declaration"),
                    Some(name.clone()),
                );
            }
        }
        FfiType::Named(name) => {
            if symbols.structs.contains(name) || symbols.enums.contains(name) {
                return;
            }
            if symbols.opaques.contains(name) {
                report.warning(
                    "LUME2734",
                    format!("opaque `{name}` should cross the FFI boundary as Handle<{name}>"),
                    Some(name.clone()),
                );
                return;
            }
            if symbols.callbacks.contains(name) {
                return;
            }
            report.warning(
                "LUME2735",
                format!("ffi type `{name}` is treated as an external ABI type"),
                Some(name.clone()),
            );
        }
        _ => {}
    }
}

fn validate_identifier(kind: &str, value: &str, report: &mut ValidationReport) {
    let mut chars = value.chars();
    let valid = chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
    if !valid {
        report.error(
            "LUME2700",
            format!("invalid ffi {kind} identifier `{value}`"),
            Some(value.to_string()),
        );
    }
}

fn validate_unique_named<'a>(
    kind: &str,
    names: impl Iterator<Item = &'a str>,
    report: &mut ValidationReport,
) {
    let mut seen = HashMap::new();
    for name in names {
        let count = seen.entry(name.to_string()).or_insert(0usize);
        *count += 1;
        if *count == 2 {
            report.error(
                "LUME2705",
                format!("duplicate ffi {kind} `{name}`"),
                Some(name.to_string()),
            );
        }
    }
}

fn is_primitive(value: &str) -> bool {
    matches!(
        value,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "Int"
            | "Float"
            | "Number"
            | "StatusCode"
    )
}

fn generic_arg<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}<");
    raw.strip_prefix(&prefix)?.strip_suffix('>')
}

fn strip_type_modifiers(raw: &str) -> &str {
    let mut end = raw.len();
    for marker in [" free=", "free=", " throws", "throws"] {
        if let Some(index) = raw.find(marker) {
            end = end.min(index);
        }
    }
    raw[..end].trim()
}

#[cfg(feature = "ast")]
pub mod ast {
    use super::*;
    use lume_ast::{
        FfiEnumDecl, FfiFunctionDecl, FfiModuleDecl, FfiOpaqueDecl, FfiStructDecl, Param,
    };

    impl FfiRegistry {
        pub fn from_ast(
            modules: &[FfiModuleDecl],
            structs: &[FfiStructDecl],
            enums: &[FfiEnumDecl],
            opaques: &[FfiOpaqueDecl],
        ) -> Self {
            Self {
                modules: modules.iter().map(FfiModule::from_ast).collect(),
                structs: structs.iter().map(FfiStruct::from_ast).collect(),
                enums: enums.iter().map(FfiEnum::from_ast).collect(),
                opaques: opaques.iter().map(FfiOpaque::from_ast).collect(),
            }
        }
    }

    impl FfiModule {
        pub fn from_ast(decl: &FfiModuleDecl) -> Self {
            let mut module = FfiModule::new(&decl.name);
            module.language = decl.language.clone().unwrap_or_else(|| "c".into());
            module.library = decl.library.clone();
            module.header = decl.header.clone();
            module.sources = decl.sources.clone();
            module.runtime = if decl.runtime.is_empty() {
                vec![FfiRuntime::Native]
            } else {
                decl.runtime.iter().map(|value| FfiRuntime::parse(value)).collect()
            };
            module.safety = SafetyLevel::parse(decl.safety.as_deref());
            module.thread_safe = decl.thread_safe;
            module.lock = decl.lock.clone();
            module.functions = decl.functions.iter().map(FfiFunction::from_ast).collect();
            module
        }
    }

    impl FfiStruct {
        pub fn from_ast(decl: &FfiStructDecl) -> Self {
            Self {
                name: decl.name.clone(),
                fields: decl.fields.iter().map(field_from_param).collect(),
                repr: Repr::parse(decl.repr.as_deref()),
            }
        }
    }

    impl FfiEnum {
        pub fn from_ast(decl: &FfiEnumDecl) -> Self {
            Self {
                name: decl.name.clone(),
                variants: decl.variants.clone(),
                repr: Repr::parse(decl.repr.as_deref()),
            }
        }
    }

    impl FfiOpaque {
        pub fn from_ast(decl: &FfiOpaqueDecl) -> Self {
            Self {
                name: decl.name.clone(),
                ownership: Ownership::parse(decl.ownership.as_deref()),
                lifetime: decl.lifetime.clone(),
            }
        }
    }

    impl FfiFunction {
        pub fn from_ast(decl: &FfiFunctionDecl) -> Self {
            Self {
                name: decl.name.clone(),
                params: decl.params.iter().map(field_from_param).collect(),
                return_ty: FfiType::parse(&decl.return_ty),
                ownership: Ownership::parse(decl.ownership.as_deref()),
                callback: decl.callback,
                free: decl
                    .free
                    .clone()
                    .or_else(|| parse_modifier_value(&decl.return_ty, "free")),
                throws: decl
                    .throws
                    .clone()
                    .or_else(|| parse_modifier_value(&decl.return_ty, "throws")),
            }
        }
    }

    fn field_from_param(param: &Param) -> FfiField {
        FfiField {
            name: param.name.clone(),
            ty: FfiType::parse(&param.ty),
        }
    }

    fn parse_modifier_value(raw: &str, name: &str) -> Option<String> {
        let (_, rest) = raw.split_once(name)?;
        let value = rest.trim_start_matches(['=', ' ']);
        value
            .split_whitespace()
            .next()
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_matches(['"', '\'']).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_owned_returns_need_free() {
        let mut module = FfiModule::new("codec");
        module.library = Some("./libcodec.so".into());
        module.functions.push(FfiFunction {
            name: "decode".into(),
            params: vec![FfiField {
                name: "input".into(),
                ty: FfiType::Borrowed(Box::new(FfiType::Bytes)),
            }],
            return_ty: FfiType::Owned(Box::new(FfiType::Bytes)),
            ownership: Ownership::Borrowed,
            callback: false,
            free: None,
            throws: None,
        });
        let report = module.validate_report();
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.code == "LUME2721"));
    }

    #[test]
    fn accepts_declared_opaque_handle_with_free() {
        let registry = FfiRegistry {
            modules: vec![FfiModule {
                name: "codec".into(),
                language: "c".into(),
                library: Some("./libcodec.so".into()),
                header: None,
                sources: Vec::new(),
                runtime: vec![FfiRuntime::Native],
                safety: SafetyLevel::Safe,
                thread_safe: Some(true),
                lock: None,
                structs: Vec::new(),
                enums: Vec::new(),
                opaques: Vec::new(),
                functions: vec![FfiFunction {
                    name: "decoder_new".into(),
                    params: Vec::new(),
                    return_ty: FfiType::Owned(Box::new(FfiType::Handle("Decoder".into()))),
                    ownership: Ownership::Owned,
                    callback: false,
                    free: Some("decoder_free".into()),
                    throws: None,
                }],
            }],
            structs: Vec::new(),
            enums: Vec::new(),
            opaques: vec![FfiOpaque {
                name: "Decoder".into(),
                ownership: Ownership::Owned,
                lifetime: None,
            }],
        };
        assert!(!registry.validate().has_errors());
    }

    #[test]
    fn parses_nested_ffi_types() {
        assert_eq!(
            FfiType::parse("Owned<Handle<Document>>"),
            FfiType::Owned(Box::new(FfiType::Handle("Document".into())))
        );
        assert_eq!(
            FfiType::parse("ConstPtr<u8>"),
            FfiType::ConstPtr(Box::new(FfiType::Primitive("u8".into())))
        );
        assert_eq!(
            FfiType::parse("Owned<Bytes>free=bytes_free"),
            FfiType::Owned(Box::new(FfiType::Bytes))
        );
    }
}
