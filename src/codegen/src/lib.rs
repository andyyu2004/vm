#![feature(decl_macro)]
#![feature(crate_visibility_modifier)]

#[macro_use]
extern crate log;

mod codegen_ctx;
mod fcx;
mod gc;
mod intrinsics;
mod layout;
mod lltypes;
mod llvm_error;
mod llvm_intrinsics;
mod monomorphize;
mod native;

#[cfg(test)]
mod tests;

pub use codegen_ctx::CodegenCtx;
pub use fcx::FnCtx;

use gc::GCFunctions;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use lcore::queries::Queries;
use lcore::ty::Ty;
use llvm_error::LLVMError;
use llvm_intrinsics::LLVMIntrinsics;
use monomorphize::Monomorphize;
use native::{NativeFunctions, NativeFunctionsBuilder};

pub fn provide(queries: &mut Queries) {
    monomorphize::provide(queries);
}

pub trait LLVMAsPtrVal<'tcx> {
    fn as_llvm_ptr(self) -> PointerValue<'tcx>;
}

impl<'tcx> LLVMAsPtrVal<'tcx> for FunctionValue<'tcx> {
    fn as_llvm_ptr(self) -> PointerValue<'tcx> {
        unsafe { std::mem::transmute(self) }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct LvalueRef<'tcx> {
    ptr: PointerValue<'tcx>,
    ty: Ty<'tcx>,
}

#[derive(Debug, Clone, Copy)]
struct ValueRef<'tcx> {
    val: BasicValueEnum<'tcx>,
    ty: Ty<'tcx>,
}

#[derive(Debug, Clone, Copy)]
struct LLVMVar<'tcx> {
    ptr: PointerValue<'tcx>,
}
