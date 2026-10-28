//! Formulas: GnuCash's parser tests, the hand-worked cases of the scheduled-transaction note, and the financial
//! functions (`design/notes/engine-scheduled-transactions.md` §3, §7 and §9.3).
//!
//! The parser cases are ported from `5.17:libgnucash/app-utils/test/test-exp-parser.c` (GPL-2.0-or-later). Its
//! function cases call Scheme procedures the test defines in Guile (`:194-230`); here they are [`TestFunctions`], the
//! same procedures in Rust, so no test runs Guile. GnuCash's error offsets are not checked: red-ale's errors carry no
//! offset. The financial values are worked by hand from the cited `5.17:bindings/guile/fin.scm` lines with exact
//! fractions, then rounded as `5.17:libgnucash/app-utils/gnc-exp-parser.c:346-349` rounds a function's result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::Numeric;
use red_ale_core::formula::{
    Argument, FormulaError, FunctionError, Functions, MAX_DEPTH, NumberFormat, Variables, evaluate, evaluate_arithmetic, evaluate_with,
    free_variables,
};

/// The procedures `test-exp-parser.c` defines in Scheme (`:195`, `:206`, `:216-220`, `:227`).
struct TestFunctions;

impl Functions for TestFunctions {
    fn call(&self, name: &str, args: &[Argument<'_>]) -> Result<Numeric, FunctionError> {
        let number = |a: &Argument<'_>| match a {
            Argument::Number(n) => Ok(*n),
            Argument::Text(_) => Err(FunctionError::Text),
        };
        let exact = |r: Result<Numeric, _>| r.map_err(|_| FunctionError::Range);
        match (name, args) {
            ("plus", [a, b]) => exact(number(a)?.add_exact(number(b)?)),
            ("sub", [a, b]) => exact(number(a)?.sub_exact(number(b)?)),
            ("test_str", [s, b]) => {
                let n = match s {
                    Argument::Text("one") => 1,
                    Argument::Text("two") => 2,
                    Argument::Text("three") => 3,
                    _ => 0,
                };
                exact(number(b)?.add_exact(Numeric::from_int(n)))
            }
            ("blindreturn", [v]) => number(v),
            _ => Err(FunctionError::Unknown),
        }
    }
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

/// `gnc_exp_parser_parse`: every unknown name is 0 (`expression_parser.c:677-705`), which red-ale's caller asks for by
/// binding the formula's variables to 0.
fn gnucash_parse(text: &str) -> Result<Numeric, FormulaError> {
    let zeros: Variables = free_variables(text, NumberFormat::default())?
        .into_iter()
        .map(|v| (v, Numeric::ZERO))
        .collect();
    evaluate_with(text, NumberFormat::default(), &zeros, &TestFunctions)
}

fn pass(text: &str, expected: Numeric) {
    match gnucash_parse(text) {
        Ok(v) => assert!(v.same_value(expected), "{text:?} gave {v}, expected {expected}"),
        Err(e) => panic!("{text:?} failed: {e}"),
    }
}

fn fail(text: &str) {
    assert!(gnucash_parse(text).is_err(), "{text:?} gave {:?}", gnucash_parse(text));
}

fn bind(pairs: &[(&str, Numeric)]) -> Variables {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

fn eval(text: &str, vars: &Variables) -> Result<Numeric, FormulaError> {
    evaluate(text, NumberFormat::default(), vars)
}

fn financial(text: &str) -> Result<Numeric, FormulaError> {
    eval(text, &Variables::new())
}

fn assert_repr(text: &str, expected: Numeric) {
    let got = financial(text).unwrap_or_else(|e| panic!("{text:?} failed: {e}"));
    assert!(got.same_value(expected), "{text:?} gave {got}, expected {expected}");
}

fn function_error(text: &str) -> FunctionError {
    match financial(text) {
        Err(FormulaError::Function { error, .. }) => error,
        other => panic!("{text:?} gave {other:?}"),
    }
}

#[test]
fn gnucash_failures() {
    // test-exp-parser.c:154-168. The NULL expression of :153 can't be written in Rust.
    [
        "",
        "  \t\n",
        "\\",
        "1 +",
        "1 2",
        "1 ç",
        "ç 1",
        "1 asdf",
        "asdf 1",
        "asdf jkl",
        "  (5 + 23)/   ",
        "  ((((5 + 23)/   ",
        "  4 / (1 - 1)",
    ]
    .iter()
    .for_each(|t| fail(t));
    // :178-183
    [
        "AUD $1.23",
        "AUD $0.0",
        "AUD 1.23",
        "AUD 0.0",
        "AUD 1.2 + CAN 2.3",
        "AUD $1.2 + CAN $2.3",
    ]
    .iter()
    .for_each(|t| fail(t));
}

#[test]
fn gnucash_arithmetic() {
    pass("0", Numeric::ZERO); // :170
    pass("\n\t   0  ", Numeric::ZERO); // :171
    pass("1 + 2", n(3, 1)); // :172
    pass("17.3 - 12.3000", n(5, 1)); // :173
    pass("5 * 6", n(30, 1)); // :174
    pass(" 34 / (22) ", n(34, 22)); // :175
    pass(" (4 + 5 * 2) - 7 / 3", n(35, 3)); // :176
    pass("(a = 42) + (b = 12) - a", n(12, 1)); // :177
    pass("1 + 2 * 3 + 4 + 5 * 6 * 7", n(221, 1)); // :185
    pass("1 - 2 * 3 + 4 - 5 * 6 * 7", n(-211, 1)); // :186
    pass(
        "22.32 * 2 + 16.8 + 34.2 * 2 + 18.81 + 85.44- 42.72 + 13.32 + 15.48 + 23.4 + 115.4",
        n(35897, 100),
    ); // :187-190
}

#[test]
fn gnucash_function_calls() {
    pass("plus(2 : 1)", n(3, 1)); // :196
    fail("plus(1:2) plus(3:4)"); // :197
    pass("plus( 1 : 2 ) + 3", n(6, 1)); // :198
    pass("plus( 1 : 2 ) * 3", n(9, 1)); // :199
    pass("plus( 1 + 2 : 3 ) * 5", n(30, 1)); // :200
    pass("plus( ( 1 + 2 ) * 3 : 4 ) + 5", n(18, 1)); // :201
    pass("5 + plus( ( 1 + 2 ) * 3 : 4 )", n(18, 1)); // :202
    pass("plus( plus( 1 : 2 ) : 3 )", n(6, 1)); // :203
    pass("plus( 4 : plus( plus( 1 : 2 ) : 3))", n(10, 1)); // :204
    pass("sub( 1 : 2 ) + 4", n(3, 1)); // :207
    pass("sub( (1 + 2 * 3) : 4 ) + 5", n(8, 1)); // :209
    pass("sub( 1 : 2 ) + sub( 3 : 4 ) + 5", n(3, 1)); // :211
    pass("sub( a = 42 : sub( plus( 1 : 2 ) : 6 * 7 )) + a", n(123, 1)); // :213
    pass("test_str( \"one\" : 1 )", n(2, 1)); // :221
    pass("test_str( \"two\" : 2 )", n(4, 1)); // :222
    fail("test_str( 3 : \"three\" )"); // :223
    pass("test_str( \"asdf\" : 1 )", n(1, 1)); // :224
    fail("\"asdf\" + 0"); // :225
    pass("blindreturn( 123.1 )", n(1231, 10)); // :228
    pass("blindreturn( 123.01 )", n(12301, 100)); // :229
    pass("blindreturn( 123.001 )", n(123_001, 1000)); // :230
}

#[test]
fn gnucash_variable_listing() {
    // test-exp-parser.c:244-245: "123 + a" names one variable.
    assert_eq!(free_variables("123 + a", NumberFormat::default()), Ok(vec!["a".to_owned()]));
}

#[test]
fn h1_parser_without_functions() {
    // Note §9.3 H1: expression_parser.c:583-639, :609-620 ((I)), :677-705, :871-1096; gnc-exp-parser.c:362-421.
    pass("1 + 2", n(3, 1));
    pass("(5)", n(-5, 1));
    pass(" ( 5 ) ", n(-5, 1));
    pass("(5) + 1", n(6, 1));
    pass("(-5)", n(-5, 1));
    pass("((5))", n(5, 1));
    pass("1,234.5 * 2", n(2469, 1));
    pass("a = 3", n(3, 1));
    pass("a + b", Numeric::ZERO);
    pass("10 / 4", n(5, 2));
    ["1 / 0", "1 2", "1 asdf"].iter().for_each(|t| fail(t));
}

#[test]
fn variables_are_bound_by_the_caller_and_unbound_reads_are_named() {
    let vars = bind(&[("amt", n(25, 2)), ("i", n(1, 1))]);
    assert_eq!(eval("amt * 2 + i", &vars), Ok(n(26, 1)), "H6: amt = 12.5, i = 1");
    assert_eq!(eval("amt * 2 + extra", &vars), Err(FormulaError::Unbound("extra".into())));
    assert_eq!(eval("x = 5", &Variables::new()), Ok(n(5, 1)), "an assignment needs no value first");
    assert_eq!(eval("x += 5", &Variables::new()), Err(FormulaError::Unbound("x".into())));
    let unset = bind(&[("x", Numeric::new(1, 0))]);
    assert_eq!(
        eval("x", &unset),
        Err(FormulaError::Unbound("x".into())),
        "an invalid Numeric is GnuCash's unset variable"
    );
}

#[test]
fn assignments_and_compound_assignments() {
    let vars = bind(&[("a", n(10, 1))]);
    assert_eq!(eval("a += 2", &vars), Ok(n(12, 1)));
    assert_eq!(eval("a -= 2", &vars), Ok(n(8, 1)));
    assert_eq!(eval("a *= 2", &vars), Ok(n(20, 1)));
    assert_eq!(eval("a /= 4", &vars), Ok(n(5, 2)));
    assert_eq!(eval("(a += 1) + a", &vars), Ok(n(22, 1)));
    assert_eq!(eval("jkl = 5 * (nj = 68.9)", &vars), Ok(n(689, 2)), "note §3.1");
    assert_eq!(eval("b = c = 3", &vars), Ok(n(3, 1)), "right-associative");
    assert_eq!(eval("1 = 2", &vars), Err(FormulaError::NotAVariable));
    assert_eq!(eval("a + 1 = 2", &vars), Err(FormulaError::NotAVariable));
    ["a == 1", "(= 1", "a := 1"]
        .iter()
        .for_each(|t| assert!(eval(t, &vars).is_err(), "{t:?}"));
}

#[test]
fn variables_are_references_as_in_gnucash() {
    // A name is read when an operator uses it (expression_parser.c:961-1075), and a sign negates the variable itself
    // (:1153-1165; gnc-exp-parser.c:424-435).
    let vars = bind(&[("a", n(1, 1))]);
    assert_eq!(eval("-a + a", &vars), Ok(n(-2, 1)));
    assert_eq!(eval("a * (a = 2)", &vars), Ok(n(4, 1)));
    assert_eq!(eval("-a", &vars), Ok(n(-1, 1)));
}

#[test]
fn grammar_errors() {
    let vars = bind(&[("a", n(1, 1))]);
    [
        "pmt (0.005 : 360 : 100000 : 0 : 0)", // note §3.1: a variable then a parenthesis
        "pmt(0.005, 360, 100000, 0, 0)",      // H4: `,` is not the separator
        "foobar(1 (2))",
        "foobar(1 :)",
        "foobar(1",
        "foobar(1 : : 2)",
        "\"abc\"",
        "foobar(\"abc)",
        "a 1",
        "2a",
        "(1 + 2",
        "1 + 2)",
        ":",
    ]
    .iter()
    .for_each(|t| assert!(eval(t, &vars).is_err(), "{t:?} gave {:?}", eval(t, &vars)));
    assert_eq!(eval("(\"abc\")", &vars), Err(FormulaError::Text("abc".into())));
}

#[test]
fn nesting_is_limited() {
    let deep = |d: usize| format!("{}1{}", "(".repeat(d), ")".repeat(d));
    assert_eq!(financial(&deep(MAX_DEPTH - 1)), Ok(n(1, 1)));
    assert_eq!(financial(&deep(MAX_DEPTH * 10)), Err(FormulaError::TooDeep));
    assert_eq!(financial(&"-".repeat(MAX_DEPTH * 10)), Err(FormulaError::TooDeep));
    assert_eq!(financial(&"a = ".repeat(MAX_DEPTH * 10)), Err(FormulaError::TooDeep));
}

#[test]
fn numbers_in_the_locale() {
    let french = NumberFormat {
        decimal_point: ',',
        group_separator: '.',
    };
    assert_eq!(evaluate("1.234,5 * 2", french, &Variables::new()), Ok(n(2469, 1)));
    assert_eq!(
        evaluate("pmt(0,005 : 360 : 100.000 : 0 : 0)", french, &Variables::new()),
        Ok(n(599_550_525_153, 1_000_000_000))
    );
}

#[test]
fn free_variables_in_order_of_first_appearance() {
    let f = NumberFormat::default();
    let names = |t: &str| free_variables(t, f).map(|v| v.join(","));
    assert_eq!(names("b + a * b - c"), Ok("b,a,c".into()));
    assert_eq!(names("x = 5"), Ok("x".into()), "assigned variables are variables (note §3.5)");
    assert_eq!(
        names("ppmt( 0.06 / 12 : i : 360 : 100000 : 0 : 0 )"),
        Ok("i".into()),
        "function names are not variables"
    );
    assert_eq!(
        names("sub( a = 42 : nosuch( b ) ) + a"),
        Ok("a,b".into()),
        "calls are not evaluated"
    );
    assert_eq!(names("x / y"), Ok("x,y".into()), "nor is arithmetic");
    assert_eq!(names("100"), Ok(String::new()));
    assert!(names("a b").is_err());
}

#[test]
fn arithmetic_mode_refuses_what_formulas_add() {
    let f = NumberFormat::default();
    ["a", "a = 1", "foobar(1)", "\"x\"", "1 : 2"].iter().for_each(|t| {
        assert!(matches!(evaluate_arithmetic(t, f), Err(FormulaError::Unexpected(_))), "{t:?}");
    });
    assert_eq!(
        evaluate_arithmetic("(5)", f),
        Ok(n(-5, 1)),
        "(n) alone is negative in business numbers too, as in GnuCash"
    );
    assert_eq!(
        evaluate_arithmetic("2 3", f),
        Err(FormulaError::Unexpected(String::new())),
        "v0.3's message"
    );
}

#[test]
fn overflow_is_an_error() {
    let big = i64::MAX;
    assert!(matches!(financial(&format!("{big} * {big}")), Err(FormulaError::Arithmetic(_))));
    let vars = bind(&[("m", Numeric::new(i64::MIN, 1))]);
    assert!(matches!(eval("-m", &vars), Err(FormulaError::Arithmetic(_))));
}

#[test]
fn h2_pmt() {
    // fin.scm:49-51, :75-90; exact 599.55052515275239…, 12 significant figures half up (gnc-numeric.cpp:87-119,
    // :467-482).
    let expected = n(599_550_525_153, 1_000_000_000);
    assert_repr("pmt(0.005 : 360 : 100000 : 0 : 0)", expected);
    assert_repr("pmt( 0.06 / 12 : 360 : 100000 : 0 : 0 )", expected);
    assert_repr("pmt(0.005 : 360 : -100000 : 0 : 0)", expected.neg());
    // type 1 (payments at the start) divides by 1 + r: 596.567686719
    assert_repr("pmt(0.005 : 360 : 100000 : 0 : 1)", n(596_567_686_719, 1_000_000_000));
    // with a future value: 1 %, 12 periods, 5000 borrowed, 1000 left owing: 365.395154713
    assert_repr("pmt(0.01 : 12 : 5000 : -1000 : 0)", n(365_395_154_713, 1_000_000_000));
}

#[test]
fn h3_ipmt_and_ppmt() {
    // fin.scm:34-46, :92-97.
    assert_repr("ipmt(0.005 : 1 : 360 : 100000 : 0 : 0)", n(500, 1));
    assert_repr("ppmt(0.005 : 1 : 360 : 100000 : 0 : 0)", n(995_505_251_528, 10_000_000_000));
    assert_repr("ipmt(0.005 : 360 : 360 : 100000 : 0 : 0)", n(298_283_843_360, 100_000_000_000));
    assert_repr("ppmt(0.005 : 360 : 360 : 100000 : 0 : 0)", n(596_567_686_719, 1_000_000_000));
    // per past nper is nper (fin.scm:37)
    assert_repr("ipmt(0.005 : 400 : 360 : 100000 : 0 : 0)", n(298_283_843_360, 100_000_000_000));
    // the loan assistant's formulas, with i bound (assistant-loan.cpp:2510-2528)
    let vars = bind(&[("i", n(1, 1))]);
    assert_eq!(
        eval("ppmt( 0.06 / 12 : i : 360 : 100000 : 0 : 0 )", &vars),
        Ok(n(995_505_251_528, 10_000_000_000))
    );
}

#[test]
fn h4_failing_calls() {
    // fin.scm:88-90: a zero rate is 0.0/0.0 in Guile, NaN, and the call fails (gnc-numeric.cpp:90-95).
    assert_eq!(function_error("pmt(0 : 12 : 1200 : 0 : 0)"), FunctionError::DivisionByZero);
    assert_eq!(function_error("ipmt(0 : 1 : 12 : 1200 : 0 : 0)"), FunctionError::DivisionByZero);
    assert_eq!(function_error("nosuch(1)"), FunctionError::Unknown);
    assert_eq!(function_error("foobar(\"abc\")"), FunctionError::Text);
    assert_eq!(
        financial("nosuch(1)").map_err(|e| e.to_string()),
        Err("function nosuch is not one of GnuCash's financial functions".into())
    );
}

#[test]
fn h5_other_functions() {
    // futureValue, fin.scm:56-64: 1000 at 5 % monthly for 10 years, 1647.00949769
    assert_repr("futureValue(1000 : 0.05 : 12 : 10)", n(164_700_949_769, 100_000_000));
    // computeInterestIncrement, fin.scm:66-68: 0.005 · 100000 · 1.005², 505.0125
    assert_repr("computeInterestIncrement(100000 : 0.06 : 12 : 3)", n(40401, 80));
    // periodic_rate, fin.scm:110-112, with a whole power: 0.005 and 1.005¹² − 1
    assert_repr("periodic_rate(0.06 : 12 : 12)", n(1, 200));
    assert_repr("periodic_rate(0.06 : 1 : 12)", n(123_355_623_729, 2_000_000_000_000));
    assert_repr("foobar(1 / 3)", n(333_333_333_333, 1_000_000_000_000));
    // The note's example: 5 % compounded semi-annually, paid monthly, is a sixth power: refused.
    assert_eq!(
        function_error("periodic_rate(0.05 : 12 : 2)"),
        FunctionError::NotWhole("ycomp / yper")
    );
    // cpd_* and amort_* round to the cent with Scheme's round on doubles (fin.scm:152-184, :300-366): refused, even
    // where the power is whole.
    [
        "cpd_pmt(0.06 : 12 : 12 : 360 : 100000 : 0 : 0)",
        "amort_ipmt(12 : 2 : 0.05 : 100000 : 4385 : 9 : 2)",
    ]
    .iter()
    .for_each(|t| assert!(matches!(function_error(t), FunctionError::Inexact(_)), "{t:?}"));
}

#[test]
fn powers_that_are_not_whole_are_refused() {
    assert_eq!(
        function_error("pmt(0.005 : 360.5 : 100000 : 0 : 0)"),
        FunctionError::NotWhole("nper")
    );
    assert_eq!(
        function_error("ipmt(0.005 : 1.5 : 360 : 100000 : 0 : 0)"),
        FunctionError::NotWhole("per")
    );
    assert_eq!(
        function_error("futureValue(1000 : 0.05 : 12 : 0.01)"),
        FunctionError::NotWhole("n × t")
    );
    assert_eq!(
        function_error("computeInterestIncrement(1 : 0.06 : 12 : 2.5)"),
        FunctionError::NotWhole("period")
    );
    // per past nper is not a power, so it need not be whole (fin.scm:37)
    assert_repr("ipmt(0.005 : 400.5 : 360 : 100000 : 0 : 0)", n(298_283_843_360, 100_000_000_000));
}

#[test]
fn arguments_and_ranges() {
    assert_eq!(function_error("pmt(0.005 : 360 : 100000 : 0)"), FunctionError::Arity(5));
    assert_eq!(function_error("pmt()"), FunctionError::Arity(5));
    assert!(matches!(
        function_error("pmt(0.005 : 1000000 : 100000 : 0 : 0)"),
        FunctionError::TooLarge(_)
    ));
    // 1.5^36000 is past a double's range, where Guile's infinity makes NaN.
    assert_eq!(function_error("pmt(0.5 : 36000 : 100000 : 0 : 0)"), FunctionError::Range);
    // Past 10^18, GncNumeric(double) refuses (gnc-numeric.cpp:89-95).
    assert_eq!(function_error("foobar(2000000000000000000)"), FunctionError::Range);
    // A negative number of periods is a negative power, still rational.
    assert!(financial("pmt(0.005 : -12 : 100000 : 0 : 0)").is_ok());
    // A rate of -100 % makes 1 + r zero; a negative power of it divides by zero.
    assert_eq!(function_error("pmt(-1 : -12 : 100000 : 0 : 0)"), FunctionError::DivisionByZero);
}

#[test]
fn the_largest_power_computes_in_reasonable_time() {
    // A century of daily periods with a rate of many digits.
    let start = std::time::Instant::now();
    assert!(financial("pmt(0.000123456789 : 36600 : 100000 : 0 : 0)").is_ok());
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "took {:?}", start.elapsed());
}
