use crate::queries::QueryCtx;
use crate::ty::*;
use crate::*;
use ast::Ident;
use index::IndexVec;
use ir::CtorKind;
use ir::{DefId, FieldIdx, ParamIdx, VariantIdx};
use itertools::Itertools;
use resolve::Resolutions;
use rustc_hash::FxHashMap;
use session::Session;
use std::cell::{Cell, RefCell};
use std::ops::Deref;

/// typing context
#[derive(Copy, Clone)]
pub struct TyCtx<'tcx> {
    gcx: &'tcx GlobalCtx<'tcx>,
}

/// thread local storage for the TyCtx<'tcx>
pub mod tls {
    use super::*;

    // stores a pointer to the gcx
    thread_local! (static GCX: Cell<usize> = Cell::new(0));

    crate fn enter_tcx<'tcx, R>(gcx: &GlobalCtx<'tcx>, f: impl FnOnce(TyCtx<'tcx>) -> R) -> R {
        // permanently sets the pointer
        GCX.with(|ptr| ptr.set(gcx as *const _ as _));
        with_tcx(f)
    }

    pub fn with_tcx<'tcx, R>(f: impl FnOnce(TyCtx<'tcx>) -> R) -> R {
        let gcx_ptr = GCX.with(|gcx| gcx.get()) as *const GlobalCtx;
        let gcx = unsafe { &*gcx_ptr };
        let tcx = TyCtx { gcx };
        f(tcx)
    }
}

impl<'tcx> TyCtx<'tcx> {
    pub fn alloc<T>(self, t: T) -> &'tcx T
    where
        T: ArenaAllocatable<'tcx>,
    {
        // these types have their own typed arena
        debug_assert!(std::any::type_name_of_val(&t) != std::any::type_name::<TyKind>());
        debug_assert!(std::any::type_name_of_val(&t) != std::any::type_name::<Const>());
        self.interners.arena.alloc(t)
    }

    pub fn alloc_iter<I, T>(self, iter: I) -> &'tcx [T]
    where
        I: IntoIterator<Item = T>,
        T: ArenaAllocatable<'tcx>,
    {
        self.interners.arena.alloc_from_iter(iter)
    }

    pub fn mk_struct_ty(
        self,
        def_id: DefId,
        ident: Ident,
        variant: VariantTy<'tcx>,
    ) -> &'tcx AdtTy<'tcx> {
        self.mk_adt(def_id, AdtKind::Struct, ident, std::iter::once(variant).collect())
    }

    pub fn mk_enum_ty(
        self,
        def_id: DefId,
        ident: Ident,
        variants: IndexVec<VariantIdx, VariantTy<'tcx>>,
    ) -> &'tcx AdtTy<'tcx> {
        self.mk_adt(def_id, AdtKind::Enum, ident, variants)
    }

    pub fn mk_adt(
        self,
        def_id: DefId,
        kind: AdtKind,
        ident: Ident,
        variants: IndexVec<VariantIdx, VariantTy<'tcx>>,
    ) -> &'tcx AdtTy<'tcx> {
        debug_assert!(kind != AdtKind::Struct || variants.len() == 1);
        self.arena.alloc(AdtTy { ident, def_id, kind, variants })
    }

    pub fn mk_adt_ty(self, adt_ty: &'tcx AdtTy<'tcx>, substs: SubstsRef<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Adt(adt_ty, substs))
    }

    pub fn mk_opaque_ty(self, def: DefId, substs: SubstsRef<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Opaque(def, substs))
    }

    pub fn mk_array_ty(self, ty: Ty<'tcx>, n: usize) -> Ty<'tcx> {
        self.mk_ty(TyKind::Array(ty, n))
    }

    pub fn mk_ty_scheme(self, forall: &'tcx Generics<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Scheme(forall, ty))
    }

    pub fn mk_ty(self, ty: TyKind<'tcx>) -> Ty<'tcx> {
        self.interners.intern_ty(ty)
    }

    pub fn mk_fn_ty(self, params: SubstsRef<'tcx>, ret: Ty<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Fn(params, ret))
    }

    pub fn mk_ptr_ty(self, ty: Ty<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Ptr(ty))
    }

    pub fn mk_box_ty(self, ty: Ty<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Box(ty))
    }

    pub fn mk_ty_param(self, def_id: DefId, idx: ParamIdx, ident: Ident) -> Ty<'tcx> {
        self.mk_ty(TyKind::Param(ParamTy { def_id, idx, ident }))
    }

    /// returns the new type after applying a projection
    pub fn apply_projection(self, ty: Ty<'tcx>, proj: Projection<'tcx>) -> Ty<'tcx> {
        match proj {
            Projection::Deref => ty.deref_ty(),
            Projection::Field(_, ty) => ty,
            Projection::PointerCast(ty) => ty,
        }
    }

    pub fn mk_prim_ty(self, prim_ty: ir::PrimTy) -> Ty<'tcx> {
        match prim_ty {
            ir::PrimTy::Char => self.types.char,
            ir::PrimTy::Bool => self.types.bool,
            ir::PrimTy::Float => self.types.float,
            ir::PrimTy::Int => self.types.int,
        }
    }

    pub fn mk_tup(self, substs: SubstsRef<'tcx>) -> Ty<'tcx> {
        self.mk_ty(TyKind::Tuple(substs))
    }

    pub fn mk_tup_iter<I>(self, iter: I) -> Ty<'tcx>
    where
        I: Iterator<Item = Ty<'tcx>>,
    {
        self.mk_tup(self.mk_substs(iter))
    }

    pub fn concat_generics(
        self,
        g: &ty::Generics<'tcx>,
        h: &ty::Generics<'tcx>,
    ) -> &'tcx ty::Generics<'tcx> {
        let mut params = g.params.to_vec();
        params.extend(h.params);
        self.alloc(ty::Generics { params: self.alloc_iter(params) })
    }

    pub fn mk_ty_err(self) -> Ty<'tcx> {
        self.mk_ty(TyKind::Error)
    }

    pub fn project_field(
        self,
        lvalue: mir::Lvalue<'tcx>,
        field: FieldIdx,
        ty: Ty<'tcx>,
    ) -> mir::Lvalue<'tcx> {
        self.project_lvalue(lvalue, Projection::Field(field, ty))
    }

    pub fn project_deref(self, lvalue: mir::Lvalue<'tcx>) -> mir::Lvalue<'tcx> {
        self.project_lvalue(lvalue, Projection::Deref)
    }

    pub fn project_lvalue(
        self,
        mir::Lvalue { id, projs }: mir::Lvalue<'tcx>,
        proj: Projection<'tcx>,
    ) -> mir::Lvalue<'tcx> {
        let mut projs = projs.to_vec();
        projs.push(proj);
        mir::Lvalue { id, projs: self.intern_lvalue_projections(&projs) }
    }

    pub fn mk_substs<I>(self, iter: I) -> SubstsRef<'tcx>
    where
        I: IntoIterator<Item = Ty<'tcx>>,
    {
        self.intern_substs(&iter.into_iter().collect_vec())
    }

    pub fn intern_lvalue_projections(
        self,
        projs: &[Projection<'tcx>],
    ) -> &'tcx List<Projection<'tcx>> {
        if projs.is_empty() {
            List::empty()
        } else {
            self.interners.intern_lvalue_projections(projs)
        }
    }

    pub fn mk_const_int(self, i: i64) -> &'tcx Const<'tcx> {
        self.mk_const(ConstKind::Int(i))
    }

    pub fn mk_const_float(self, f: f64) -> &'tcx Const<'tcx> {
        self.mk_const(ConstKind::Float(f))
    }

    pub fn mk_const_bool(self, b: bool) -> &'tcx Const<'tcx> {
        self.mk_const(ConstKind::Bool(b))
    }

    pub fn mk_const_discr(self, discr: i16) -> &'tcx Const<'tcx> {
        self.mk_const(ConstKind::Discr(discr))
    }

    pub fn mk_const_unit(self) -> &'tcx Const<'tcx> {
        self.mk_const(ConstKind::Unit)
    }

    pub fn mk_const(self, kind: ConstKind) -> &'tcx Const<'tcx> {
        let ty = match kind {
            ConstKind::Float(_) => self.types.float,
            ConstKind::Int(_) => self.types.int,
            ConstKind::Discr(_) => self.types.discr,
            ConstKind::Bool(_) => self.types.bool,
            ConstKind::Unit => self.types.unit,
        };
        self.intern_const(Const { kind, ty })
    }

    pub fn intern_const(self, c: Const<'tcx>) -> &'tcx Const<'tcx> {
        self.interners.intern_const(c)
    }

    pub fn variant_ty(
        self,
        ident: Ident,
        ctor: Option<DefId>,
        variant_kind: &ir::VariantKind<'tcx>,
    ) -> VariantTy<'tcx> {
        let mut seen = FxHashMap::default();
        let fields = self.alloc_iter(variant_kind.fields().iter().map(|f| {
            if let Some(span) = seen.insert(f.ident, f.span) {
                self.sess.emit_error(
                    vec![f.span, span],
                    TypeError::FieldAlreadyDeclared(f.ident, ident),
                );
            }
            // TODO check the number of generic params are correct
            FieldTy { def_id: f.id.def, ident: f.ident, vis: f.vis, ir_ty: f.ty }
        }));
        VariantTy { ctor, ident, fields, ctor_kind: CtorKind::from(variant_kind) }
    }

    pub fn intern_substs(self, slice: &[Ty<'tcx>]) -> SubstsRef<'tcx> {
        if slice.is_empty() { Substs::empty() } else { self.interners.intern_substs(slice) }
    }
}

pub struct GlobalCtx<'tcx> {
    pub arena: &'tcx Arena<'tcx>,
    pub sess: &'tcx Session,
    pub ir: &'tcx ir::Ir<'tcx>,
    pub types: CommonTypes<'tcx>,
    pub resolutions: Resolutions<'tcx>,
    queries: QueryCtx<'tcx>,
    interners: CtxInterners<'tcx>,
    // caches below
    // these should be populated during collection
    /// maps an item's DefId to its type
    collected_tys: RefCell<FxHashMap<DefId, Ty<'tcx>>>,
    /// maps a types DefId to the DefId's of its inherent impl blocks
    inherent_impls: RefCell<FxHashMap<DefId, Vec<DefId>>>,
    generics: RefCell<FxHashMap<DefId, &'tcx Generics<'tcx>>>,
}

impl<'tcx> GlobalCtx<'tcx> {
    pub fn new(
        ir: &'tcx ir::Ir<'tcx>,
        arena: &'tcx Arena<'tcx>,
        resolutions: Resolutions<'tcx>,
        sess: &'tcx Session,
        queries: QueryCtx<'tcx>,
    ) -> Self {
        let interners = CtxInterners::new(arena);
        let types = CommonTypes::new(&interners);
        Self {
            types,
            ir,
            arena,
            interners,
            resolutions,
            sess,
            queries,
            collected_tys: Default::default(),
            inherent_impls: Default::default(),
            generics: Default::default(),
        }
    }

    pub fn enter_tcx<R>(&self, f: impl FnOnce(TyCtx<'tcx>) -> R) -> R {
        tls::enter_tcx(self, f)
    }
}

/// collection methods
impl<'tcx> TyCtx<'tcx> {
    /// write collected ty to tcx map
    pub fn collect_ty(self, def: DefId, ty: Ty<'tcx>) -> Ty<'tcx> {
        info!("collect item {}: {}", def, ty);
        assert!(self.collected_tys.borrow_mut().insert(def, ty).is_none());
        ty
    }

    /// finds the type of an item that was obtained during the collection phase
    pub fn collected_ty(self, def_id: DefId) -> Ty<'tcx> {
        self.collected_ty_opt(def_id).unwrap_or_else(|| {
            panic!("no collected type found for `{:?}`", self.defs().get(def_id))
        })
    }

    pub fn collected_ty_opt(self, def_id: DefId) -> Option<Ty<'tcx>> {
        self.collected_tys.borrow().get(&def_id).copied()
    }

    // pub fn generics_of(self, def_id: DefId) -> &'tcx Generics<'tcx> {
    //     self.generics.borrow().get(&def_id).unwrap()
    // }

    pub fn cache_generics(self, def_id: DefId, generics: &'tcx Generics<'tcx>) {
        assert!(self.generics.borrow_mut().insert(def_id, generics).is_none());
    }

    // we just set the refcell once to avoid repeated borrows
    pub fn set_inherent_impls(self, inherent_impls: FxHashMap<DefId, Vec<DefId>>) {
        debug_assert!(self.inherent_impls.borrow().is_empty());
        *self.inherent_impls.borrow_mut() = inherent_impls;
    }

    pub fn inherent_impls_of_def(self, def_id: DefId) -> Vec<DefId> {
        self.inherent_impls.borrow().get(&def_id).cloned().unwrap_or_else(Vec::new)
    }
}

/// debug
impl<'tcx> TyCtx<'tcx> {
    pub fn dump_collected_tys(self) {
        println!("type collection results");
        self.collected_tys.borrow().iter().for_each(|(k, v)| println!("{:?}: {}", k, v));
    }
}

pub struct CommonTypes<'tcx> {
    pub char: Ty<'tcx>,
    pub bool: Ty<'tcx>,
    pub unit: Ty<'tcx>,
    pub discr: Ty<'tcx>,
    pub float: Ty<'tcx>,
    pub int: Ty<'tcx>,
    pub never: Ty<'tcx>,
    /// type of `main` must be `fn() -> int`
    pub main: Ty<'tcx>,
}

impl<'tcx> CommonTypes<'tcx> {
    fn new(interners: &CtxInterners<'tcx>) -> CommonTypes<'tcx> {
        let mk = |ty| interners.intern_ty(ty);
        let int = mk(TyKind::Int);
        CommonTypes {
            bool: mk(TyKind::Bool),
            char: mk(TyKind::Char),
            discr: mk(TyKind::Discr),
            never: mk(TyKind::Never),
            float: mk(TyKind::Float),
            main: mk(TyKind::Fn(Substs::empty(), int)),
            unit: mk(TyKind::Tuple(Substs::empty())),
            int,
        }
    }
}

impl<'tcx> Deref for TyCtx<'tcx> {
    type Target = GlobalCtx<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.gcx
    }
}

impl<'tcx> Deref for GlobalCtx<'tcx> {
    type Target = QueryCtx<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.queries
    }
}
