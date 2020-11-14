use super::*;
use span::Span;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use util;

#[derive(Debug, PartialEq, Clone)]
pub struct Item<K = ItemKind> {
    pub span: Span,
    pub id: NodeId,
    pub vis: Visibility,
    pub ident: Ident,
    pub kind: K,
}

impl Item {
    pub fn generics(&self) -> Option<&Generics> {
        match &self.kind {
            ItemKind::Impl { generics: g, .. }
            | ItemKind::Fn(_, g, _)
            | ItemKind::Struct(g, _)
            | ItemKind::TypeAlias(g, _)
            | ItemKind::Enum(g, _) => Some(g),
            ItemKind::Mod(..) | ItemKind::Use(..) | ItemKind::Extern(..) => None,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ItemKind {
    /// fn f() {}
    Fn(FnSig, Generics, Option<P<Expr>>),
    /// enum E {}
    Enum(Generics, Vec<Variant>),
    /// struct S {}
    Struct(Generics, VariantKind),
    /// extern "abi" {}
    Extern(Vec<P<ForeignItem>>),
    /// type T = S;
    TypeAlias(Generics, P<Ty>),
    /// mod foo;
    Mod(Module),
    /// use some::path;
    Use(Path),
    /// impl Trait for Type {}
    /// impl Type {}
    Impl { generics: Generics, trait_path: Option<Path>, self_ty: P<Ty>, items: Vec<P<AssocItem>> },
}

#[derive(Debug, PartialEq, Clone)]
pub struct Module {
    pub span: Span,
    pub items: Vec<P<Item>>,
}

impl ItemKind {
    pub fn descr(&self) -> &str {
        match self {
            ItemKind::Fn(_, _, body) => match body {
                Some(_) => "function",
                None => "bodyless function",
            },
            ItemKind::Enum(..) => "enum",
            ItemKind::Struct(..) => "struct",
            ItemKind::Impl { .. } => "impl block",
            ItemKind::Extern(..) => "extern block",
            ItemKind::TypeAlias(..) => "type alias",
            ItemKind::Use(..) => "use import",
            ItemKind::Mod(..) => "module",
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum AssocItemKind {
    Fn(FnSig, Generics, Option<P<Expr>>),
}

impl TryFrom<ItemKind> for AssocItemKind {
    type Error = ItemKind;

    fn try_from(kind: ItemKind) -> Result<Self, Self::Error> {
        match kind {
            ItemKind::Fn(sig, generics, expr) => Ok(Self::Fn(sig, generics, expr)),
            ItemKind::TypeAlias(..) => todo!("assoc types not impl"),
            ItemKind::Use(..)
            | ItemKind::Mod(..)
            | ItemKind::Extern(..)
            | ItemKind::Enum(..)
            | ItemKind::Struct(..)
            | ItemKind::Impl { .. } => Err(kind),
        }
    }
}

pub type AssocItem = Item<AssocItemKind>;

pub type ForeignItem = Item<ForeignItemKind>;

#[derive(Debug, PartialEq, Clone)]
pub enum ForeignItemKind {
    Fn(FnSig, Generics),
}

impl TryFrom<ItemKind> for ForeignItemKind {
    type Error = ItemKind;

    fn try_from(kind: ItemKind) -> Result<Self, Self::Error> {
        match kind {
            ItemKind::Fn(sig, generics, expr) if expr.is_none() => Ok(Self::Fn(sig, generics)),
            _ => Err(kind),
        }
    }
}

impl Display for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ItemKind::Fn(sig, _generics, body) => writeln!(
                f,
                "{} fn {}({}) -> {:?} {}",
                self.vis.node,
                self.ident,
                util::join(&sig.params, ", "),
                sig.ret_ty,
                body.as_ref().unwrap()
            ),
            ItemKind::TypeAlias(generics, ty) =>
                write!(f, "{} type {}<{}> = {}", self.vis.node, self.ident, generics, ty),
            ItemKind::Enum(_generics, _variants) => todo!(),
            ItemKind::Struct(_generics, _variant_kind) => todo!(),
            ItemKind::Impl { .. } => todo!(),
            ItemKind::Extern(_) => todo!(),
            ItemKind::Use(path) => write!(f, "use {}", path),
            ItemKind::Mod(..) => todo!(),
        }
    }
}
