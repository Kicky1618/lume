use lume_ast::*;
use lume_diagnostics::{Diagnostic, Diagnostics};
use lume_hir::HirProgram;

#[derive(Clone, Debug)]
pub struct LumeProgram {
    pub component: ComponentDecl,
    pub styles: Vec<StyleDecl>,
    pub themes: Vec<ThemeDecl>,
    pub routes: Vec<RouteDecl>,
}

pub fn build(program: &HirProgram) -> Result<LumeProgram, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let styles = program
        .ast
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            Decl::Style(style) => Some(style.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let themes = program
        .ast
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            Decl::Theme(theme) => Some(theme.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let routes = program
        .ast
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            Decl::Route(route) => Some(route.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for decl in &program.ast.declarations {
        if let Decl::Component(component) | Decl::Page(component) = decl {
            return Ok(LumeProgram {
                component: component.clone(),
                styles,
                themes,
                routes,
            });
        }
    }
    diagnostics.push(Diagnostic::error(
        "LUME4001",
        "no component or page found to render",
        Some(program.ast.span),
    ));
    Err(diagnostics)
}

impl LumeProgram {
    pub fn states(&self) -> impl Iterator<Item = &StateDecl> {
        self.component.items.iter().filter_map(|item| match item {
            ComponentItem::State(state) => Some(state),
            _ => None,
        })
    }

    pub fn view(&self) -> Option<&ViewBlock> {
        self.component.items.iter().find_map(|item| match item {
            ComponentItem::View(view) => Some(view),
            _ => None,
        })
    }
}
