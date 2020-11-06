//! Sometimes, the pull-based query system is not sufficient to compile all programs correctly
//! consider an incorrect program such as the following
//!
//! struct S<T> {
//!     s: &S,
//! }
//!
//! fn main() -> int { 0 }
//!
//! Clearly, this should not compile as the field `s` has an incorrect number of type parameters
//! for the type `S`.
//!
//! However, using queries alone to compile will not catch this error as `S` is never referenced
//! from any function.
//!
//! One solution to this is to run some passes that will force everything to be checked, even if
//! never used.

use ir::{ItemDefVisitor, ItemVisitor};
use lcore::TyCtx;

/// runs all phases of analyses
/// if no errors are caught during this, then the code should be correct
/// and safe to codegen
fn analysis<'tcx>(tcx: TyCtx<'tcx>) {
    ItemTypeValidationPass::run_pass(tcx);
}

trait AnalysisPass<'tcx> {
    fn run_pass(tcx: TyCtx<'tcx>);
}

struct ItemTypeValidationPass<'tcx> {
    tcx: TyCtx<'tcx>,
}

impl<'tcx> AnalysisPass<'tcx> for ItemTypeValidationPass<'tcx> {
    fn run_pass(tcx: TyCtx<'tcx>) {
        Self { tcx }.visit_ir(tcx.ir);
    }
}

impl<'tcx> ItemVisitor<'tcx> for ItemTypeValidationPass<'tcx> {
    fn visit_item(&mut self, item: &'tcx ir::Item<'tcx>) {
        // self.tcx.check_item_type(item);
    }

    fn visit_impl_item(&mut self, _impl_item: &'tcx ir::ImplItem<'tcx>) {
    }
}
