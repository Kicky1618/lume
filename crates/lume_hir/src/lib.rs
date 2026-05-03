use lume_ast::Program;

#[derive(Clone, Debug)]
pub struct HirProgram {
    pub ast: Program,
}

pub fn lower(program: Program) -> HirProgram {
    HirProgram { ast: program }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResumeBoundaryId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateScopeId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResumeSymbolId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChunkId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CaptureId(pub String);

#[derive(Clone, Debug)]
pub enum ResumeFallback {
    HydrateBoundary,
    ClientOnly,
    Error,
}

#[derive(Clone, Debug)]
pub struct ResumeBoundary {
    pub id: ResumeBoundaryId,
    pub root_node: String,
    pub state_scopes: Vec<StateScopeId>,
    pub symbols: Vec<ResumeSymbolId>,
    pub fallback: ResumeFallback,
}

#[derive(Clone, Debug)]
pub struct ResumeSymbol {
    pub id: ResumeSymbolId,
    pub event: Option<String>,
    pub action: ActionId,
    pub captures: Vec<CaptureId>,
    pub chunk: Option<ChunkId>,
    pub wasm_export: Option<String>,
}
