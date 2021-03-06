use super::*;

#[test]
fn main_function_return_type_annotations() {
    let _tir = typeck!("fn main() -> int { 5 }");
}

#[test]
fn typeck_return_stmt_expr() {
    // note the trailing semicolon in the final return expression
    // this makes it an expression statement with type `()`
    // however, this is actually type correct
    // the current implementation is rather hacky, where the parser "upgrades" the final return
    // statement into a return expression
    let _tir = typeck!("fn main() -> int { return 5; }");
}
