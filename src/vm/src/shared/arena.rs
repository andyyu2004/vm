use crate::{
    arena::{DroplessArena, TypedArena}, ty::{Ty, TyKind, TyS}
};

/// collective arena which contains all main arenas
#[derive(Default)]
crate struct Arena<'tcx> {
    pub dropless: DroplessArena,
    pub ir: DroplessArena,
    tys: TypedArena<TyS<'tcx>>,
    tir: DroplessArena,
}

impl<'tcx> Arena<'tcx> {
    pub fn alloc_ty(&'tcx self, kind: TyKind<'tcx>) -> Ty<'tcx> {
        let ty_structure = TyS { kind };
        self.tys.alloc(ty_structure)
    }

    pub fn alloc_tir<T>(&self, tir: T) -> &T {
        self.tir.alloc(tir)
    }
}
