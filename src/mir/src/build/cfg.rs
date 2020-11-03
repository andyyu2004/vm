//! control flow graph

use super::{BasicBlock, BlockId, Builder, ReleaseInfo, ENTRY_BLOCK};
use index::IndexVec;
use lcore::mir::*;


pub struct Cfg<'tcx> {
    pub(super) basic_blocks: IndexVec<BlockId, BasicBlock<'tcx>>,
}

impl Default for Cfg<'_> {
    fn default() -> Self {
        let mut cfg = Self { basic_blocks: Default::default() };
        assert_eq!(cfg.append_basic_block(), ENTRY_BLOCK);
        cfg
    }
}

impl<'tcx> Cfg<'tcx> {
    pub fn append_basic_block(&mut self) -> BlockId {
        self.basic_blocks.push(BasicBlock::default())
    }

    pub fn push_assignment(
        &mut self,
        info: SpanInfo,
        block: BlockId,
        lvalue: Lvalue<'tcx>,
        rvalue: Rvalue<'tcx>,
    ) {
        self.push(block, Stmt { info, kind: StmtKind::Assign(lvalue, rvalue) });
    }

    pub fn push(&mut self, block: BlockId, stmt: Stmt<'tcx>) {
        self.basic_blocks[block].stmts.push(stmt);
    }

    fn block_mut(&mut self, block: BlockId) -> &mut BasicBlock<'tcx> {
        &mut self.basic_blocks[block]
    }

    pub fn terminate(&mut self, info: SpanInfo, block: BlockId, kind: TerminatorKind<'tcx>) {
        let block = self.block_mut(block);
        debug_assert!(block.terminator.is_none(), "block already has terminator");
        block.terminator = Some(Terminator { info, kind })
    }
}

impl<'a, 'tcx> Builder<'a, 'tcx> {
    pub fn append_basic_block(&mut self) -> BlockId {
        self.cfg.append_basic_block()
    }

    /// branch inst
    pub fn branch(&mut self, info: SpanInfo, from: BlockId, to: BlockId) {
        self.terminate(info, from, TerminatorKind::Branch(to))
    }

    pub fn terminate(&mut self, info: SpanInfo, block: BlockId, kind: TerminatorKind<'tcx>) {
        self.cfg.terminate(info, block, kind)
    }

    pub fn push_release(&mut self, block: BlockId, release: ReleaseInfo) {
        let ReleaseInfo { info, var } = release;
        self.push(block, Stmt { info, kind: StmtKind::Release(var) })
    }

    pub fn push_retain(&mut self, info: SpanInfo, block: BlockId, var: VarId) {
        self.push(block, Stmt { info, kind: StmtKind::Retain(var) })
    }

    /// push a statement onto the given block
    pub fn push(&mut self, block: BlockId, stmt: Stmt<'tcx>) {
        self.cfg.basic_blocks[block].stmts.push(stmt);
    }

    /// writes a unit into `lvalue`
    pub fn push_assign_unit(&mut self, info: SpanInfo, block: BlockId, lvalue: Lvalue<'tcx>) {
        let unit = self.tcx.mk_const_unit();
        self.push_assignment(info, block, lvalue, Rvalue::Operand(Operand::Const(unit)));
    }

    pub fn push_assignment(
        &mut self,
        info: SpanInfo,
        block: BlockId,
        lvalue: Lvalue<'tcx>,
        rvalue: Rvalue<'tcx>,
    ) {
        self.cfg.push_assignment(info, block, lvalue, rvalue);

        // if the type is pointer, then it is a box and we need to do refcounting
        // TODO need to differentiate between initialization and reassignments
        // https://youtu.be/Ntj8ab-5cvE?t=2328
        let var = self.vars[lvalue.id];
        if var.ty.is_box() && lvalue.projs.is_empty() {
            self.push_retain(info, block, lvalue.id);
            self.schedule_release(info, lvalue.id);
        }
    }
}
