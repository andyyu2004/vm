use crate::ir::{self, DefId};
use crate::tir;
use crate::ty::{InferenceVarSubstFolder, Subst, Ty};
use crate::typeck::{inference::InferCtx, TyCtx, TypeckTables};
use indexed_vec::Idx;
use std::marker::PhantomData;

/// ir -> tir
crate struct IrLoweringCtx<'a, 'tcx> {
    tcx: TyCtx<'tcx>,
    infcx: &'a InferCtx<'a, 'tcx>,
    tables: &'a TypeckTables<'tcx>,
}

impl<'a, 'tcx> IrLoweringCtx<'a, 'tcx> {
    pub fn new(infcx: &'a InferCtx<'a, 'tcx>, tables: &'a TypeckTables<'tcx>) -> Self {
        Self { infcx, tcx: infcx.tcx, tables }
    }

    pub fn lower_item(&mut self, item: &ir::Item<'tcx>) -> &'tcx tir::Item<'tcx> {
        // this `tir` may still have unsubstituted inference variables in it
        item.to_tir_alloc(self)
    }

    fn node_type(&mut self, id: ir::Id) -> Ty<'tcx> {
        info!("irloweringctx: query typeof {:?}", id);
        self.tables.node_type(id)
    }

    fn lower_tuple_subpats(&mut self, pats: &[ir::Pattern<'tcx>]) -> &'tcx [tir::FieldPat<'tcx>] {
        let tcx = self.tcx;
        let pats = pats
            .iter()
            .enumerate()
            .map(|(i, pat)| tir::FieldPat { field: tir::Field::new(i), pat: pat.to_tir(self) });
        tcx.alloc_tir_iter(pats)
    }
}

/// trait for conversion to tir
crate trait Tir<'tcx> {
    type Output;
    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output;

    fn to_tir_alloc(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> &'tcx Self::Output {
        let tir = self.to_tir(ctx);
        ctx.tcx.alloc_tir(tir)
    }
}

impl<'tcx> Tir<'tcx> for ir::Item<'tcx> {
    type Output = tir::Item<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let &Self { span, id, ident, vis, ref kind } = self;
        let kind = match kind {
            ir::ItemKind::Fn(sig, generics, body) => {
                let (inputs, output) = ctx.tcx.item_ty(self.id.def_id).expect_fn();
                let sig = ctx.tcx.alloc_tir(tir::FnSig { inputs, output });
                tir::ItemKind::Fn(sig, generics.to_tir(ctx), body.to_tir(ctx))
            }
        };
        tir::Item { kind, span, id, ident, vis }
    }
}

impl<'tcx> Tir<'tcx> for ir::Param<'tcx> {
    type Output = tir::Param<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let &Self { id, span, ref pat } = self;
        tir::Param { id, span, pat: pat.to_tir_alloc(ctx) }
    }
}

impl<'tcx> Tir<'tcx> for ir::Pattern<'tcx> {
    type Output = tir::Pattern<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let &Self { id, span, ref kind } = self;
        let kind = match kind {
            ir::PatternKind::Wildcard => tir::PatternKind::Wildcard,
            ir::PatternKind::Binding(ident, sub) => {
                let subpat = sub.map(|pat| pat.to_tir_alloc(ctx));
                tir::PatternKind::Binding(*ident, subpat)
            }
            ir::PatternKind::Tuple(pats) => tir::PatternKind::Field(ctx.lower_tuple_subpats(pats)),
        };
        let ty = ctx.node_type(id);
        tir::Pattern { id, span, kind, ty }
    }
}

impl<'tcx> Tir<'tcx> for ir::Body<'tcx> {
    type Output = &'tcx tir::Body<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let tcx = ctx.tcx;
        let params = self.params.to_tir(ctx);
        let body = tir::Body { params, expr: self.expr.to_tir_alloc(ctx) };
        ctx.tcx.alloc_tir(body)
    }
}

impl<'tcx> Tir<'tcx> for ir::Generics<'tcx> {
    type Output = &'tcx tir::Generics<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        ctx.tcx.alloc_tir(tir::Generics { data: 0, pd: PhantomData })
    }
}

impl<'tcx> Tir<'tcx> for ir::Let<'tcx> {
    type Output = tir::Let<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let tcx = ctx.tcx;
        tir::Let {
            id: self.id,
            pat: self.pat.to_tir_alloc(ctx),
            init: self.init.map(|init| init.to_tir_alloc(ctx)),
        }
    }
}

impl<'tcx> Tir<'tcx> for ir::Stmt<'tcx> {
    type Output = tir::Stmt<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let &Self { id, span, ref kind } = self;
        let kind = match kind {
            ir::StmtKind::Let(l) => tir::StmtKind::Let(l.to_tir_alloc(ctx)),
            // we can map both semi and expr to expressions and their distinction is no longer
            // important after typechecking is done
            ir::StmtKind::Expr(expr) | ir::StmtKind::Semi(expr) =>
                tir::StmtKind::Expr(expr.to_tir_alloc(ctx)),
        };
        tir::Stmt { id, span, kind }
    }
}

impl<'tcx> Tir<'tcx> for ir::Block<'tcx> {
    type Output = tir::Block<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let tcx = ctx.tcx;
        let stmts = tcx.alloc_tir_iter(self.stmts.iter().map(|stmt| stmt.to_tir(ctx)));
        let expr = self.expr.map(|expr| expr.to_tir_alloc(ctx));
        tir::Block { id: self.id, stmts, expr }
    }
}

impl<'tcx, T> Tir<'tcx> for Option<T>
where
    T: Tir<'tcx>,
{
    type Output = Option<T::Output>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        self.as_ref().map(|t| t.to_tir(ctx))
    }
}

impl<'tcx, T> Tir<'tcx> for &'tcx [T]
where
    T: Tir<'tcx>,
{
    type Output = &'tcx [T::Output];

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let tcx = ctx.tcx;
        tcx.alloc_tir_iter(self.iter().map(|t| t.to_tir(ctx)))
    }

    fn to_tir_alloc(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> &'tcx Self::Output {
        panic!("use `to_tir` for slices")
    }
}

impl<'tcx> Tir<'tcx> for ir::Expr<'tcx> {
    type Output = tir::Expr<'tcx>;

    fn to_tir(&self, ctx: &mut IrLoweringCtx<'_, 'tcx>) -> Self::Output {
        let kind = match &self.kind {
            ir::ExprKind::Bin(op, l, r) =>
                tir::ExprKind::Bin(*op, l.to_tir_alloc(ctx), r.to_tir_alloc(ctx)),
            ir::ExprKind::Unary(op, expr) => tir::ExprKind::Unary(*op, expr.to_tir_alloc(ctx)),
            ir::ExprKind::Block(block) => tir::ExprKind::Block(block.to_tir_alloc(ctx)),
            ir::ExprKind::Path(path) => match path.res {
                ir::Res::Local(id) => tir::ExprKind::VarRef(id),
                ir::Res::PrimTy(_) => unreachable!(),
            },
            ir::ExprKind::Tuple(xs) => tir::ExprKind::Tuple(xs.to_tir(ctx)),
            ir::ExprKind::Lambda(_, body) => tir::ExprKind::Lambda(body.to_tir_alloc(ctx)),
            ir::ExprKind::Call(f, args) =>
                tir::ExprKind::Call(f.to_tir_alloc(ctx), args.to_tir(ctx)),
            &ir::ExprKind::Lit(lit) => tir::ExprKind::Lit(lit),
        };
        let ty = ctx.node_type(self.id);
        tir::Expr { span: self.span, kind, ty }
    }
}