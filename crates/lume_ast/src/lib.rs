use lume_span::Span;

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub declarations: Vec<Decl>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Module(String),
    Import { items: Vec<String>, from: String },
    Component(ComponentDecl),
    Page(ComponentDecl),
    Layout(ComponentDecl),
    Route(RouteDecl),
    Theme(ThemeDecl),
    Style(StyleDecl),
    Type(ReservedDecl),
    App(ReservedDecl),
    ServerAction(ReservedDecl),
    Form(ReservedDecl),
    Ffi(ReservedDecl),
    Export(Box<Decl>),
    Reserved(ReservedDecl),
}

#[derive(Clone, Debug)]
pub struct ReservedDecl {
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct ComponentDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub items: Vec<ComponentItem>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: String,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ComponentItem {
    State(StateDecl),
    Derived {
        name: String,
        expr: Expr,
        span: Span,
    },
    Action(ActionDecl),
    View(ViewBlock),
    Reserved(ReservedDecl),
}

#[derive(Clone, Debug)]
pub struct StateDecl {
    pub name: String,
    pub ty: String,
    pub init: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ActionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub is_async: bool,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Assign {
        target: String,
        op: AssignOp,
        expr: Expr,
        span: Span,
    },
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
}

#[derive(Clone, Debug, Default)]
pub struct ViewBlock {
    pub nodes: Vec<ViewNode>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ViewNode {
    Element(ElementNode),
    If(IfNode),
    For(ForNode),
    Match(ReservedDecl),
    SlotUse {
        name: Option<String>,
        span: Span,
    },
    SlotFill {
        name: String,
        body: ViewBlock,
        span: Span,
    },
    Event(EventNode),
    Text(TextNode),
}

#[derive(Clone, Debug)]
pub struct ElementNode {
    pub name: String,
    pub args: Vec<Arg>,
    pub attrs: Vec<Attribute>,
    pub children: Option<ViewBlock>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EventNode {
    pub event: String,
    pub params: Vec<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct IfNode {
    pub condition: Expr,
    pub then_block: ViewBlock,
    pub else_block: Option<ViewBlock>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ForNode {
    pub item: String,
    pub index: Option<String>,
    pub iterable: Expr,
    pub key: Option<Expr>,
    pub body: ViewBlock,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TextNode {
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Arg {
    Positional(Expr),
    Named(String, Expr),
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StyleDecl {
    pub name: String,
    pub properties: Vec<StyleProperty>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StyleProperty {
    pub name: String,
    pub value: Expr,
    pub pseudo: Option<String>,
    pub media: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ThemeDecl {
    pub name: String,
    pub tokens: Vec<ThemeToken>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ThemeToken {
    pub category: String,
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct RouteDecl {
    pub path: String,
    pub view: Option<ViewBlock>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub raw: String,
    pub span: Span,
}
