use super::util::LLVMAsPtrVal;
use super::FnCtx;
use crate::ast;
use crate::ir::{self, DefId};
use crate::lexer::symbol;
use crate::mir::{self, *};
use crate::tir;
use crate::ty::{Const, ConstKind, SubstsRef, Ty, TyKind};
use crate::typeck::TyCtx;
use ast::Ident;
use inkwell::types::{BasicType, BasicTypeEnum, FloatType, FunctionType};
use inkwell::values::*;
use inkwell::{
    basic_block::BasicBlock, builder::Builder, context::Context, module::Module, passes::PassManager
};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::fmt::Display;
use std::ops::Deref;
use symbol::Symbol;

pub struct CodegenCtx<'tcx> {
    pub tcx: TyCtx<'tcx>,
    pub llctx: &'tcx Context,
    pub builder: Builder<'tcx>,
    pub fpm: PassManager<FunctionValue<'tcx>>,
    pub module: Module<'tcx>,
    pub vals: CommonValues<'tcx>,
    /// stores the `Ident` for a `DefId` which can then be used to lookup the function in the `llctx`
    /// this api is a bit awkward, but its what inkwell has so..
    pub items: RefCell<FxHashMap<DefId, Ident>>,
    curr_fn: Option<FunctionValue<'tcx>>,
}

pub struct CommonValues<'tcx> {
    zero: IntValue<'tcx>,
}

impl<'tcx> CodegenCtx<'tcx> {
    pub fn new(tcx: TyCtx<'tcx>, llctx: &'tcx Context) -> Self {
        let module = llctx.create_module("main");
        let fpm = PassManager::create(&module);
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();
        fpm.add_gvn_pass();
        fpm.add_cfg_simplification_pass();
        fpm.add_basic_alias_analysis_pass();
        fpm.add_promote_memory_to_register_pass();
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();
        fpm.initialize();
        let vals = CommonValues { zero: llctx.i64_type().const_zero() };
        Self {
            tcx,
            llctx,
            module,
            fpm,
            builder: llctx.create_builder(),
            vals,
            curr_fn: None,
            items: Default::default(),
        }
    }

    /// returns the main function
    pub fn codegen(&mut self, prog: &'tcx mir::Prog<'tcx>) -> FunctionValue<'tcx> {
        // we need to predeclare all items as we don't require them to be declared in the source
        // file in topological order
        for (id, item) in &prog.items {
            self.items.borrow_mut().insert(id.def, item.ident);
            match &item.kind {
                ItemKind::Fn(body) => {
                    let (_, ty) = self.tcx.item_ty(id.def).expect_scheme();
                    let (params, ret) = ty.expect_fn();
                    let llty = self.llvm_fn_ty(params, ret);
                    let llfn = self.module.add_function(item.ident.as_str(), llty, None);

                    // TODO
                    // define the parameters
                    // for (param, arg) in body.params.into_iter().zip(llfn.get_param_iter()) {
                    //     arg.set_name(&param.pat.id.to_string());
                    // }
                }
            };
        }
        for (id, item) in &prog.items {
            match &item.kind {
                ItemKind::Fn(body) => self.codegen_body(item, body),
            };
        }
        self.module.print_to_stderr();
        self.module.print_to_file("ir.ll").unwrap();
        self.module.verify().unwrap();
        self.module.get_function(symbol::MAIN.as_str()).unwrap()
    }

    fn codegen_body(
        &mut self,
        item: &mir::Item,
        body: &'tcx mir::Body<'tcx>,
    ) -> FunctionValue<'tcx> {
        let llfn = self.module.get_function(item.ident.as_str()).unwrap();
        let mut fcx = FnCtx::new(&self, body, llfn);
        fcx.codegen_body();
        llfn
    }

    pub fn llvm_fn_ty(&self, params: SubstsRef, ret: Ty) -> FunctionType<'tcx> {
        self.llvm_ty(ret).fn_type(&params.iter().map(|ty| self.llvm_ty(ty)).collect_vec(), false)
    }

    pub fn llvm_ty(&self, ty: Ty) -> BasicTypeEnum<'tcx> {
        match ty.kind {
            TyKind::Bool => self.llctx.bool_type().into(),
            TyKind::Char => todo!(),
            TyKind::Num => self.llctx.f64_type().into(),
            TyKind::Array(ty) => todo!(),
            TyKind::Fn(params, ret) =>
                self.llvm_fn_ty(params, ret).ptr_type(AddressSpace::Generic).into(),
            TyKind::Tuple(_) => todo!(),
            TyKind::Param(_) => todo!(),
            TyKind::Scheme(_, _) => todo!(),
            TyKind::Never => todo!(),
            TyKind::Error | TyKind::Infer(_) => unreachable!(),
            TyKind::Adt(..) => todo!(),
        }
    }
}

impl<'tcx> Deref for CodegenCtx<'tcx> {
    type Target = Builder<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.builder
    }
}