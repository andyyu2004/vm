use crate::mir::Mir;
use crate::ty::*;
use ir::DefId;
use rustc_hash::FxHashMap;
use std::cell::RefCell;

macro_rules! define_queries {
    (tcx: $tcx:tt, inputs: { $(([$name:ident] [$K:ty] [$R:ty]))* }) => {
        pub struct Queries {
            $(pub $name: for<$tcx> fn(TyCtx<$tcx>, $K) -> $R,)*
        }

        impl Default for Queries {
            fn default() -> Self {
                $(fn $name<$tcx>(_: TyCtx<$tcx>, key: $K) -> $R {
                    panic!("`tcx.{}({:?})` unprovided", stringify!($name), key);
                })*
                Self { $($name),* }
            }
        }
    };
}

macro_rules! define_query_caches {
    (tcx: $tcx:tt, inputs: {$(([$name:ident] [$K:ty] [$R:ty]))*}) => {

        #[derive(Default)]
        pub struct QueryCache<$tcx> {
            $($name: RefCell<FxHashMap<$K, $R>>),*
        }

        impl<$tcx> QueryCache<$tcx> {
            pub fn new() -> Self {
                Self {
                    $($name: Default::default()),*
                }
            }
        }
    };
}

macro_rules! define_query_context {
    (tcx: $tcx:tt, inputs: {$(([$name:ident] [$K:ty] [$R:ty]))*}) => {

        define_queries!(tcx: $tcx, inputs: { $(([$name] [$K] [$R]))* });
        define_query_caches!(tcx: $tcx, inputs: { $(([$name] [$K] [$R]))* });

        pub struct QueryCtx<'tcx> {
            cache: QueryCache<'tcx>,
            queries: Queries,
        }

        impl<'tcx> QueryCtx<'tcx> {
            pub fn new(queries: Queries) -> Self {
                Self { queries, cache: Default::default() }
            }
        }

        impl<'tcx> TyCtx<'tcx> {
            $(pub fn $name(self, key: $K) -> $R {
                // we must have a early return otherwise we will run into
                // refcell `BorrowMut` errors
                if let Some(&value) = self.cache.$name.borrow().get(&key) {
                    return value;
                }

                let value = (self.queries.$name)(self, key);
                // let ptr = self.cache.arena.$name.alloc(computed);
                self.cache.$name.borrow_mut().insert(key, value);
                value
            })*
        }
    };
}

// currently queries only deal with stuff that happens post ir generation
// maybe we can remove this inconsistency and make everything query based?
define_query_context! {
    tcx: 'tcx,
    inputs: {
        // master query
        ([analyze] [()] [()])

        // typecheck
        ([typeck] [DefId] [&'tcx TypeckTables<'tcx>])
        ([type_of] [DefId] [Ty<'tcx>])
        ([fn_sig] [DefId] [FnSig<'tcx>])
        ([adt_ty] [DefId] [&'tcx AdtTy])
        ([generics_of] [DefId] [&'tcx Generics<'tcx>])
        ([validate_item_type] [DefId] [()])
        ([inherent_impls] [()] [&'tcx InherentImpls])
        ([inherent_impls_of] [DefId] [&'tcx [DefId]])
        ([trait_impls] [()] [&'tcx TraitImpls])
        ([trait_impls_of] [DefId] [&'tcx [DefId]])

        // mir
        ([mir_of] [DefId] [&'tcx Mir<'tcx>])
        ([instance_mir] [Instance<'tcx>] [&'tcx Mir<'tcx>])

        // codegen
        ([monomorphization_instances] [()] [&'tcx Instances<'tcx>])
        ([resolve_instance] [(DefId, SubstsRef<'tcx>)] [Instance<'tcx>])
    }
}
