//! conversion of `ir::Ty` to `lcore::ty::Ty`

use ir::{DefKind, QPath, Res};
use lcore::ty::{FnSig, Generics, Subst, Substs, Ty, TyCtx, TyParam, TypeError};
use span::Span;

/// refer to module comments
pub trait TyConv<'tcx> {
    fn tcx(&self) -> TyCtx<'tcx>;

    fn infer_ty(&self, span: Span) -> Ty<'tcx>;

    fn ir_ty_to_ty(&self, ir_ty: &ir::Ty<'tcx>) -> Ty<'tcx> {
        let tcx = self.tcx();
        match &ir_ty.kind {
            ir::TyKind::Err => tcx.mk_ty_err(),
            ir::TyKind::Box(ty) => tcx.mk_box_ty(self.ir_ty_to_ty(ty)),
            ir::TyKind::Fn(params, ret) => tcx.mk_fn_ptr(FnSig {
                params: tcx.mk_substs(params.iter().map(|ty| self.ir_ty_to_ty(ty))),
                ret: ret.map(|ty| self.ir_ty_to_ty(ty)).unwrap_or(tcx.types.unit),
            }),
            ir::TyKind::Path(qpath) => self.qpath_to_ty(qpath),
            ir::TyKind::Tuple(tys) => tcx.mk_tup_iter(tys.iter().map(|ty| self.ir_ty_to_ty(ty))),
            ir::TyKind::Ptr(ty) => tcx.mk_ptr_ty(self.ir_ty_to_ty(ty)),
            ir::TyKind::Array(_ty) => {
                // tcx.mk_array_ty(self.ir_ty_to_ty(ty), todo!()),
                todo!();
            }
            ir::TyKind::Infer => self.infer_ty(ir_ty.span),
        }
    }

    fn qpath_to_ty(&self, qpath: &ir::QPath<'tcx>) -> Ty<'tcx> {
        match qpath {
            QPath::Resolved(path) => self.path_to_ty(path),
            QPath::TypeRelative(_, _) => todo!(),
        }
    }

    fn path_to_ty(&self, path: &ir::Path<'tcx>) -> Ty<'tcx> {
        let tcx = self.tcx();
        match path.res {
            Res::PrimTy(prim_ty) => tcx.mk_prim_ty(prim_ty),
            Res::Def(def_id, def_kind) => match def_kind {
                DefKind::TyParam(idx) => tcx.mk_ty_param(def_id, idx, tcx.defs().ident(def_id)),
                DefKind::Struct | DefKind::Enum => {
                    let adt_ty = tcx.type_of(def_id);
                    let (adt, _) = adt_ty.expect_adt();
                    let generic_params = tcx.generics_of(def_id).params;
                    let expected_argc = generic_params.len();
                    // there should only be generic args in the very last position the preceding
                    // segments should be a module path, and the segments afterwards are type
                    // relative
                    let (last, segs) = path.segments.split_last().unwrap();
                    self.ensure_no_generic_args(segs);
                    let generic_args = last.args;
                    // replace each generic parameter with either the specified type argument
                    // or id generics
                    let substs = match generic_args {
                        Some(args) =>
                            if args.args.len() != expected_argc {
                                let err =
                                    TypeError::GenericArgCount(expected_argc, args.args.len());
                                tcx.sess
                                    .build_error(
                                        vec![
                                            (
                                                tcx.defs().generics(adt.def_id).span,
                                                "generic parameter declaration",
                                            ),
                                            (path.span, "generic arguments"),
                                        ],
                                        err,
                                    )
                                    .emit();
                                return tcx.mk_ty_err();
                            } else {
                                tcx.mk_substs(args.args.iter().map(|ty| self.ir_ty_to_ty(ty)))
                            },
                        None => Substs::id_for_def(tcx, def_id),
                    };
                    adt_ty.subst(tcx, substs)
                }
                DefKind::Ctor(..) => todo!(),
                DefKind::AssocFn | DefKind::Impl | DefKind::Fn => todo!(),
                DefKind::Extern => todo!(),
            },
            Res::SelfTy { impl_def } => tcx.type_of(impl_def),
            Res::SelfVal { impl_def: _ } => todo!(),
            Res::Err => tcx.mk_ty_err(),
            Res::Local(_) => panic!("unexpected resolution"),
        }
    }

    fn ensure_no_generic_args(&self, segments: &[ir::PathSegment<'tcx>]) {
        for segments in segments {
            if segments.args.is_some() {
                panic!()
            }
        }
    }

    fn lower_generics(&self, generics: &ir::Generics<'tcx>) -> &'tcx Generics<'tcx> {
        let params =
            generics.params.iter().map(|&ir::TyParam { id, index, ident, span, default }| {
                TyParam { id, span, ident, index, default: default.map(|ty| self.ir_ty_to_ty(ty)) }
            });
        self.tcx().alloc(Generics { params: self.tcx().alloc_iter(params) })
    }

    fn ir_fn_sig_to_ty(&self, sig: &ir::FnSig<'tcx>) -> Ty<'tcx> {
        self.tcx().mk_fn_ptr(self.lower_fn_sig(sig))
    }

    fn lower_fn_sig(&self, sig: &ir::FnSig<'tcx>) -> FnSig<'tcx> {
        let tcx = self.tcx();
        let params = tcx.mk_substs(sig.inputs.iter().map(|ty| self.ir_ty_to_ty(ty)));
        // `None` return type on fn sig implies unit type
        let ret = sig.output.map(|ty| self.ir_ty_to_ty(ty)).unwrap_or(tcx.types.unit);
        FnSig { params, ret }
    }
}

impl<'tcx> TyConv<'tcx> for TyCtx<'tcx> {
    fn tcx(&self) -> TyCtx<'tcx> {
        *self
    }

    fn infer_ty(&self, _span: Span) -> Ty<'tcx> {
        panic!("tyctx can't lower types with inference variables")
    }
}
