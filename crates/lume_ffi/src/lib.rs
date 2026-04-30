#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiModule {
    pub name: String,
    pub language: String,
    pub library: Option<String>,
    pub header: Option<String>,
    pub structs: Vec<FfiStruct>,
    pub enums: Vec<FfiEnum>,
    pub opaques: Vec<FfiOpaque>,
    pub functions: Vec<FfiFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiStruct {
    pub name: String,
    pub fields: Vec<FfiField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiField {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfiEnum {
    pub name: String,
    pub variants: Vec<String>,
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
    pub return_ty: String,
    pub ownership: Ownership,
    pub callback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ownership {
    Borrowed,
    Owned,
    Caller,
    Callee,
    Opaque(String),
}

impl Ownership {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("borrowed").trim() {
            "borrowed" => Self::Borrowed,
            "owned" => Self::Owned,
            "caller" => Self::Caller,
            "callee" => Self::Callee,
            other => Self::Opaque(other.to_string()),
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
            structs: Vec::new(),
            enums: Vec::new(),
            opaques: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.language.as_str(), "c" | "cpp" | "c++") {
            return Err(format!("unsupported ffi language `{}`", self.language));
        }
        for function in &self.functions {
            if function.name.trim().is_empty() {
                return Err("ffi function name cannot be empty".into());
            }
            if function.callback && matches!(function.ownership, Ownership::Owned) {
                return Err(format!(
                    "ffi callback `{}` cannot transfer owned callback memory",
                    function.name
                ));
            }
        }
        Ok(())
    }
}
