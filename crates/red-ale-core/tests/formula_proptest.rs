//! Formulas: no input panics the parser, and arithmetic on whole numbers and bound variables gives what Numeric's
//! own exact operations give (`design/milestone4.md`, "Testing").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use red_ale_core::formula::{NumberFormat, Variables, evaluate, evaluate_arithmetic, free_variables};
use red_ale_core::{DENOM_AUTO, How, Numeric, NumericError};

const FORMATS: [NumberFormat; 3] = [
    NumberFormat {
        decimal_point: '.',
        group_separator: ',',
    },
    NumberFormat {
        decimal_point: ',',
        group_separator: '.',
    },
    NumberFormat {
        decimal_point: ',',
        group_separator: ' ',
    },
];

/// Every entry point on `text`, in every format, with a few bindings; only panics matter.
fn exercise(text: &str) {
    let vars: Variables = ["a", "i", "x"].iter().map(|v| ((*v).to_owned(), Numeric::new(7, 2))).collect();
    FORMATS.iter().for_each(|f| {
        let _ = evaluate_arithmetic(text, *f);
        let _ = evaluate(text, *f, &vars);
        let _ = free_variables(text, *f);
    });
}

/// An arithmetic expression over whole numbers and the variables `a`, `b`, `c`.
#[derive(Debug, Clone)]
enum Expr {
    Int(i64),
    Var(usize),
    Op(Box<Expr>, char, Box<Expr>),
    Neg(Box<Expr>),
}

const NAMES: [&str; 3] = ["a", "b", "c"];

fn expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![(-1000_i64..1000).prop_map(Expr::Int), (0..3_usize).prop_map(Expr::Var)];
    leaf.prop_recursive(4, 24, 2, |inner| {
        prop_oneof![
            (inner.clone(), prop::sample::select(vec!['+', '-', '*', '/']), inner.clone())
                .prop_map(|(l, op, r)| Expr::Op(Box::new(l), op, Box::new(r))),
            inner.prop_map(|e| Expr::Neg(Box::new(e))),
        ]
    })
}

/// The formula, every operation in parentheses so precedence doesn't matter; a lone leaf is never parenthesised, so
/// GnuCash's `(n)` negation never applies.
fn render(e: &Expr) -> String {
    match e {
        Expr::Int(n) if *n < 0 => format!("-{}", n.unsigned_abs()),
        Expr::Int(n) => n.to_string(),
        Expr::Var(i) => NAMES[*i].to_owned(),
        Expr::Op(l, op, r) => format!("({} {op} {})", render(l), render(r)),
        Expr::Neg(e) => format!("-{}", render(e)),
    }
}

/// The same with Numeric's exact operations (`numeric_ops`, `gnc-exp-parser.c:383-421`).
fn expected(e: &Expr, vars: &[Numeric; 3]) -> Result<Numeric, NumericError> {
    let exact = How::EXACT;
    match e {
        Expr::Int(n) => Ok(Numeric::from_int(*n)),
        Expr::Var(i) => Ok(vars[*i]),
        Expr::Neg(e) => expected(e, vars).map(Numeric::neg),
        Expr::Op(l, op, r) => {
            let (l, r) = (expected(l, vars)?, expected(r, vars)?);
            match op {
                '+' => l.add(r, DENOM_AUTO, exact),
                '-' => l.sub(r, DENOM_AUTO, exact),
                '*' => l.mul(r, DENOM_AUTO, exact),
                _ if r.is_zero() => Err(NumericError::DivideByZero),
                _ => l.div(r, DENOM_AUTO, exact),
            }
        }
    }
}

/// A Neg of a variable negates the variable for the rest of the formula, as GnuCash does; the comparison leaves
/// those out.
fn negates_a_variable(e: &Expr) -> bool {
    match e {
        Expr::Neg(inner) => matches!(**inner, Expr::Var(_)) || negates_a_variable(inner),
        Expr::Op(l, _, r) => negates_a_variable(l) || negates_a_variable(r),
        _ => false,
    }
}

/// The variables an expression names, in order of first appearance.
fn names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Var(i) if !out.iter().any(|n| n == NAMES[*i]) => out.push(NAMES[*i].to_owned()),
        Expr::Op(l, _, r) => {
            names(l, out);
            names(r, out);
        }
        Expr::Neg(e) => names(e, out),
        _ => {}
    }
}

fn formula_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::sample::select(vec![
                "pmt(", "ipmt(", "ppmt(", "futureValue(", "periodic_rate(", "computeInterestIncrement(", "foobar(", "a", "i", "x",
                "=", "+=", "*=", ":", "(", ")", "+", "-", "*", "/", "\"s\"", " ", "0", "1", "12", "360", "0.005", "1,234.5",
                "9223372036854775807",
            ])
            .prop_map(str::to_owned),
            "[0-9.,:()=+*/a-z\" -]{1,4}",
        ],
        0..24,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_text_never_panics(text in any::<String>()) {
        exercise(&text);
    }

    #[test]
    fn formula_like_text_never_panics(text in formula_text()) {
        exercise(&text);
    }

    #[test]
    fn whole_number_arithmetic_matches_numeric(e in expr(), a in -50_i64..50, b in -50_i64..50, c in 1_i64..50) {
        prop_assume!(!negates_a_variable(&e));
        let values = [Numeric::from_int(a), Numeric::new(b, 4), Numeric::new(c, 3)];
        let vars: Variables = NAMES.iter().zip(values).map(|(n, v)| ((*n).to_owned(), v)).collect();
        let text = render(&e);
        let got = evaluate(&text, NumberFormat::default(), &vars);
        match expected(&e, &values) {
            Ok(want) => prop_assert!(got.as_ref().is_ok_and(|g| g.same_repr(want.reduce())), "{text}: {got:?} vs {want}"),
            Err(_) => prop_assert!(got.is_err(), "{text}: {got:?}"),
        }
        let mut want_names = Vec::new();
        names(&e, &mut want_names);
        prop_assert_eq!(free_variables(&text, NumberFormat::default()), Ok(want_names));
    }
}
