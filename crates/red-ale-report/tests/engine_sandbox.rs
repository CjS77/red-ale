//! The engine's sandbox (`design/reports.md` §13, "Properties and safety"): no floats, no clock, no `eval`, imports
//! only of known scripts, `print` to the log, and every limit raising `report-limit` with its name.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{EMPTY_RESULT, GUID, Host, empty_book, eval, limit_of, registry, registry_with, render, template, template_with};
use red_ale_report::ReportError;
use red_ale_report::library::Library;
use red_ale_report::limits::{Limit, Limits};
use red_ale_report::result::LogLevel;
use serde_json::json;

fn load_problem(script: &str) -> ReportError {
    let reg = registry(&[("bad.rhai", script)]);
    assert!(reg.templates().is_empty());
    reg.problems().first().cloned().expect("the script does not load")
}

#[test]
fn a_float_literal_does_not_compile() {
    let e = load_problem(&template("let x = 1.5;\n#{ title: \"t\", sections: [] }"));
    assert!(matches!(&e, ReportError::Load(l) if l.source == "./bad.rhai" && l.line == Some(4)), "{e:?}");
}

#[test]
fn there_is_no_clock_eval_or_sleep() {
    let e = render(&format!("let t = timestamp();\n{EMPTY_RESULT}")).unwrap_err();
    assert!(matches!(&e, ReportError::Script(s) if s.message.contains("timestamp")), "{e:?}");
    assert!(matches!(load_problem(&template(&format!("eval(\"1\");\n{EMPTY_RESULT}"))), ReportError::Load(_)));
    let e = render(&format!("sleep(100);\n{EMPTY_RESULT}")).unwrap_err();
    assert!(matches!(&e, ReportError::Script(s) if s.message.contains("sleep is not available")), "{e:?}");
}

#[test]
fn imports_reach_only_known_scripts() {
    let library = Library::from_sources([("lib/helper.rhai", "fn twice(x) { x * 2 }")]);
    let importer = |path: &str| template(&format!("import \"{path}\" as m;\n#{{ title: `${{m::twice(2)}}`, sections: [] }}"));
    let run = |path: &str, extra: &[(&str, &str)]| {
        let script = importer(path);
        let mut scripts = vec![("mine/report.rhai", script.as_str())];
        scripts.extend_from_slice(extra);
        let reg = registry_with(library.clone(), &scripts, Limits::default());
        Host::new(empty_book()).run(&reg, GUID, &json!({}))
    };
    assert_eq!(run("lib/helper", &[]).unwrap().title, "4");
    assert_eq!(run("./util", &[("mine/util.rhai", "fn twice(x) { x + x }")]).unwrap().title, "4");
    let refused = ["/etc/passwd", "../outside", "./../outside", "lib/missing", "./sub/../util", "mine/util"];
    refused.iter().for_each(|path| {
        let e = run(path, &[("mine/util.rhai", "fn twice(x) { x }"), ("outside.rhai", "fn twice(x) { x }")]).unwrap_err();
        assert!(matches!(&e, ReportError::Script(s) if s.message.contains("Module not found")), "{path}: {e:?}");
    });
}

#[test]
fn a_built_in_script_cannot_import_user_scripts() {
    let library = Library::from_sources([("lib/helper.rhai", "import \"./mine\" as m;\nfn twice(x) { m::f(x) }")]);
    let script = template("import \"lib/helper\" as h;\n#{ title: `${h::twice(1)}`, sections: [] }");
    let reg = registry_with(library, &[("x.rhai", &script), ("mine.rhai", "fn f(x) { x }")], Limits::default());
    let e = Host::new(empty_book()).run(&reg, GUID, &json!({})).unwrap_err();
    assert!(matches!(&e, ReportError::Script(s) if s.message.contains("Module not found") && s.source == "lib/helper.rhai"), "{e:?}");
}

#[test]
fn print_and_debug_go_to_the_log() {
    let r = render(&format!("print(\"hello\");\ndebug(42);\n{EMPTY_RESULT}")).unwrap();
    assert_eq!(r.log.len(), 2);
    assert_eq!((r.log[0].level, r.log[0].text.as_str()), (LogLevel::Print, "hello"));
    assert_eq!((r.log[1].level, r.log[1].text.as_str(), r.log[1].line), (LogLevel::Debug, "42", Some(5)));
    assert_eq!(r.log[1].source.as_deref(), Some("./test.rhai"));
}

#[test]
fn a_failing_script_says_where() {
    let e = render("throw \"no accounts\";").unwrap_err();
    let ReportError::Script(s) = &e else { panic!("{e:?}") };
    assert_eq!((s.report.as_str(), s.source.as_str(), s.line, s.message.as_str()), (GUID, "./test.rhai", Some(4), "no accounts"));
    assert_eq!(e.kind().code(), 3000);
    let e = render("let x = undefined_function(1);").unwrap_err();
    assert!(matches!(&e, ReportError::Script(s) if s.message.contains("undefined_function")), "{e:?}");
    assert!(matches!(load_problem(&template("let y = not_declared;")), ReportError::Load(_)), "strict variables");
}

/// Run `body` under `limits`; the `data.limit` it fails with.
fn limit(limits: Limits, body: &str) -> Option<String> {
    let (prelude, body) = body.split_once("\n%%\n").unwrap_or(("", body));
    let script = template_with(prelude, body);
    let reg = registry_with(Library::from_sources([]), &[("test.rhai", &script)], limits);
    let outcome = match reg.problems().first() {
        Some(problem) => Err(problem.clone()),
        None => Host::new(empty_book()).run(&reg, GUID, &json!({})),
    };
    let e = outcome.expect_err("the limit stops the script");
    assert_eq!(e.kind().code(), 3001, "{e:?}");
    limit_of(&e)
}

#[test]
fn every_limit_raises_report_limit_with_its_name() {
    // A stuck case fails on the 10-second time limit rather than hanging the test run.
    let small = |l: Limit, max: u64| Limits::default().with(Limit::Time, 10).with(l, max);
    let cases: Vec<(Limit, Limits, String)> = vec![
        (Limit::Operations, small(Limit::Operations, 1000), "let i = 0; loop { i += 1; }".into()),
        (Limit::Time, small(Limit::Time, 0), "let i = 0; loop { i += 1; }".into()),
        (Limit::CallDepth, small(Limit::CallDepth, 8), "fn deep(n) { deep(n + 1) }\n%%\ndeep(0)".into()),
        (Limit::ExpressionDepth, small(Limit::ExpressionDepth, 8), format!("let x = {}1{};", "(".repeat(20), ")".repeat(20))),
        (Limit::StringSize, small(Limit::StringSize, 100), "let s = \"x\"; loop { s += s; }".into()),
        (Limit::ArraySize, small(Limit::ArraySize, 100), "let a = []; loop { a.push(1); }".into()),
        (Limit::MapSize, small(Limit::MapSize, 10),
         "let m = #{}; let i = 0; loop { let one = #{}; one[`k${i}`] = i; m += one; i += 1; }".into()),
        (Limit::Modules, small(Limit::Modules, 0), "import \"./other\" as o;".into()),
        (Limit::Variables, small(Limit::Variables, 5),
         "let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; let f = 6; let g = 7;".into()),
        (Limit::Functions, small(Limit::Functions, 2), "1".into()),
        (Limit::NumberSize, small(Limit::NumberSize, 64), "let r = rational(2); loop { r = r * r; }".into()),
    ];
    assert_eq!(cases.len(), Limit::all().count());
    cases.into_iter().for_each(|(l, limits, body)| assert_eq!(limit(limits, &body).as_deref(), Some(l.as_str()), "{l}"));
}

#[test]
fn a_script_cannot_catch_a_limit() {
    let body =
        "fn grow() { let r = rational(2); loop { r = r * r; } }\n%%\ntry { grow(); } catch (e) { }\n#{ title: \"caught\", sections: [] }";
    assert_eq!(limit(Limits::default().with(Limit::NumberSize, 64), body).as_deref(), Some("number-size"));
}

#[test]
fn rationals_are_exact_and_mix_with_integers() {
    assert_eq!(eval("rational(1, 3) + rational(1, 6)").unwrap(), "1/2");
    assert_eq!(eval("rational(\"12.34\") * 100").unwrap(), "1234");
    assert_eq!(eval("2 - rational(\"0.5\")").unwrap(), "3/2");
    assert_eq!(eval("rational(\"2.5\").round()").unwrap(), "2");
    assert_eq!(eval("rational(\"0.125\").round_to(100, \"round\")").unwrap(), "3/25");
    assert_eq!(eval("rational(\"0.125\").round_to(100, \"half-up\").to_decimal(2)").unwrap(), "0.13");
    assert_eq!(eval("fixed(rational(1, 2), 2, 1)").unwrap(), ".5");
    assert_eq!(eval("fixed(rational(1, 2), (), 1)").unwrap(), "0.5");
    assert_eq!(eval("rational(-3, 4).abs() > 0 && rational(1, 2) == rational(2, 4) && rational(3) >= 3").unwrap(), "true");
    assert_eq!(eval("[rational(1, 2), rational(1, 3)].contains(rational(2, 6))").unwrap(), "true");
    assert_eq!(eval("let r = rational(6, 4); `${r.num()}/${r.denom()} ${r.sign()} ${r.is_integer()}`").unwrap(), "3/2 1 false");
    assert_eq!(eval("max(rational(1, 2), rational(2, 3))").unwrap(), "2/3");
    assert!(matches!(eval("rational(1) / 0"), Err(ReportError::Script(s)) if s.message.contains("division by zero")));
}

#[test]
fn monetaries_add_only_one_commodity() {
    assert_eq!(eval("(monetary(\"USD\", rational(\"1.25\")) + monetary(\"CURRENCY::USD\", 2)).to_string()").unwrap(), "$3.25");
    assert_eq!(eval("(-monetary(\"USD\", rational(\"1234.505\"))).round_scu().to_string()").unwrap(), "-$1,234.50");
    assert_eq!(eval("monetary(\"USD\", 1).commodity.mnemonic").unwrap(), "USD");
    assert!(matches!(eval("monetary(\"USD\", 1) + monetary(\"GBP\", 1)"), Err(ReportError::Script(s)) if s.message.contains("combine")));
    assert!(matches!(eval("monetary(\"XYZ::NOPE\", 1)"), Err(ReportError::Script(s)) if s.message.contains("unknown commodity")));
}
