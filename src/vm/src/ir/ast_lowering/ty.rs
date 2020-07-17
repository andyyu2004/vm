use super::AstLoweringCtx;
use crate::ast::*;
use crate::ir;
use itertools::Itertools;

impl<'ir> AstLoweringCtx<'_, 'ir> {
    fn lower_tys(&mut self, tys: &[Box<Ty>]) -> &'ir [ir::Ty<'ir>] {
        self.arena.alloc_from_iter(tys.iter().map(|x| self.lower_ty_inner(x)))
    }

    crate fn lower_ty(&mut self, ty: &Ty) -> &'ir ir::Ty<'ir> {
        self.arena.alloc(self.lower_ty_inner(ty))
    }

    pub(super) fn lower_ty_inner(&mut self, ty: &Ty) -> ir::Ty<'ir> {
        let &Ty { span, id, ref kind } = ty;
        let kind = match kind {
            TyKind::Array(ty) => ir::TyKind::Array(self.lower_ty(ty)),
            TyKind::Tuple(tys) => ir::TyKind::Tuple(self.lower_tys(tys)),
            TyKind::Paren(ty) => return self.lower_ty_inner(ty),
            TyKind::Path(p) => todo!(),
            TyKind::Fn(_, _) => todo!(),
            TyKind::Infer => ir::TyKind::Infer,
        };

        ir::Ty { span, id: self.lower_node_id(id), kind }
    }
}