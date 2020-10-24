use crate::{FnCtx, TcxTypeofExt};
use ir::{CtorKind, DefId, DefKind, QPath, Res};
use lcore::ty::*;
use span::Span;

impl<'a, 'tcx> FnCtx<'a, 'tcx> {
    crate fn resolve_qpath(&mut self, qpath: &QPath) -> Res {
        match qpath {
            QPath::Resolved(path) => path.res,
            QPath::TypeRelative(_, _) => todo!(),
        }
    }

    crate fn check_struct_path(
        &mut self,
        qpath: &ir::QPath,
    ) -> Option<(&'tcx VariantTy<'tcx>, Ty<'tcx>)> {
        let ty = self.check_expr_qpath(qpath);
        let res = self.resolve_qpath(qpath);
        // we don't directly return `substs` as it can be accessed through `ty`
        let variant = match res {
            ir::Res::Def(_, DefKind::Struct) => match ty.kind {
                Adt(adt, _substs) => Some((adt.single_variant(), ty)),
                _ => unreachable!(),
            },
            ir::Res::SelfVal { impl_def } => {
                let self_ty = self.type_of(impl_def);
                assert_eq!(self_ty, ty);
                match self_ty.kind {
                    Adt(adt, _substs) => Some((adt.single_variant(), self_ty)),
                    _ => unreachable!(),
                }
            }
            ir::Res::Def(def_id, DefKind::Ctor(CtorKind::Struct)) => match ty.kind {
                Adt(adt, _substs) => Some((adt.variant_with_ctor(def_id), ty)),
                _ => unreachable!(),
            },
            ir::Res::Local(_) => None,
            ir::Res::PrimTy(..) | ir::Res::SelfTy { .. } => unreachable!(),
            _ => unimplemented!("{} (res: {:?})", qpath, res),
        };

        variant.or_else(|| {
            self.emit_ty_err(
                qpath.span(),
                TypeError::Msg(format!("expected struct path, found {:?}", qpath)),
            );
            None
        })
    }

    crate fn check_expr_qpath(&mut self, qpath: &ir::QPath) -> Ty<'tcx> {
        match qpath {
            ir::QPath::Resolved(path) => self.check_expr_path(path),
            ir::QPath::TypeRelative(_, _) => todo!(),
        }
    }

    crate fn check_expr_path(&mut self, path: &ir::Path) -> Ty<'tcx> {
        match path.res {
            ir::Res::Local(id) => self.local_ty(id).ty,
            ir::Res::Def(def_id, def_kind) => self.check_expr_path_def(path.span, def_id, def_kind),
            ir::Res::SelfVal { impl_def } => self.type_of(impl_def),
            ir::Res::PrimTy(_) => panic!("found type resolution in value namespace"),
            ir::Res::Err => self.set_ty_err(),
            ir::Res::SelfTy { .. } => todo!(),
        }
    }

    fn check_expr_path_def(&mut self, span: Span, def_id: DefId, def_kind: DefKind) -> Ty<'tcx> {
        match def_kind {
            // instantiate ty params
            DefKind::Fn
            | DefKind::AssocFn
            | DefKind::Enum
            | DefKind::Struct
            | DefKind::Ctor(..) => self.instantiate(span, self.collected_ty(def_id)),
            DefKind::Extern | DefKind::TyParam(_) | DefKind::Impl => unreachable!(),
        }
    }
}
