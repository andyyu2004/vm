use super::*;
use itertools::Itertools;

#[test]
#[should_panic]
fn check_lambda_wrong_arity_0() {
    typeck_expr!("(fn () => 5)(3); 5");
}

#[test]
#[should_panic]
fn check_lambda_wrong_arity_1() {
    typeck_expr!("(fn (x) => x)(3, 5); 5");
}

#[test]
#[should_panic]
fn check_call_non_function() {
    typeck!("5(5); 5");
}

#[test]
fn check_recursive_named_closure() {
    let tir = typeck_expr!("fn f() { f() }");
}

#[test]
fn check_simple_lambda_no_capture() {
    let tir = typeck_expr!("fn () => 5; 5");
    let lines = lines!(&tir);
    assert_eq!(lines[0], "(λ() 5:number):fn()->number;");
}

#[test]
fn check_simple_lambda_with_parameter_no_capture() {
    let tir = typeck_expr!("fn (x) => 5 + x; 5");
    let lines = lines!(&tir);
    assert_eq!(lines[0], "(λ(x:number) (+ 5:number x:number):number):fn(number)->number;");
}

#[test]
fn check_fn_call() {
    let tir = typeck_expr!("let f = fn(x) => x; f(3)");
    let lines = lines!(&tir);
    assert_eq!(lines[0], "let f:fn(number)->number = (λ(x:number) x:number):fn(number)->number;");
    assert_eq!(lines[1], "(f:fn(number)->number 3:number):number");
}

#[test]
fn check_lambda_with_capture() {
    let tir = typeck_expr!("let num = 55; fn(y) => num + y; num");
    let lines = lines!(&tir);
    assert_eq!(lines[0], "let num:number = 55:number;");
    assert_eq!(lines[1], "(λ(y:number) (+ num:number y:number):number):fn(number)->number;");
}

#[test]
fn check_higher_order_lambda() {
    let tir = typeck_expr!("let f = fn(x) => false; let g = fn(p) => p(3); g(f); 5");
    let lines = lines!(&tir);
    // note `false` is represented as `0`
    assert_eq!(lines[0], "let f:fn(number)->bool = (λ(x:number) 0:bool):fn(number)->bool;");
    assert_eq!(
        lines[1],
        "let g:fn(fn(number)->bool)->bool = (λ(p:fn(number)->bool) (p:fn(number)->bool 3:number):bool):fn(fn(number)->bool)->bool;"
    );
    assert_eq!(lines[2], "(g:fn(fn(number)->bool)->bool f:fn(number)->bool):bool;");
}