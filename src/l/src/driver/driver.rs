use super::Session;
use crate::arena::DroplessArena;
use crate::ast::{self, P};
use crate::compiler::{Compiler, Executable, GlobalCompilerCtx};
use crate::core::Arena;
use crate::error::{DiagnosticBuilder, LError, LResult, ParseResult};
use crate::gc::GC;
use crate::jit::{self, JitCtx};
use crate::llvm::CodegenCtx;
use crate::pluralize;
use crate::resolve::{Resolutions, Resolver, ResolverArenas};
use crate::typeck::{GlobalCtx, TyCtx};
use crate::{exec, ir, lexer, mir, parser, span, tir};
use exec::VM;
use inkwell::{context::Context as LLVMCtx, values::FunctionValue, OptimizationLevel};
use ir::AstLoweringCtx;
use lexer::{symbol, Lexer, Tok};
use once_cell::unsync::OnceCell;
use parser::Parser;
use span::SourceMap;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Driver<'tcx> {
    arena: Arena<'tcx>,
    resolver_arenas: ResolverArenas<'tcx>,
    global_ctx: OnceCell<GlobalCtx<'tcx>>,
    sess: Session,
}

#[macro_export]
macro_rules! pluralize {
    ($x:expr) => {
        if $x != 1 { "s" } else { "" }
    };
}

/// exits if any errors have been reported
macro check_errors($self:expr, $ret:expr) {{
    if $self.sess.has_errors() {
        let errc = $self.sess.err_count();
        e_red_ln!("{} error{} emitted", errc, pluralize!(errc));
        Err(LError::ErrorReported)
    } else {
        Ok($ret)
    }
}}

impl<'tcx> Driver<'tcx> {
    pub fn new(src: &str) -> Self {
        span::GLOBALS
            .with(|globals| *globals.source_map.borrow_mut() = Some(Rc::new(SourceMap::new(src))));
        Self {
            resolver_arenas: Default::default(),
            arena: Default::default(),
            global_ctx: OnceCell::new(),
            sess: Default::default(),
        }
    }

    pub fn lex(&self) -> LResult<Vec<Tok>> {
        let mut lexer = Lexer::new();
        let tokens = lexer.lex();
        Ok(tokens)
    }

    /// used for testing parsing
    pub fn parse_expr(&self) -> Option<P<ast::Expr>> {
        let tokens = self.lex().unwrap();
        let mut parser = Parser::new(&self.sess, tokens);
        parser.parse_expr().map_err(|err| err.emit()).ok()
    }

    pub fn parse(&self) -> LResult<P<ast::Prog>> {
        let tokens = self.lex()?;
        let mut parser = Parser::new(&self.sess, tokens);
        let ast = parser.parse();
        error!("{:#?}", ast);
        check_errors!(self, ast.unwrap())
    }

    pub fn gen_ir(&'tcx self) -> LResult<(&'tcx ir::IR<'tcx>, Resolutions)> {
        let ast = self.parse()?;
        let mut resolver = Resolver::new(&self.sess, &self.resolver_arenas);
        resolver.resolve(&ast);
        let lctx = AstLoweringCtx::new(&self.arena, &mut resolver);
        let ir = lctx.lower_prog(&ast);
        info!("{:#?}", ir);
        let resolutions = resolver.complete();
        Ok((ir, resolutions))
    }

    fn with_tcx<R>(&'tcx self, f: impl FnOnce(TyCtx<'tcx>) -> R) -> LResult<R> {
        let (ir, mut resolutions) = self.gen_ir()?;
        let resolutions = self.arena.alloc(std::mem::take(&mut resolutions));
        let gcx = self
            .global_ctx
            .get_or_init(|| GlobalCtx::new(ir, &self.arena, &resolutions, &self.sess));
        let ret = gcx.enter_tcx(|tcx| {
            tcx.run_typeck();
            f(tcx)
        });
        check_errors!(self, ret)
    }

    pub fn gen_tir(&'tcx self) -> LResult<tir::Prog<'tcx>> {
        self.with_tcx(|tcx| tcx.build_tir())
    }

    pub fn dump_mir(&'tcx self) -> LResult<()> {
        self.with_tcx(|tcx| tcx.dump_mir(&mut std::io::stderr()))
    }

    pub fn check(&'tcx self) -> LResult<()> {
        self.with_tcx(|tcx| tcx.check())
    }

    pub fn create_codegen_ctx(&'tcx self) -> LResult<CodegenCtx> {
        let llvm_ctx = LLVMCtx::create();
        self.with_tcx(|tcx| CodegenCtx::new(tcx, self.arena.alloc(llvm_ctx)))
    }

    pub fn llvm_compile(&'tcx self) -> LResult<(CodegenCtx, FunctionValue<'tcx>)> {
        let mut cctx = self.create_codegen_ctx()?;
        let main_fn = cctx.codegen();
        check_errors!(self, (cctx, main_fn.unwrap()))
    }

    pub fn llvm_exec(&'tcx self) -> LResult<i32> {
        let (cctx, main_fn) = self.llvm_compile()?;
        dbg!("llvm codegen complete");
        let jit = cctx.module.create_jit_execution_engine(OptimizationLevel::Default).unwrap();
        let val = unsafe { jit.run_function_as_main(main_fn, &[]) };
        Ok(val)
    }

    pub fn llvm_jit(&'tcx self) -> LResult<i32> {
        let cctx = self.create_codegen_ctx()?;
        let jcx = JitCtx::new(&cctx, GC::default());
        todo!()
    }

    // pub fn compile(&'tcx self) -> LResult<Executable> {
    //     let tir = self.gen_mir()?;
    //     println!("{}", tir);
    //     let gcx = self.global_ctx.get().unwrap();

    //     let cctx = gcx.enter_tcx(|tcx| self.arena.alloc(GlobalCompilerCtx::new(tcx)));
    //     let executable = Compiler::new(cctx).compile(&tir);
    //     println!("{}", executable);
    //     Ok(executable)
    // }

    // pub fn exec(&'tcx self) -> LResult<exec::Val> {
    //     let executable = self.compile()?;
    //     let mut vm = VM::with_default_gc(executable);
    //     let value = vm.run()?;
    //     Ok(value)
    // }

    #[cfg(test)]
    pub fn has_errors(&self) -> bool {
        self.sess.has_errors()
    }
}
