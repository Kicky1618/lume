use lume_ast::Program;

#[derive(Clone, Debug)]
pub struct HirProgram {
    pub ast: Program,
}

pub fn lower(program: Program) -> HirProgram {
    HirProgram { ast: program }
}
