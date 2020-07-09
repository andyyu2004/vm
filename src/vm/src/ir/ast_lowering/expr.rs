use super::LoweringCtx;
use crate::ast::*;
use crate::ir;
use itertools::Itertools;

impl<'ir> LoweringCtx<'ir> {
    fn lower_exprs(&mut self, exprs: &[Box<Expr>]) -> &'ir [ir::Expr<'ir>] {
        self.arena.alloc_from_iter(exprs.iter().map(|x| self.lower_expr_inner(x)))
    }

    crate fn lower_expr(&mut self, expr: &Expr) -> &'ir ir::Expr<'ir> {
        self.arena.alloc(self.lower_expr_inner(expr))
    }

    fn lower_expr_inner(&mut self, expr: &Expr) -> ir::Expr<'ir> {
        let kind = match &expr.kind {
            ExprKind::Lit(lit) => ir::ExprKind::Lit(*lit),
            ExprKind::Bin(op, l, r) => {
                ir::ExprKind::Bin(*op, self.lower_expr(&l), self.lower_expr(&r))
            }
            ExprKind::Unary(op, expr) => ir::ExprKind::Unary(*op, self.lower_expr(&expr)),
            ExprKind::Paren(expr) => return self.lower_expr_inner(&expr),
            ExprKind::Block(block) => ir::ExprKind::Block(self.lower_block(block)),
            ExprKind::Path(path) => todo!(),
        };
        ir::Expr::new(expr.span, kind)
    }

    fn lower_block(&mut self, block: &Block) -> &'ir ir::Block<'ir> {
        let mut expr = None;
        let mut stmts = block.stmts.iter().map(|stmt| self.lower_stmt_inner(stmt)).collect_vec();
        if let Some(&ir::StmtKind::Expr(e)) = stmts.last().map(|s| &s.kind) {
            expr = Some(e);
            stmts.pop();
        }
        let ir_block = ir::Block {
            stmts: self.arena.alloc_from_iter(stmts),
            id: self.lower_node_id(block.id),
            expr,
            span: block.span,
        };
        self.arena.alloc(ir_block)
    }
}
