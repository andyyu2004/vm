mod closure_tests;
mod enum_tests;
mod match_tests;
mod output_tests;
mod pattern_tests;
mod ptr_tests;
mod struct_tests;

pub macro llvm_exec_inner($src:expr) {
    ldriver::Driver::new($src).llvm_exec()
}
pub macro llvm_exec($src:expr) {
    llvm_exec_inner!($src).unwrap()
}

pub macro llvm_expect_error($src:expr) {
    llvm_exec_inner!($src).unwrap_err()
}