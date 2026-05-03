## 41. 構文の EBNF 概略

```ebnf
Program            = { TopLevelDecl } ;

TopLevelDecl       = ModuleDecl | ImportDecl | ExportDecl | AppDecl | ComponentDecl
                   | PageDecl | LayoutDecl | RouteDecl | ThemeDecl | StyleDecl
                   | TypeDecl | FfiModuleDecl | FfiStructDecl | FfiEnumDecl | FfiOpaqueDecl
                   | ServerActionDecl | FormDecl ;

ModuleDecl         = "module" Identifier [ "strict" ] ;
ImportDecl         = "import" ImportClause "from" StringLiteral ;
ExportDecl         = "export" (TopLevelDecl | Identifier) ;

ComponentDecl      = "component" Identifier [ Params ] ComponentBody ;
PageDecl           = "page" Identifier [ Params ] ComponentBody ;
LayoutDecl         = "layout" Identifier [ Params ] ComponentBody ;
AppDecl            = "app" Identifier Block ;

ComponentBody      = "{" { ComponentItem } "}" ;
ComponentItem      = StateDecl | DerivedDecl | ActionDecl | EffectDecl | QueryDecl
                   | MutationDecl | SearchDecl | MetadataDecl | ViewDecl ;

StateDecl          = "state" Identifier ":" Type "=" Expr ;
DerivedDecl        = "derived" Identifier [ "memo" ] "=" Expr ;
ActionDecl         = [ "async" ] "action" Identifier [ Params ] [ ConcurrencySpec ] Block ;
ConcurrencySpec    = "concurrency" "=" ("enqueue" | "drop" | "restart") ;
EffectDecl         = "effect" [ "[" ExprList "]" ] Block ;
ViewDecl           = "view" ViewBlock ;

RouteDecl          = "route" StringLiteral [ RouteAttrList ] RouteBody ;
RouteBody          = "{" { IndexRouteDecl | RouteDecl | ViewNode } "}" ;
IndexRouteDecl     = "index" ViewBlock ;
RouteAttrList      = { RouteAttr } ;
RouteAttr          = "layout" "=" Identifier | "guard" "=" Expr ;

ServerActionDecl   = "server" "action" Identifier [ Params ] ":" Type { ServerModifier } Block ;
ServerModifier     = ValidateDecl | AuthDecl | CsrfDecl | RateLimitDecl | TransactionDecl
                   | RuntimeDecl | InvalidatesDecl | MaxBodySizeDecl ;

ViewBlock          = "{" { ViewNode } "}" ;
ViewNode           = ElementNode | IfNode | ForNode | MatchNode | SlotUseNode | SlotFillNode ;
ElementNode        = Identifier [ ArgList ] [ AttrList ] [ ViewBlock ] ;
ArgList            = "(" [ Arg { "," Arg } ] ")" ;
AttrList           = { Attribute } ;
Attribute          = Identifier [ "=" Expr ] ;
SlotUseNode        = "slot" [ Identifier ] ;
SlotFillNode       = "slot" ":" Identifier ViewBlock ;

IfNode             = "if" Expr ViewBlock [ "else" (IfNode | ViewBlock) ] ;
ForNode            = "for" Identifier [ "," Identifier ] "in" Expr [ "key" "=" Expr ] ViewBlock ;
MatchNode          = "match" Expr "{" { "case" Expr ViewBlock } [ "default" ViewBlock ] "}" ;

Params             = "(" [ Param { "," Param } ] ")" ;
Param              = Identifier ":" Type [ "=" Expr ] ;
ExprList           = Expr { "," Expr } ;
```

---

## 42. AST 仕様

### 42.1 Program

```rust
pub struct Program {
    pub declarations: Vec<Decl>,
    pub span: Span,
}
```

### 42.2 Decl

```rust
pub enum Decl {
    Module(ModuleDecl),
    Import(ImportDecl),
    Export(ExportDecl),
    App(AppDecl),
    Component(ComponentDecl),
    Page(PageDecl),
    Layout(LayoutDecl),
    Route(RouteDecl),
    Theme(ThemeDecl),
    Style(StyleDecl),
    Type(TypeDecl),
    FfiModule(FfiModuleDecl),
    FfiStruct(FfiStructDecl),
    FfiEnum(FfiEnumDecl),
    FfiOpaque(FfiOpaqueDecl),
    ServerAction(ServerActionDecl),
    Form(FormDecl),
}
```

### 42.3 RouteDecl

```rust
pub struct RouteDecl {
    pub path: String,
    pub attrs: Vec<RouteAttr>,
    pub body: RouteBody,
    pub span: Span,
}

pub struct RouteBody {
    pub index: Option<ViewBlock>,
    pub children: Vec<RouteDecl>,
    pub view_nodes: Vec<ViewNode>,
}
```

### 42.4 ServerActionDecl

```rust
pub struct ServerActionDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_ty: TypeRef,
    pub modifiers: Vec<ServerModifier>,
    pub body: Block,
    pub span: Span,
}
```

### 42.5 ViewNode

```rust
pub enum ViewNode {
    Element(ElementNode),
    If(IfNode),
    For(ForNode),
    Match(MatchNode),
    SlotUse(SlotUseNode),
    SlotFill(SlotFillNode),
    Text(TextNode),
}

pub struct SlotUseNode {
    pub name: Option<Ident>,
    pub span: Span,
}

pub struct SlotFillNode {
    pub name: Ident,
    pub body: ViewBlock,
    pub span: Span,
}
```

---

## 43. メモリモデルと再描画モデル

Lume の UI は状態変化に応じて再評価される。

### 43.1 基本原則

1. `state` の変更は再描画を引き起こす
2. `derived` は依存値が変化した場合に再評価される
3. `view` は純粋関数として扱う
4. `effect` は描画後に実行される

### 43.2 ターゲット依存

Lume runtime は依存グラフに基づく差分再描画モデルに従う。

### 43.3 Resumable metadata

Resumable は v0.1 では新しいソース構文を追加しない。parser / AST は既存の component、state、derived、action、effect、view、event handler を保持し、HIR lowering 以降で resume 用 metadata を生成する。

HIR / IR は以下の派生情報を持てる。

```rust
pub struct ResumeBoundary {
    pub id: ResumeBoundaryId,
    pub root_node: NodeId,
    pub state_scopes: Vec<StateScopeId>,
    pub symbols: Vec<ResumeSymbolId>,
    pub fallback: ResumeFallback,
}

pub struct ResumeSymbol {
    pub id: ResumeSymbolId,
    pub event: Option<EventKind>,
    pub action: ActionId,
    pub captures: Vec<CaptureId>,
    pub chunk: Option<ChunkId>,
    pub wasm_export: Option<String>,
}

pub enum ResumeFallback {
    HydrateBoundary,
    ClientOnly,
    Error,
}
```

`captures` には resumable action が参照する state、props、derived 値、Server Action stub を記録する。capture graph に非直列化値または server-only 値が含まれる場合、resumability 診断を発行する。

---

