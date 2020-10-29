#![feature(array_value_iter)]
#![feature(crate_visibility_modifier)]
#![feature(decl_macro)]

#[macro_use]
extern crate log;

mod construct;
mod expr;
mod item;
mod pat;
mod path;
mod stmt;
mod ty;

#[cfg(test)]
mod tests;

use ast::*;
use index::Idx;
use ir::{DefId, LocalId, Res};
use resolve::Resolver;
use rustc_hash::FxHashMap;
use session::Session;
use std::cell::Cell;
use std::collections::BTreeMap;

ir::arena_types!(arena::declare_arena, [], 'tcx);

pub struct AstLoweringCtx<'a, 'ir> {
    arena: &'ir Arena<'ir>,
    sess: &'ir Session,
    resolver: &'a mut Resolver<'ir>,
    node_id_to_id: FxHashMap<NodeId, ir::Id>,
    item_stack: Vec<(DefId, usize)>,
    items: BTreeMap<DefId, ir::Item<'ir>>,
    impl_items: BTreeMap<ir::ImplItemId, ir::ImplItem<'ir>>,
    entry_id: Option<DefId>,
    /// this counter counts backwards as to be sure not to not
    /// overlap with the ids that the parser assigned
    /// this counts nodes that are constructed by desugaring
    new_node_id_counter: Cell<usize>,
}

impl<'a, 'ir> AstLoweringCtx<'a, 'ir> {
    pub fn new(
        arena: &'ir Arena<'ir>,
        sess: &'ir Session,
        resolver: &'a mut Resolver<'ir>,
    ) -> Self {
        Self {
            arena,
            sess,
            resolver,
            entry_id: None,
            item_stack: Default::default(),
            items: Default::default(),
            impl_items: Default::default(),
            node_id_to_id: Default::default(),
            new_node_id_counter: Cell::new(0xffff_ff00),
        }
    }

    fn alloc<T>(&self, t: T) -> &'ir T
    where
        T: ArenaAllocatable<'ir>,
    {
        self.arena.alloc(t)
    }

    fn alloc_from_iter<I>(&self, iter: I) -> &'ir [I::Item]
    where
        I: IntoIterator,
        I::Item: ArenaAllocatable<'ir>,
    {
        self.arena.alloc_from_iter(iter)
    }

    pub fn lower_prog(mut self, prog: &Prog) -> &'ir ir::Ir<'ir> {
        prog.items.iter().for_each(|item| self.lower_item(item));
        self.arena.alloc(ir::Ir {
            entry_id: self.entry_id,
            items: self.items,
            impl_items: self.impl_items,
        })
    }

    fn lower_generics(&mut self, generics: &Generics) -> &'ir ir::Generics<'ir> {
        let &Generics { span, ref params } = generics;
        let params = self.arena.alloc_from_iter(params.iter().map(|p| self.lower_ty_param(p)));
        params.iter().for_each(|ty_param| self.resolver.def_node(ty_param.id.def, ty_param.into()));
        self.arena.alloc(ir::Generics { span, params })
    }

    fn lower_ty_param(&mut self, param: &TyParam) -> ir::TyParam<'ir> {
        // `TyParam`s have their own `DefId`
        self.with_owner(param.id, |lctx| {
            let &TyParam { span, id, ident, ref default } = param;
            ir::TyParam {
                span,
                id: lctx.lower_node_id(id),
                index: lctx.resolver.idx_of_ty_param(id),
                ident,
                default: default.as_ref().map(|ty| lctx.lower_ty(ty)),
            }
        })
    }

    fn with_owner<T>(&mut self, owner: NodeId, f: impl FnOnce(&mut Self) -> T) -> T {
        let def_id = self.resolver.def_id(owner);
        self.item_stack.push((def_id, 0));
        let ret = f(self);
        let (popped_def_id, _popped_counter) = self.item_stack.pop().unwrap();
        debug_assert_eq!(popped_def_id, def_id);
        ret
    }

    fn curr_owner(&self) -> DefId {
        self.item_stack.last().unwrap().0
    }

    fn mk_node_id(&self) -> NodeId {
        let c = self.new_node_id_counter.get();
        self.new_node_id_counter.set(c - 1);
        NodeId::new(c)
    }

    fn new_id(&mut self) -> ir::Id {
        let node_id = self.mk_node_id();
        self.lower_node_id(node_id)
    }

    fn lower_node_id(&mut self, node_id: NodeId) -> ir::Id {
        self.lower_node_id_generic(node_id, |this| {
            let &mut (def, ref mut counter) = this.item_stack.last_mut().unwrap();
            let local_id = *counter;
            *counter += 1;
            ir::Id { def, local: LocalId::new(local_id) }
        })
    }

    fn lower_body(&mut self, sig: &FnSig, expr: &Expr) -> &'ir ir::Body<'ir> {
        let params = self.lower_params(&sig.params);
        let expr = self.lower_expr(expr);
        self.alloc(ir::Body { params, expr })
    }

    fn lower_params(&mut self, params: &[Param]) -> &'ir [ir::Param<'ir>] {
        self.arena.alloc_from_iter(params.iter().map(|p| self.lower_param(p)))
    }

    fn lower_param(&mut self, param: &Param) -> ir::Param<'ir> {
        let span = param.span;
        let id = self.lower_node_id(param.id);
        let pattern = self.lower_pattern(&param.pattern);
        ir::Param { span, id, pat: pattern }
    }

    fn lower_res(&mut self, res: Res<NodeId>) -> Res {
        res.map_id(|id| self.lower_node_id(id))
    }

    fn lower_node_id_generic(
        &mut self,
        node_id: NodeId,
        mk_id: impl FnOnce(&mut Self) -> ir::Id,
    ) -> ir::Id {
        match self.node_id_to_id.get(&node_id) {
            Some(&existing) => existing,
            None => {
                let new_id = mk_id(self);
                self.node_id_to_id.insert(node_id, new_id);
                new_id
            }
        }
    }
}