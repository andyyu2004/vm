use super::{Ident, NodeId, P};
use crate::span::Span;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub struct Pattern {
    pub span: Span,
    pub id: NodeId,
    pub kind: PatternKind,
}

#[derive(Debug, PartialEq, Clone, Eq, Hash, Copy)]
pub enum Mutability {
    Mut,
    Imm,
}

impl Display for Mutability {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Mutability::Mut => write!(f, "mut "),
            Mutability::Imm => Ok(()),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub enum PatternKind {
    /// _
    Wildcard,
    /// ident ( @ <subpattern> )?
    Ident(Ident, Option<P<Pattern>>, Mutability),
    Paren(P<Pattern>),
    Tuple(Vec<P<Pattern>>),
}
