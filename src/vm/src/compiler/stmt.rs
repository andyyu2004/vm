use super::Compiler;
use crate::tir;

impl<'tcx> Compiler<'tcx> {
    pub(super) fn compile_stmt(&mut self, stmt: &tir::Stmt) {
        match stmt.kind {
            tir::StmtKind::Let(l) => self.compile_let_stmt(l),
            tir::StmtKind::Expr(expr) => {
                self.compile_expr(expr);
                self.pop();
            }
        }
    }

    fn compile_let_stmt(&mut self, l: &tir::Let) {
        // if no initializer, just put a `unit` in the slot
        match l.init {
            Some(expr) => self.compile_expr(expr),
            None => self.unit(),
        };
        self.compile_let_pat(l.pat);
    }
}
