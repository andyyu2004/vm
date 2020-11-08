mod expr;
mod item;
mod methods;
mod pat;
mod path;
mod stmt;

use crate::TyConv;
use ast::Mutability;
use error::{LError, LResult};
use infer::{InferCtx, InferCtxBuilder, TyCtxtInferExt};
use ir::{self, DefId};
use lcore::queries::Queries;
use lcore::ty::*;
use rustc_hash::FxHashMap;
use span::Span;
use std::cell::RefCell;
use std::ops::Deref;

pub fn provide(queries: &mut Queries) {
    item::provide(queries);
    *queries = Queries { typeck, ..*queries }
}

/// checks the bodies of item
fn typeck<'tcx>(tcx: TyCtx<'tcx>, def_id: DefId) -> LResult<&'tcx TypeckTables<'tcx>> {
    let body = tcx.defs().body(def_id);
    InheritedCtx::build(tcx, def_id).enter(|inherited| {
        let fcx = inherited.check_fn_item(def_id, body);
        if tcx.sess.has_errors() {
            return Err(LError::ErrorReported);
        }
        Ok(fcx.resolve_inference_variables(body))
    })
}

pub struct FnCtx<'a, 'tcx> {
    inherited: &'a InheritedCtx<'a, 'tcx>,
    unsafe_ctx: bool,
    crate param_tys: SubstsRef<'tcx>,
    crate ret_ty: Ty<'tcx>,
}

impl<'a, 'tcx> FnCtx<'a, 'tcx> {
    pub fn new(inherited: &'a InheritedCtx<'a, 'tcx>, fn_ty: Ty<'tcx>) -> Self {
        let (param_tys, ret_ty) = fn_ty.expect_fn();
        Self { inherited, param_tys, ret_ty, unsafe_ctx: false }
    }

    crate fn in_unsafe_ctx(&self) -> bool {
        self.unsafe_ctx
    }

    crate fn with_unsafe_ctx<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = self.unsafe_ctx;
        self.unsafe_ctx = true;
        let ret = f(self);
        self.unsafe_ctx = old;
        ret
    }
}

impl<'a, 'tcx> Deref for FnCtx<'a, 'tcx> {
    type Target = InheritedCtx<'a, 'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.inherited
    }
}

impl<'a, 'tcx> FnCtx<'a, 'tcx> {
    pub fn lower_tys(&self, ir_tys: &[ir::Ty<'tcx>]) -> &'tcx [Ty<'tcx>] {
        self.tcx.mk_substs(ir_tys.iter().map(|ty| self.ir_ty_to_ty(ty)))
    }
}

impl<'a, 'tcx> TyConv<'tcx> for InferCtx<'a, 'tcx> {
    fn tcx(&self) -> TyCtx<'tcx> {
        self.tcx
    }

    fn infer_ty(&self, span: Span) -> Ty<'tcx> {
        self.new_infer_var(span)
    }
}

impl<'a, 'tcx> Deref for InheritedCtx<'a, 'tcx> {
    type Target = InferCtx<'a, 'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.infcx
    }
}

/// context that is shared between functions
/// nested lambdas will have their own `FnCtx` but will share `Inherited` will outer lambdas as
/// well as the outermost fn item
pub struct InheritedCtx<'a, 'tcx> {
    crate infcx: &'a InferCtx<'a, 'tcx>,
    locals: RefCell<FxHashMap<ir::Id, LocalTy<'tcx>>>,
}

pub struct InheritedCtxBuilder<'tcx> {
    infcx: InferCtxBuilder<'tcx>,
}

#[derive(Debug, Clone, Copy)]
pub struct LocalTy<'tcx> {
    pub ty: Ty<'tcx>,
    pub mtbl: Mutability,
}

impl<'tcx> LocalTy<'tcx> {
    pub fn new(ty: Ty<'tcx>, mtbl: Mutability) -> Self {
        Self { ty, mtbl }
    }
}

impl<'tcx> InheritedCtxBuilder<'tcx> {
    pub fn enter<R>(&mut self, f: impl for<'a> FnOnce(InheritedCtx<'a, 'tcx>) -> R) -> R {
        self.infcx.enter(|infcx| f(InheritedCtx::new(&infcx)))
    }
}

impl<'a, 'tcx> InheritedCtx<'a, 'tcx> {
    pub fn new(infcx: &'a InferCtx<'a, 'tcx>) -> Self {
        Self { infcx, locals: Default::default() }
    }

    pub fn build(tcx: TyCtx<'tcx>, def_id: DefId) -> InheritedCtxBuilder<'tcx> {
        InheritedCtxBuilder { infcx: tcx.infer_ctx(def_id) }
    }

    /// top level entry point for typechecking a function item
    pub fn check_fn_item(&'a self, def_id: DefId, body: &ir::Body<'tcx>) -> FnCtx<'a, 'tcx> {
        let fn_ty = self.tcx.type_of(def_id);
        // don't instantiate anything and typeck the body using the param tys
        // don't know if this is a good idea
        let (_forall, ty) = fn_ty.expect_scheme();
        self.check_fn(ty, body)
    }

    pub fn check_fn(&'a self, fn_ty: Ty<'tcx>, body: &ir::Body<'tcx>) -> FnCtx<'a, 'tcx> {
        let mut fcx = FnCtx::new(self, fn_ty);
        fcx.check_body(body);
        fcx
    }

    pub fn def_local(&self, id: ir::Id, mtbl: Mutability, ty: Ty<'tcx>) -> Ty<'tcx> {
        debug!("deflocal {:?} : {}", id, ty);
        self.locals.borrow_mut().insert(id, LocalTy::new(ty, mtbl));
        ty
    }

    pub fn local_ty(&self, id: ir::Id) -> LocalTy<'tcx> {
        debug!("lookup ty for local {:?}", id);
        self.locals.borrow().get(&id).cloned().expect("no entry for local variable")
    }
}
