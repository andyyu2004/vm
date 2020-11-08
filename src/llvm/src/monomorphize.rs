use crate::CodegenCtx;
use ir::{DefId, FnVisitor, Visitor};
use lcore::mir::Operand;
use lcore::ty::{Instance, InstanceKind, Subst, TyCtx, TypeFoldable};
use mir::MirVisitor;
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::ops::Deref;

// pub fn provide(queries: &mut Queries) {
//     *queries = Queries { monomorphization_instances }
// }

pub trait Monomorphize<'tcx> {
    fn monomorphize<T>(&self, t: T) -> T
    where
        T: TypeFoldable<'tcx>;
}

impl<'tcx> CodegenCtx<'tcx> {
    /// collects all references to generic items along with substitutions representing
    /// each unique instantiation of the generic parameters
    /// we refer to these non-generic items as "roots"
    pub fn collect_monomorphization_instances(&self) -> FxHashSet<Instance<'tcx>> {
        let roots = RootCollector::new(self.tcx).collect_roots();
        MonomorphizationCollector::new(self, &roots).collect_instances()
    }
}

struct MonomorphizationCollector<'a, 'tcx> {
    cctx: &'a CodegenCtx<'tcx>,
    roots: &'a Vec<DefId>,
    mono_instances: RefCell<FxHashSet<Instance<'tcx>>>,
}

impl<'a, 'tcx> MonomorphizationCollector<'a, 'tcx> {
    fn new(cctx: &'a CodegenCtx<'tcx>, roots: &'a Vec<DefId>) -> Self {
        Self { cctx, roots, mono_instances: Default::default() }
    }

    fn collect_instances(self) -> FxHashSet<Instance<'tcx>> {
        for &root in self.roots {
            let instance = Instance::mono_item(root);
            self.collect_instance(instance);
        }
        self.mono_instances.into_inner()
    }

    fn collect_instance(&self, instance: Instance<'tcx>) {
        self.mono_instances.borrow_mut().insert(instance);
        match instance.kind {
            InstanceKind::Item =>
                if let Ok(mir) = self.tcx.mir_of(instance.def_id) {
                    InstanceCollector { collector: self, instance }.visit_mir(mir)
                },
            // no need to recurse on intrinsics as they do not have associated mir
            InstanceKind::Intrinsic => {}
        }
    }
}

/// collects all the non-generic `roots`
struct RootCollector<'tcx> {
    tcx: TyCtx<'tcx>,
    roots: Vec<DefId>,
}

impl<'tcx> RootCollector<'tcx> {
    pub fn collect_roots(mut self) -> Vec<DefId> {
        self.visit_ir(self.tcx.ir);
        self.roots
    }

    pub fn new(tcx: TyCtx<'tcx>) -> Self {
        Self { tcx, roots: Default::default() }
    }
}

impl<'tcx> FnVisitor<'tcx> for RootCollector<'tcx> {
    fn visit_fn(&mut self, def_id: ir::DefId) {
        // note we don't have to visit enum constructors as a root
        // as they does not call anything else
        let generics = self.tcx.generics_of(def_id);
        if !generics.params.is_empty() {
            return;
        }
        self.roots.push(def_id)
    }
}

/// finds all instantiations of generic items starting from a given (non-polymorphic) instance
/// note that although the instance could initially be polymorphic, the substitutions should
/// provide concrete instantiations for all its type parameters
struct InstanceCollector<'a, 'tcx> {
    collector: &'a MonomorphizationCollector<'a, 'tcx>,
    instance: Instance<'tcx>,
}

impl<'a, 'tcx> Monomorphize<'tcx> for InstanceCollector<'a, 'tcx> {
    fn monomorphize<T>(&self, t: T) -> T
    where
        T: TypeFoldable<'tcx>,
    {
        t.subst(self.collector.tcx, self.instance.substs)
    }
}

impl<'a, 'tcx> MirVisitor<'tcx> for InstanceCollector<'a, 'tcx> {
    fn visit_operand(&mut self, operand: &Operand<'tcx>) {
        // `Operand::Item` is currently the only way to reference a generic item
        if let &Operand::Item(def_id, fn_ty) = operand {
            // note that `fn_ty` is not the fully generic type obtained through collection
            // it is the type of the function after typechecking and so will only contain
            // type parameters if it was called in a generic context.
            // we monomorphize the function type with the "parent" instance's substitutions
            // and this should provide concrete types for the type parameters
            let mono_ty = self.monomorphize(fn_ty);
            // `ty` should have no type parameters after monomorphization
            debug_assert!(!mono_ty.has_ty_params());
            // this `substs` is the substitution applied to the generic function with def_id
            // `def_id` to obtain its concrete type
            let scheme = self.tcx.type_of(def_id);
            let substs = self.tcx.unify_scheme(scheme, mono_ty);
            let instance = Instance::resolve(self.tcx, def_id, substs);
            if let Some(prev) =
                self.collector.operand_instance_map.borrow_mut().insert((def_id, mono_ty), instance)
            {
                // the same operand key shouldn't map to different instances
                assert_eq!(prev, instance);
            }

            if !self.mono_instances.borrow().contains(&instance) {
                // recursively collect all its neighbours
                self.collector.collect_instance(instance);
            }
        }
    }
}

impl<'a, 'tcx> Deref for MonomorphizationCollector<'a, 'tcx> {
    type Target = CodegenCtx<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.cctx
    }
}

impl<'a, 'tcx> Deref for InstanceCollector<'a, 'tcx> {
    type Target = MonomorphizationCollector<'a, 'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.collector
    }
}
