use crate::ast::{Ident, Visibility};
use crate::ir::{DefId, ParamIdx};
use crate::ty::{SubstsRef, TypeFoldable, TypeVisitor};
use crate::typeck::inference::TyVid;
use crate::{span::Span, util};
use bitflags::bitflags;
use indexed_vec::{Idx, IndexVec};
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::{marker::PhantomData, ptr};

pub type Ty<'tcx> = &'tcx TyS<'tcx>;

#[derive(Debug, Eq)]
pub struct TyS<'tcx> {
    pub flags: TyFlags,
    pub kind: TyKind<'tcx>,
}

impl<'tcx> TyS<'tcx> {
    pub fn is_unit(&self) -> bool {
        match self.kind {
            TyKind::Tuple(tys) => tys.is_empty(),
            _ => false,
        }
    }
}

/// visitor that searches for inference variables
struct InferenceVarVisitor;

impl<'tcx> TyS<'tcx> {
    pub fn expect_scheme(&self) -> (Forall<'tcx>, Ty<'tcx>) {
        match self.kind {
            TyKind::Scheme(forall, ty) => (forall, ty),
            _ => panic!("expected TyKind::Scheme, found {}", self),
        }
    }

    pub fn expect_fn(&self) -> (SubstsRef<'tcx>, Ty<'tcx>) {
        match self.kind {
            TyKind::Fn(params, ret) => (params, ret),
            _ => panic!("expected TyKind::Fn, found {}", self),
        }
    }

    pub fn expect_adt(&self) -> &'tcx AdtTy<'tcx> {
        match self.kind {
            TyKind::Adt(adt, _) => adt,
            _ => panic!("expected TyKind::Adt, found {}", self),
        }
    }
}

impl<'tcx> Hash for TyS<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self as *const TyS<'tcx>).hash(state)
    }
}

/// we can perform equality using pointers as we ensure that at most one of each TyS is allocated
/// (by doing a deep compare on TyKind during allocation)
impl<'tcx> PartialEq for TyS<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self, other)
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum TyKind<'tcx> {
    /// bool
    Bool,
    /// char
    Char,
    /// float
    Float,
    /// Int
    Int,
    Error,
    Never,
    /// [<ty>]
    Array(Ty<'tcx>),
    /// fn(<ty>...) -> <ty>
    Fn(SubstsRef<'tcx>, Ty<'tcx>),
    Tuple(SubstsRef<'tcx>),
    Infer(InferTy),
    Param(ParamTy),
    Adt(&'tcx AdtTy<'tcx>, SubstsRef<'tcx>),
    Scheme(Forall<'tcx>, Ty<'tcx>),
}

newtype_index!(VariantIdx);

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct AdtTy<'tcx> {
    pub def_id: DefId,
    pub ident: Ident,
    pub variants: IndexVec<VariantIdx, VariantTy<'tcx>>,
}

impl<'tcx> AdtTy<'tcx> {
    pub fn only_variant(&self) -> &VariantTy<'tcx> {
        assert_eq!(self.variants.len(), 1);
        &self.variants[VariantIdx::new(0)]
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct VariantTy<'tcx> {
    pub ident: Ident,
    pub fields: &'tcx [FieldTy<'tcx>],
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct FieldTy<'tcx> {
    pub def_id: DefId,
    pub ident: Ident,
    pub vis: Visibility,
    pub ty: Ty<'tcx>,
}

bitflags! {
    pub struct TyFlags: u32  {
        const HAS_ERROR = 1 << 0;
        const HAS_INFER = 1 << 1;
        const HAS_PARAM = 1 << 2;
    }
}

pub trait TyFlag {
    fn ty_flags(&self) -> TyFlags;
}

impl<'tcx> TyFlag for TyS<'tcx> {
    fn ty_flags(&self) -> TyFlags {
        self.kind.ty_flags()
    }
}

impl<'tcx> TyFlag for SubstsRef<'tcx> {
    fn ty_flags(&self) -> TyFlags {
        self.iter().fold(TyFlags::empty(), |acc, ty| acc | ty.ty_flags())
    }
}

impl<'tcx> TyS<'tcx> {
    pub fn has_flags(&self, flags: TyFlags) -> bool {
        self.flags.intersects(flags)
    }

    pub fn has_infer_vars(&self) -> bool {
        self.has_flags(TyFlags::HAS_INFER)
    }

    pub fn has_ty_params(&self) -> bool {
        self.has_flags(TyFlags::HAS_PARAM)
    }

    pub fn contains_err(&self) -> bool {
        self.has_flags(TyFlags::HAS_ERROR)
    }
}

impl<'tcx> TyFlag for TyKind<'tcx> {
    fn ty_flags(&self) -> TyFlags {
        match self {
            TyKind::Array(ty) => ty.kind.ty_flags(),
            TyKind::Tuple(tys) => tys.ty_flags(),
            TyKind::Fn(params, ret) => params.ty_flags() | ret.ty_flags(),
            TyKind::Scheme(_, ty) => ty.ty_flags(),
            TyKind::Infer(_) => TyFlags::HAS_INFER,
            TyKind::Param(_) => TyFlags::HAS_PARAM,
            TyKind::Error => TyFlags::HAS_ERROR,
            TyKind::Adt(_, substs) => substs.ty_flags(),
            TyKind::Float | TyKind::Never | TyKind::Bool | TyKind::Char | TyKind::Int =>
                TyFlags::empty(),
        }
    }
}

impl<'tcx> Display for TyKind<'tcx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TyKind::Fn(params, ret) =>
                write!(f, "fn({})->{}", util::join2(params.into_iter(), ","), ret),
            TyKind::Infer(infer_ty) => write!(f, "{}", infer_ty),
            TyKind::Array(ty) => write!(f, "[{}]", ty),
            TyKind::Tuple(tys) => write!(f, "({})", tys),
            TyKind::Param(param_ty) => write!(f, "{}", param_ty),
            TyKind::Scheme(forall, ty) => write!(f, "∀{}.{}", forall, ty),
            TyKind::Adt(adt, _) => write!(f, "{}", adt.ident),
            TyKind::Bool => write!(f, "bool"),
            TyKind::Char => write!(f, "char"),
            TyKind::Int => write!(f, "int"),
            TyKind::Float => write!(f, "float"),
            TyKind::Error => write!(f, "err"),
            TyKind::Never => write!(f, "!"),
        }
    }
}

/// the current representation of type parameters are their DefId
#[derive(Debug, Eq, Copy, Hash, PartialEq, Clone)]
pub struct Forall<'tcx> {
    pub binders: &'tcx [ParamIdx],
}

impl<'tcx> Display for Forall<'tcx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "τ{}", util::join2(self.binders.iter(), ","))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamTy {
    pub def_id: DefId,
    pub idx: ParamIdx,
}

impl Display for ParamTy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "τ{}", self.idx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InferTy {
    TyVar(TyVid),
    // IntVar(IntVid),
    // FloatVar(FloatVid),
}

impl Display for InferTy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TyVar(vid) => write!(f, "?{}", vid),
        }
    }
}

impl<'tcx> Display for TyS<'tcx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

/// typed constant value
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Const<'tcx> {
    pub kind: ConstKind,
    marker: PhantomData<&'tcx ()>,
}

impl<'tcx> std::ops::Add for Const<'tcx> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (self.kind, rhs.kind) {
            (ConstKind::Float(x), ConstKind::Float(y)) => Self::new(ConstKind::Float(x + y)),
            (ConstKind::Int(x), ConstKind::Int(y)) => Self::new(ConstKind::Int(x + y)),
            _ => panic!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstKind {
    Float(f64),
    Int(i64),
    Bool(bool),
    Unit,
}

impl<'tcx> Const<'tcx> {
    pub fn new(kind: ConstKind) -> Self {
        Self { kind, marker: PhantomData }
    }

    pub fn unit() -> Self {
        Self::new(ConstKind::Unit)
    }
}

impl<'tcx> Display for Const<'tcx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.kind {
            ConstKind::Float(d) => write!(f, "{:?}", d),
            ConstKind::Int(i) => write!(f, "{}", i),
            ConstKind::Bool(b) => write!(f, "{}", b),
            ConstKind::Unit => write!(f, "()"),
        }
    }
}
