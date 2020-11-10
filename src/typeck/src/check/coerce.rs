use crate::FnCtx;
use lcore::ty::{self, *};
use span::Span;
use std::ops::Deref;

pub struct Coercion<'a, 'tcx> {
    fcx: &'a FnCtx<'a, 'tcx>,
    span: Span,
    adjustments: Vec<Adjustment<'tcx>>,
}

type CoercionResult<'tcx> = TypeResult<'tcx, Vec<Adjustment<'tcx>>>;

impl<'a, 'tcx> TypeRelation<'tcx> for Coercion<'a, 'tcx> {
    fn tcx(&self) -> TyCtx<'tcx> {
        self.tcx
    }

    fn relate_tys(&mut self, ty: Ty<'tcx>, target: Ty<'tcx>) -> TypeResult<'tcx, Ty<'tcx>> {
        match (ty.kind, target.kind) {
            (ty::Never, _) => {
                self.adjustments.push(Adjustment::new(target, AdjustmentKind::NeverToAny));
                Ok(target)
            }
            (ty::FnDef(def_id, substs), ty::FnPtr(sig)) => {
                let adj = Adjustment::new(target, AdjustmentKind::Cast(PointerCast::ReifyFn));
                self.adjustments.push(adj);

                // equate the signatures
                let fn_sig = self.fn_sig(def_id).subst(self.tcx, substs);
                self.at(self.span).equate(fn_sig, sig)?;

                Ok(target)
            }
            // if it isn't one of the cases for coercion, fallback to `equate`
            _ => self.at(self.span).equate(ty, target),
        }
    }
}

impl<'a, 'tcx> Coercion<'a, 'tcx> {
    fn new(fcx: &'a FnCtx<'a, 'tcx>, span: Span) -> Self {
        Self { fcx, span, adjustments: Default::default() }
    }

    fn coerce(&mut self, ty: Ty<'tcx>, target: Ty<'tcx>) -> CoercionResult<'tcx> {
        if ty.contains_err() || target.contains_err() {
            return Ok(vec![]);
        }
        let ty = self.partially_resolve_ty(self.span, ty);
        self.relate_tys(ty, target)?;
        return Ok(vec![]);
    }
}

impl<'a, 'tcx> FnCtx<'a, 'tcx> {
    /// this is analogous to equate, but only requires that `ty` is coercible into `target`
    pub fn coerce(&self, expr: &ir::Expr<'tcx>, ty: Ty<'tcx>, target: Ty<'tcx>) {
        if let Err(err) = self.try_coerce(expr, ty, target) {
            self.emit_ty_err(expr.span, err);
        }
    }

    /// attempts to coerce `expr: ty` to type `target`
    /// returns a type error on failure
    /// returns the target type on success
    /// and records the appropriate adjustments
    fn try_coerce(
        &self,
        expr: &ir::Expr<'tcx>,
        ty: Ty<'tcx>,
        target: Ty<'tcx>,
    ) -> TypeResult<'tcx, Ty<'tcx>> {
        Coercion::new(self, expr.span).coerce(ty, target).and_then(|adjustments| {
            self.record_adjustments(expr.id, adjustments);
            Ok(if ty.contains_err() { self.set_ty_err() } else { target })
        })
    }
}

impl<'a, 'tcx> Deref for Coercion<'a, 'tcx> {
    type Target = FnCtx<'a, 'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.fcx
    }
}
