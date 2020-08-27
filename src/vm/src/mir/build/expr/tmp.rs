use crate::ast;
use crate::mir::build::*;
use crate::mir::*;
use crate::set;
use crate::span::Span;
use crate::ty::Ty;

impl<'a, 'tcx> Builder<'a, 'tcx> {
    pub fn as_tmp(&mut self, mut block: BlockId, expr: &tir::Expr<'tcx>) -> BlockAnd<Lvalue<'tcx>> {
        let info = self.span_info(expr.span);
        let lvalue = self.alloc_tmp(info, expr.ty).into();
        // include a pattern if some expressiosn requires special treatment
        match expr.kind {
            _ => {
                set!(block = self.write_expr(block, lvalue, expr));
                block.and(lvalue)
            }
        }
    }
}
