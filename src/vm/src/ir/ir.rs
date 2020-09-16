use crate::ast::{Ident, Visibility};
use crate::ir::{self, DefId, ParamIdx, Res};
use crate::lexer::Symbol;
use crate::span::Span;
use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;

#[derive(Debug)]
pub struct Variant<'ir> {
    pub id: ir::Id,
    pub ident: Ident,
    pub span: Span,
    pub kind: ir::VariantKind<'ir>,
}

#[derive(Debug)]
pub enum VariantKind<'ir> {
    Struct(&'ir [ir::FieldDecl<'ir>]),
    Tuple(&'ir [ir::FieldDecl<'ir>]),
    Unit,
}

impl<'ir> VariantKind<'ir> {
    pub fn fields(&self) -> &'ir [ir::FieldDecl<'ir>] {
        match self {
            Self::Struct(fields) | Self::Tuple(fields) => fields,
            Self::Unit => &[],
        }
    }
}

#[derive(Debug)]
pub struct Field<'ir> {
    pub id: ir::Id,
    pub span: Span,
    pub ident: Ident,
    pub expr: &'ir ir::Expr<'ir>,
}

#[derive(Debug)]
pub struct FieldDecl<'ir> {
    pub span: Span,
    pub ident: Ident,
    pub vis: Visibility,
    pub id: ir::Id,
    pub ty: &'ir ir::Ty<'ir>,
}

impl Display for ParamIdx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Generics<'ir> {
    pub span: Span,
    pub params: &'ir [ir::TyParam<'ir>],
}

#[derive(Debug)]
pub struct TyParam<'ir> {
    pub span: Span,
    pub id: ir::Id,
    pub ident: Ident,
    pub index: ParamIdx,
    pub default: Option<&'ir ir::Ty<'ir>>,
}

#[derive(Debug)]
pub struct Body<'ir> {
    pub params: &'ir [ir::Param<'ir>],
    pub expr: &'ir ir::Expr<'ir>,
}

impl<'ir> Body<'ir> {
    pub fn id(&self) -> ir::Id {
        self.expr.id
    }
}

#[derive(Debug)]
pub enum MatchSource {
    Match,
    If,
}

#[derive(Debug)]
pub struct Arm<'ir> {
    pub id: ir::Id,
    pub span: Span,
    pub pat: &'ir ir::Pattern<'ir>,
    pub guard: Option<&'ir ir::Expr<'ir>>,
    pub body: &'ir ir::Expr<'ir>,
}

#[derive(Debug)]
pub struct FnSig<'ir> {
    // rest of parameter information is in `Body`
    pub inputs: &'ir [ir::Ty<'ir>],
    pub output: Option<&'ir ir::Ty<'ir>>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct Path<'ir> {
    pub span: Span,
    pub res: Res,
    pub segments: &'ir [PathSegment<'ir>],
}

#[derive(Debug)]
pub struct Param<'ir> {
    // no type as it is in `FnSig`
    pub span: Span,
    pub id: ir::Id,
    pub pat: &'ir ir::Pattern<'ir>,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct PathSegment<'ir> {
    pub ident: Ident,
    pub id: ir::Id,
    pub pd: PhantomData<&'ir ()>,
}

#[derive(Debug)]
pub struct Block<'ir> {
    pub id: ir::Id,
    pub span: Span,
    pub stmts: &'ir [ir::Stmt<'ir>],
    pub expr: Option<&'ir ir::Expr<'ir>>,
}

pub enum Lvalue {
    Local(ir::Id),
}

#[derive(Debug)]
pub struct Let<'ir> {
    pub id: ir::Id,
    pub span: Span,
    pub pat: &'ir ir::Pattern<'ir>,
    pub ty: Option<&'ir ir::Ty<'ir>>,
    pub init: Option<&'ir ir::Expr<'ir>>,
}
