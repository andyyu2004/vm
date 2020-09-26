use super::AstLoweringCtx;
use crate::ast::*;
use crate::ir::{self, DefKind, Res, VariantIdx};
use crate::lexer::Symbol;
use indexed_vec::Idx;
use std::marker::PhantomData;

impl<'a, 'ir> AstLoweringCtx<'a, 'ir> {
    pub fn lower_item(&mut self, item: &Item) {
        self.with_owner(item.id, |lctx| {
            let &Item { span, id, vis, ref kind, ident } = item;
            let generic_arg_count = item.generics().params.len();
            let id = lctx.lower_node_id(id);
            lctx.resolver.record_generic_arg_count(id.def, generic_arg_count);

            let kind = match &kind {
                ItemKind::Fn(sig, generics, expr) => {
                    // assume the function has a body for now
                    let lowered_sig = lctx.lower_fn_sig(sig);
                    let body = lctx.lower_body(sig, expr.as_ref().unwrap());
                    let lowered_sig = lctx.lower_fn_sig(sig);
                    let generics = lctx.lower_generics(generics);
                    ir::ItemKind::Fn(lowered_sig, generics, body)
                }
                ItemKind::Enum(generics, variants) => {
                    let generics = lctx.lower_generics(generics);
                    let variants = lctx.arena.ir.alloc_from_iter(
                        variants.iter().enumerate().map(|(i, v)| lctx.lower_variant(item, i, v)),
                    );
                    ir::ItemKind::Enum(generics, variants)
                }
                ItemKind::Struct(generics, variant_kind) => {
                    let generics = lctx.lower_generics(generics);
                    let kind = lctx.lower_variant_kind(variant_kind);
                    ir::ItemKind::Struct(generics, kind)
                }
                ItemKind::Impl { generics, trait_path, self_ty, items } =>
                    lctx.lower_impl(generics, trait_path.as_ref(), self_ty, items),
            };
            let item = ir::Item { span, id, vis, ident, kind };
            lctx.items.insert(item.id, item);
        });
    }

    fn lower_impl(
        &mut self,
        generics: &Generics,
        path: Option<&Path>,
        self_ty: &Ty,
        impl_items: &[Box<AssocItem>],
    ) -> ir::ItemKind<'ir> {
        let arena = &self.arena.ir;
        let generics = self.lower_generics(generics);
        let trait_path = path.map(|path| self.lower_path(path));
        let self_ty = self.lower_ty(self_ty);
        let impl_items = impl_items.iter().map(|item| self.lower_impl_item_ref(item));
        let impl_item_refs = arena.alloc_from_iter(impl_items);
        ir::ItemKind::Impl { generics, trait_path, self_ty, impl_item_refs }
    }

    fn lower_impl_item_ref(&mut self, impl_item: &AssocItem) -> ir::ImplItemRef {
        self.with_owner(impl_item.id, |lctx| {
            let item = lctx.lower_impl_item(impl_item);
            let id = ir::ImplItemId(item.id);
            lctx.impl_items.insert(id, item);
            ir::ImplItemRef { id }
        })
    }

    fn lower_impl_item(&mut self, impl_item: &AssocItem) -> ir::ImplItem<'ir> {
        let &AssocItem { span, id, vis, ident, ref kind } = impl_item;
        let (generics, kind) = match kind {
            AssocItemKind::Fn(sig, generics, body) => {
                let generics = self.lower_generics(generics);
                let body = body.as_ref().map(|body| self.lower_body(sig, body));
                let sig = self.lower_fn_sig(sig);
                (generics, ir::ImplItemKind::Fn(sig, body))
            }
        };
        ir::ImplItem { id: self.lower_node_id(id), ident, span, vis, generics, kind }
    }

    fn lower_variant(&mut self, item: &Item, idx: usize, variant: &Variant) -> ir::Variant<'ir> {
        let adt_def = self.curr_owner();
        self.with_owner(variant.id, |lctx| ir::Variant {
            adt_def,
            id: lctx.lower_node_id(variant.id),
            idx: VariantIdx::new(idx),
            ident: variant.ident,
            span: variant.span,
            kind: lctx.lower_variant_kind(&variant.kind),
        })
    }

    fn lower_field_decl(&mut self, (i, field): (usize, &FieldDecl)) -> ir::FieldDecl<'ir> {
        let &FieldDecl { span, ident, vis, id, ref ty } = field;
        let ident = ident.unwrap_or_else(|| Ident { span: field.span, symbol: Symbol::intern(i) });
        ir::FieldDecl { span, ident, vis, id: self.lower_node_id(id), ty: self.lower_ty(ty) }
    }

    fn lower_variant_kind(&mut self, variant_kind: &VariantKind) -> ir::VariantKind<'ir> {
        match variant_kind {
            VariantKind::Tuple(fields) => ir::VariantKind::Tuple(
                self.arena
                    .ir
                    .alloc_from_iter(fields.iter().enumerate().map(|f| self.lower_field_decl(f))),
            ),
            VariantKind::Struct(fields) => ir::VariantKind::Struct(
                self.arena
                    .ir
                    .alloc_from_iter(fields.iter().enumerate().map(|f| self.lower_field_decl(f))),
            ),
            VariantKind::Unit => ir::VariantKind::Unit,
        }
    }

    pub(super) fn lower_fn_sig(&mut self, sig: &FnSig) -> &'ir ir::FnSig<'ir> {
        let inputs =
            self.arena.ir.alloc_from_iter(sig.params.iter().map(|p| self.lower_ty_inner(&p.ty)));
        let output = sig.ret_ty.as_ref().map(|ty| self.lower_ty(ty));
        self.arena.alloc(ir::FnSig { inputs, output })
    }
}
