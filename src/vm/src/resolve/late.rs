use super::{Resolver, Scope, Scopes};
use crate::ast::{self, *};
use crate::error::{ResolutionError, ResolutionResult};
use crate::ir::{PerNS, Res, NS};
use std::marker::PhantomData;

struct LateResolutionVisitor<'a, 'r, 'ast> {
    resolver: &'r mut Resolver,
    scopes: PerNS<Scopes<Res<NodeId>>>,
    pd: &'a PhantomData<&'ast ()>,
}

impl<'a, 'r, 'ast> LateResolutionVisitor<'a, 'r, 'ast> {
    pub fn new(resolver: &'r mut Resolver) -> Self {
        Self { resolver, pd: &PhantomData, scopes: Default::default() }
    }

    pub fn with_val_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.scopes[NS::Value].push(Scope::default());
        let ret = f(self);
        self.scopes[NS::Value].pop();
        ret
    }

    fn resolve_pattern(&mut self, pat: &'ast Pattern) {
        match &pat.kind {
            PatternKind::Ident(ident, _sub) => {
                let res = Res::Local(pat.id);
                self.scopes[NS::Value].define(*ident, res);
            }
            PatternKind::Wildcard | PatternKind::Tuple(_) | PatternKind::Paren(_) => {}
        }
    }

    fn resolve_path(&mut self, path: &'ast Path, ns: NS) -> ResolutionResult<()> {
        if path.segments.len() > 1 {
            unimplemented!()
        }
        let segment = &path.segments[0];

        let res = *self.scopes[ns]
            .lookup(&segment.ident)
            .ok_or_else(|| ResolutionError::unbound_variable(segment.ident.span))?;
        self.resolver.resolve_node(segment.id, res);
        Ok(())
    }
}

impl<'a, 'ast> ast::Visitor<'ast> for LateResolutionVisitor<'a, '_, 'ast> {
    fn visit_block(&mut self, block: &'ast Block) {
        self.with_val_scope(|this| ast::walk_block(this, block));
    }

    fn visit_let(&mut self, Let { pat, ty, init, .. }: &'ast Let) {
        self.resolve_pattern(pat);
        ast::walk_pat(self, pat);
        ty.as_ref().map(|ty| self.visit_ty(ty));
        init.as_ref().map(|expr| self.visit_expr(expr));
    }

    fn visit_item(&mut self, item: &'ast Item) {
        self.resolver.create_def(item.id);
        ast::walk_item(self, item);
    }

    fn visit_path(&mut self, path: &'ast Path) {
        self.resolve_path(path, NS::Value).unwrap();
    }
}

impl Resolver {
    crate fn late_resolve_prog(&mut self, prog: &Prog) {
        let mut visitor = LateResolutionVisitor::new(self);
        visitor.visit_prog(prog);
    }
}
