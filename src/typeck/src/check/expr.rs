use super::FnCtx;
use crate::{Autoderef, TyConv};
use ast::{BinOp, Ident, Lit, Mutability, UnaryOp};
use itertools::Itertools;
use lcore::ty::*;
use rustc_hash::FxHashMap;

impl<'a, 'tcx> FnCtx<'a, 'tcx> {
    pub fn check_expr(&mut self, expr: &ir::Expr<'tcx>) -> Ty<'tcx> {
        let ty = match &expr.kind {
            ir::ExprKind::Box(expr) => self.check_box_expr(expr),
            ir::ExprKind::Err => self.set_ty_err(),
            ir::ExprKind::Lit(lit) => self.check_lit(lit),
            ir::ExprKind::Bin(op, l, r) => self.check_binop(*op, l, r),
            ir::ExprKind::Unary(op, operand) => self.check_unary_expr(expr, *op, operand),
            ir::ExprKind::Block(block) => self.check_block(block),
            ir::ExprKind::Path(qpath) => self.check_qpath(expr, qpath),
            ir::ExprKind::Tuple(xs) => self.check_expr_tuple(xs),
            ir::ExprKind::Closure(sig, body) => self.check_closure_expr(expr, sig, body),
            ir::ExprKind::Call(f, args) => self.check_call_expr(expr, f, args),
            ir::ExprKind::Match(expr, arms, src) => self.check_match_expr(expr, arms, src),
            ir::ExprKind::Struct(qpath, fields) => self.check_struct_expr(expr, qpath, fields),
            ir::ExprKind::Assign(l, r) => self.check_assign_expr(expr, l, r),
            ir::ExprKind::Ret(ret) => self.check_ret_expr(expr, ret.as_deref()),
            ir::ExprKind::Field(base, ident) => self.check_field_expr(expr, base, *ident),
        };
        self.record_ty(expr.id, ty)
    }

    fn check_unary_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        op: UnaryOp,
        operand: &ir::Expr<'tcx>,
    ) -> Ty<'tcx> {
        let operand_ty = self.check_expr(operand);
        match op {
            UnaryOp::Neg => todo!(),
            UnaryOp::Not => {
                self.equate(expr.span, self.types.bool, operand_ty);
                self.types.bool
            }
            // TODO how to handle mutability?
            UnaryOp::Deref => self.deref_ty(expr.span, operand_ty),
            UnaryOp::Ref => {
                if !self.in_unsafe_ctx() {
                    self.emit_ty_err(expr.span, TypeError::RequireUnsafeCtx);
                }
                self.mk_ptr_ty(operand_ty)
            }
        }
    }

    fn check_box_expr(&mut self, expr: &ir::Expr<'tcx>) -> Ty<'tcx> {
        let ty = self.check_expr(expr);
        self.mk_box_ty(ty)
    }

    fn check_field_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        base: &ir::Expr<'tcx>,
        ident: Ident,
    ) -> Ty<'tcx> {
        let (autoderef, ty) = self.check_field_expr_inner(expr, base, ident);
        let adjustments = autoderef.get_adjustments();
        self.record_adjustments(base.id, adjustments);
        ty
    }

    fn check_field_expr_inner(
        &mut self,
        expr: &ir::Expr<'tcx>,
        base: &ir::Expr<'tcx>,
        ident: Ident,
    ) -> (Autoderef<'_, 'tcx>, Ty<'tcx>) {
        let base_ty = self.check_expr(base);
        let mut autoderef = self.autoderef(expr.span, base_ty);
        for ty in &mut autoderef {
            match ty.kind {
                Adt(adt, substs) if adt.kind != AdtKind::Enum => {
                    let variant = adt.single_variant();
                    let ty = if let Some((idx, field)) =
                        variant.fields.iter().find_position(|f| f.ident == ident)
                    {
                        // note the id belongs is the id of the entire field expression
                        // not just the identifier or base
                        self.record_field_index(expr.id, idx);
                        field.ty(self.tcx, substs)
                    } else {
                        self.emit_ty_err(expr.span, TypeError::UnknownField(base_ty, ident))
                    };
                    return (autoderef, ty);
                }
                Tuple(tys) => {
                    // `tuple.i` literally means the i'th element of tuple
                    // so we can weirdly parse the identifier as the actual index
                    let idx = ident.as_str().parse::<usize>().unwrap();
                    self.record_field_index(expr.id, idx);
                    let ty = match tys.get(idx) {
                        Some(ty) => ty,
                        None =>
                            self.emit_ty_err(expr.span, TypeError::TupleOutOfBounds(idx, tys.len())),
                    };
                    return (autoderef, ty);
                }
                _ => continue,
            }
        }
        (autoderef, self.emit_ty_err(expr.span, TypeError::BadFieldAccess(base_ty)))
    }

    /// return expressions have the type of the expression that follows the return
    fn check_ret_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        ret_expr: Option<&ir::Expr<'tcx>>,
    ) -> Ty<'tcx> {
        let ty = ret_expr.map(|expr| self.check_expr(expr)).unwrap_or(self.tcx.types.unit);
        self.equate(expr.span, self.sig.ret, ty);
        self.tcx.types.never
    }

    /// checks the expressions is a lvalue and mutable, hence assignable
    fn check_lvalue(&mut self, l: &ir::Expr) {
        if !l.is_lvalue() {
            self.emit_ty_err(
                l.span,
                TypeError::Msg(format!("expected lvalue as target of assignment")),
            );
        }
    }

    fn check_assign_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        l: &ir::Expr<'tcx>,
        r: &ir::Expr<'tcx>,
    ) -> Ty<'tcx> {
        self.check_lvalue(l);
        let lty = self.check_expr(l);
        let rty = self.check_expr(r);
        self.equate(expr.span, lty, rty);
        rty
    }

    fn check_struct_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        qpath: &ir::QPath<'tcx>,
        fields: &[ir::Field<'tcx>],
    ) -> Ty<'tcx> {
        let (variant_ty, ty) = match self.check_struct_path(expr, qpath) {
            Some(tys) => tys,
            None => return self.tcx.mk_ty_err(),
        };
        let (_adt_ty, substs) = ty.expect_adt();
        self.check_struct_expr_fields(expr, substs, variant_ty, fields);
        ty
    }

    fn check_struct_expr_fields(
        &mut self,
        expr: &ir::Expr<'tcx>,
        substs: SubstsRef<'tcx>,
        variant: &VariantTy<'tcx>,
        fields: &[ir::Field<'tcx>],
    ) -> bool {
        // note we preserve the field declaration order of the struct
        let mut remaining_fields = variant
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| (f.ident, (i, f)))
            .collect::<FxHashMap<Ident, (usize, &FieldTy)>>();
        let mut seen_fields = FxHashMap::default();
        let mut has_error = false;
        for field in fields {
            // handle unknown field or setting field twice
            let ty = self.check_expr(field.expr);
            match remaining_fields.remove(&field.ident) {
                Some((idx, f)) => {
                    seen_fields.insert(field.ident, field.span);
                    self.record_field_index(field.id, idx);
                    self.equate(field.span, f.ty(self.tcx, substs), ty);
                }
                None => {
                    has_error = true;
                    if let Some(&span) = seen_fields.get(&field.ident) {
                        // write the index even on error to avoid missing
                        // entries in table later (may be unnecessary)
                        // self.record_field_index(field.id, idx);
                        self.emit_ty_err(
                            vec![field.span, span],
                            TypeError::Msg(format!("field `{}` set more than once", field.ident)),
                        );
                    } else {
                        self.emit_ty_err(
                            field.span,
                            TypeError::Msg(format!("unknown field `{}`", field.ident)),
                        );
                    }
                }
            }
        }

        if !remaining_fields.is_empty() {
            has_error = true;
            self.emit_ty_err(expr.span, TypeError::Msg(format!("incomplete fields")));
        }

        has_error
    }

    fn check_match_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        arms: &[ir::Arm<'tcx>],
        src: &ir::MatchSource,
    ) -> Ty<'tcx> {
        let expr_ty = self.check_expr(expr);
        match src {
            ir::MatchSource::If => self.equate(expr.span, self.tcx.types.bool, expr_ty),
            ir::MatchSource::Match => {}
        };

        // check that each arm pattern is the same type as the scrutinee
        for arm in arms {
            self.check_pat(arm.pat, expr_ty);
        }

        // special case when match has no arms
        if arms.is_empty() {
            return self.tcx.types.unit;
        }

        // otherwise, consider the last arm's body to be the expected type
        let n = arms.len() - 1;
        let expected_ty = self.check_expr(arms[n].body);
        arms[..n].iter().for_each(|arm| {
            let arm_ty = self.check_expr(arm.body);
            arm.guard.iter().for_each(|guard| {
                let guard_ty = self.check_expr(guard);
                self.equate(guard.span, self.tcx.types.bool, guard_ty);
            });
            self.equate(arm.span, expected_ty, arm_ty);
        });

        expected_ty
    }

    fn check_call_expr(
        &mut self,
        expr: &ir::Expr<'tcx>,
        f: &ir::Expr<'tcx>,
        args: &[ir::Expr<'tcx>],
    ) -> Ty<'tcx> {
        let ret = self.new_infer_var(expr.span);
        let f_ty = self.check_expr(f);
        let params = self.check_expr_list(args);
        let ty = self.tcx.mk_fn_ptr(FnSig { params, ret });
        self.coerce(expr, f_ty, ty);
        ret
    }

    fn check_closure_expr(
        &mut self,
        closure: &ir::Expr<'tcx>,
        sig: &ir::FnSig<'tcx>,
        body: &ir::Body<'tcx>,
    ) -> Ty<'tcx> {
        // the resolver resolved the closure name to the id of the entire closure expr
        // so we define an immutable local variable for it with the closure's type
        let sig = self.lower_fn_sig(sig);
        let ty = self.mk_fn_ptr(sig);
        self.record_upvars(closure, body);
        self.def_local(closure.id, Mutability::Imm, ty);
        let _fcx = self.check_fn(sig, body);
        ty
    }

    /// inputs are the types from the type signature (or inference variables) adds the parameters
    /// to locals and typechecks the expr of the body
    pub fn check_body(&mut self, body: &ir::Body<'tcx>) {
        for (param, ty) in body.params.iter().zip(self.sig.params) {
            self.check_pat(param.pat, ty);
        }
        let body_ty = self.check_expr(body.expr);
        self.equate(body.expr.span, self.sig.ret, body_ty);
        // explicitly overwrite the type of body with the return type of the function in the case
        // where it is inferred to be `!` this is a special case due to return statements in the
        // top level block expr without this overwrite, if the final statement is diverging (i.e.
        // return) then the body function will be recorded to have type `!` which is not correct
        self.record_ty(body.id(), self.sig.ret);
    }

    fn check_expr_list(&mut self, xs: &[ir::Expr<'tcx>]) -> SubstsRef<'tcx> {
        self.tcx.mk_substs(xs.iter().map(|expr| self.check_expr(expr)))
    }

    fn check_expr_tuple(&mut self, xs: &[ir::Expr<'tcx>]) -> Ty<'tcx> {
        self.tcx.mk_tup_iter(xs.iter().map(|expr| self.check_expr(expr)))
    }

    fn check_block(&mut self, block: &ir::Block<'tcx>) -> Ty<'tcx> {
        if block.is_unsafe {
            self.with_unsafe_ctx(|fcx| fcx.check_block_inner(block))
        } else {
            self.check_block_inner(block)
        }
    }

    fn check_block_inner(&mut self, block: &ir::Block<'tcx>) -> Ty<'tcx> {
        block.stmts.iter().for_each(|stmt| self.check_stmt(stmt));
        match &block.expr {
            Some(expr) => self.check_expr(expr),
            None => self.tcx.types.unit,
        }
    }

    fn check_binop(&mut self, op: ast::BinOp, l: &ir::Expr<'tcx>, r: &ir::Expr<'tcx>) -> Ty<'tcx> {
        let tl = self.check_expr(l);
        let tr = self.check_expr(r);
        match op {
            BinOp::Eq => todo!(),
            // TODO deal with floats
            BinOp::Mul | BinOp::Div | BinOp::Add | BinOp::Sub => {
                self.equate(l.span, self.tcx.types.int, tl);
                self.equate(r.span, tl, tr);
                tl
            }
            BinOp::Lt | BinOp::Gt => {
                // TODO deal with floats
                self.equate(l.span, self.tcx.types.int, tl);
                self.equate(r.span, tl, tr);
                self.tcx.types.bool
            }
            BinOp::Neq => todo!(),
            BinOp::And | BinOp::Or => {
                self.equate(l.span, self.tcx.types.int, tl);
                self.equate(r.span, tl, tr);
                tl
            }
        }
    }

    fn check_lit(&self, lit: &ast::Lit) -> Ty<'tcx> {
        match lit {
            Lit::Bool(..) => self.tcx.types.bool,
            Lit::Float(..) => self.tcx.types.float,
            Lit::Int(..) => self.tcx.types.int,
        }
    }
}
