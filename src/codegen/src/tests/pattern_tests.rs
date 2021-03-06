//! pattern matching tests

use super::*;

#[test]
fn llvm_struct_pattern_test() {
    let src = r#"
    struct S {
        b: bool,
        x: int,
        y: int,
    }

    fn main() -> int {
        let s = S {
            x: 9,
            b: false,
            y: 7,
        };
        let S { x, y } = s;
        x - y
    }
    "#;

    assert_eq!(llvm_jit!(src), 2);
}

#[test]
fn llvm_unpack_tuple_in_let() {
    let src = r#"
    fn main() -> int {
        let (i, b) = mktuple();
        i
    }

    fn mktuple() -> (int, bool) {
        (30, true)
    }
    "#;
    assert_eq!(llvm_jit!(src), 30)
}

#[test]
fn llvm_unpack_nested_tuples_in_let() {
    // the code generated seems to be fine and even runs fine
    // but fails to verify?

    let src = r#"
    fn main() -> int {
        let (f, (i, b)) = mk_nested_tuple();
        i
    }

    fn mk_nested_tuple() -> (float, (int, bool)) {
        (90.0, (30, false))
    }
    "#;

    assert_eq!(llvm_jit!(src), 30)
}

#[test]
fn llvm_unpack_tuple_in_parameter() {
    let src = r#"
    fn main() -> int {
        let i = snd((false, 185));
        i
    }

    fn snd((b, i): (bool, int)) -> int {
        i
    }
    "#;

    assert_eq!(llvm_jit!(src), 185)
}
