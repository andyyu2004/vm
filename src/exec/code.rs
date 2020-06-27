use super::Op;
use crate::{exec::Type, util::As8Bytes};
use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub struct Code(Vec<u8>);

#[derive(Default)]
pub struct CodeBuilder {
    code: Vec<u8>,
}

impl CodeBuilder {
    fn emit_byte(mut self, byte: u8) -> Self {
        self.code.push(byte);
        self
    }

    pub fn emit_op(self, op: Op) -> Self {
        self.emit_byte(op as u8)
    }

    pub fn emit_closure(self, f_idx: u8, upvalues: Vec<(bool, u8)>) -> Self {
        let mut this = self.emit_op(Op::clsr).emit_byte(f_idx);
        for (in_enclosing, index) in upvalues {
            this = this.emit_upval(in_enclosing, index);
        }
        this
    }

    fn emit_upval(self, in_enclosing: bool, index: u8) -> Self {
        self.emit_byte(in_enclosing as u8).emit_byte(index)
    }

    pub fn emit_iconst(self, i: i64) -> Self {
        self.emit_op(Op::iconst).write_const(i)
    }

    pub fn emit_uconst(self, u: u64) -> Self {
        self.emit_op(Op::uconst).write_const(u)
    }

    /// writes a 8 byte constant into the code
    pub fn write_const(mut self, c: impl As8Bytes) -> Self {
        self.code.extend_from_slice(&c.as_bytes());
        self
    }

    pub fn emit_invoke(self, argc: u8) -> Self {
        self.emit_op(Op::invoke).emit_byte(argc)
    }

    pub fn emit_ldc(self, idx: u8) -> Self {
        self.emit_op(Op::ldc).emit_byte(idx)
    }

    pub fn emit_array(self, ty: Type, size: u64) -> Self {
        self.emit_uconst(size)
            .emit_op(Op::newarr)
            .emit_byte(ty as u8)
    }

    pub fn emit_iaload(self, index: u64) -> Self {
        self.emit_uconst(index as u64).emit_op(Op::iaload)
    }

    pub fn emit_iloadl(self, index: u8) -> Self {
        self.emit_op(Op::iloadl).emit_byte(index)
    }

    pub fn emit_istorel(self, index: u8, value: i64) -> Self {
        self.emit_iconst(value)
            .emit_op(Op::istorel)
            .emit_byte(index)
    }

    pub fn emit_iloadu(self, index: u8) -> Self {
        self.emit_op(Op::iloadu).emit_byte(index)
    }

    pub fn emit_istoreu(self, index: u8, value: i64) -> Self {
        self.emit_iconst(value)
            .emit_op(Op::istoreu)
            .emit_byte(index)
    }

    pub fn emit_iastore(self, index: u64, value: i64) -> Self {
        self.emit_uconst(index)
            .emit_iconst(value)
            .emit_op(Op::iastore)
    }

    pub fn build(self) -> Code {
        Code(self.code)
    }
}

impl Deref for Code {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Code {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
