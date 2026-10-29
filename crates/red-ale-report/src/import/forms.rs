//! The forms GnuCash writes into `saved-reports-2.8` (and `saved-reports-2.4`) and `stylesheets-2.0`, matched as
//! data. Each top-level form is one entry; a form that is not exactly the shape GnuCash writes is an error for that
//! entry, never something to run.
//!
//! A saved report configuration (`gnc:report-template-serialize-internal`,
//! `5.17:gnucash/report/report-core.scm:620-643`; the same since `2.6.21:src/report/report-system/report.scm:576-600`):
//!
//! ```scheme
//! (let ()
//!   (define (options-gen)
//!     (let ((options (gnc:report-template-new-options/report-guid "<template guid>" "<template name>"))
//!           (new-embedded-report-ids '()))
//!       <option forms>
//!       options))
//!   (gnc:define-report 'version 1 'name "<name>" 'report-guid "<guid>" 'parent-type "<template guid>"
//!     'options-generator options-gen 'menu-path (list gnc:menuname-custom)
//!     'renderer (gnc:report-template-renderer/report-guid "<template guid>" "<template name>")))
//! ```
//!
//! A Multicolumn View's entry (`gnc:report-serialize-embedded`, `5.17:gnucash/report/report-core.scm:567-618`) has
//! more inside `options-gen`. Among its option forms, `__general`/`report-list` holds the saving session's report
//! instance ids with each child's spans, `'((<id> <wide> <high> #f) …)`; after them comes one form per child, in
//! placement order, then one form that puts the new instances' ids in place of the saved ones, written even when
//! there are no children:
//!
//! ```scheme
//! (let ((options (gnc:report-template-new-options/report-guid "<template guid>" "<template name>")))
//!   <option forms>
//!   (set! new-embedded-report-ids
//!     (append new-embedded-report-ids
//!       (list (gnc:restore-report-by-guid-with-custom-template #f "<template guid>" "<template name>"
//!                                                              "<saved configuration guid, or empty>" options)))))
//! (let* ((option (gnc-lookup-option (gnc:optiondb options) "__general" "report-list"))
//!        (saved-report-list (GncOption-get-value option)))
//!   ((lambda (option)
//!      (if option ((gnc:option-setter option)
//!                  (map (lambda (x y) (cons x (cdr y))) new-embedded-report-ids saved-report-list))))
//!    option))
//! ```
//!
//! 3.x and 4.x wrote the last form with `(gnc:lookup-option options …)` and `(gnc:option-value option)`
//! (`4.14:gnucash/report/report-core.scm:586-600`). Only a view's own children are written: a child that is itself a
//! view keeps its report-list of instance ids among its option forms, and its own children are not saved.
//!
//! A style sheet (`gnc:save-style-sheet-options`, `5.17:gnucash/report/html-style-sheet.scm:145-166`):
//!
//! ```scheme
//! (let ((template (gnc:html-style-sheet-template-find "<template>")))
//!   (if template
//!     (let ((options ((gnc:html-style-sheet-template-options-generator template))))
//!       <option forms>
//!       (gnc:restore-html-style-sheet "<name>" "<template>" options))))
//! ```
//!
//! An option form (`gnc:generate-restore-forms`, `5.17:bindings/guile/options.scm:130-172`), with the setter 5.17
//! and 3.x/4.x write, or the one 2.x wrote (`2.6.21:src/app-utils/options.scm:192-197`):
//!
//! ```scheme
//! (let ((option (gnc:lookup-option options "<section>" "<name>")))
//!   ((lambda (o) (if o (gnc:option-set-value o <value>))) option))
//! (let ((option (gnc:lookup-option options "<section>" "<name>")))
//!   ((lambda (option) (if option ((gnc:option-setter option) <value>))) option))
//! ```
//!
//! The value is a literal: a string, a boolean, a number, or a quoted datum (`'(relative . today)`), or the budget
//! option's `(gnc-budget-lookup "<guid>" (gnc-get-current-book))` (`options.scm:150-160`), matched exactly and never
//! called. Anything else is refused.

use super::sexp::{Datum, Form};

/// One option an entry sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionForm {
    /// The section, GnuCash's English string.
    pub section: String,
    /// The option's name.
    pub name: String,
    /// The value, unquoted: `(relative . today)` for `'(relative . today)`.
    pub value: Datum,
}

/// A saved report configuration as GnuCash wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedReportForm {
    /// The line the entry starts on.
    pub line: u32,
    /// Its name (`'name`).
    pub name: String,
    /// Its GUID (`'report-guid`), as written.
    pub guid: String,
    /// The GUID of the template it configures (`'parent-type`), as written.
    pub template: String,
    /// The options it sets, or why they cannot be read. A Multicolumn View's report-list is not among them: it is read
    /// into `embedded`.
    pub options: Result<Vec<OptionForm>, String>,
    /// A Multicolumn View's children, in placement order; `None` for an entry that is not a view, or whose options
    /// cannot be read.
    pub embedded: Option<Vec<EmbeddedForm>>,
}

/// One child of a saved Multicolumn View: a copy of a report's options, as GnuCash makes one when the child is added
/// (`5.17:gnucash/report/report-core.scm:389-409`). The saved configuration it was copied from, if any, is not kept:
/// the options are complete without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedForm {
    /// The GUID of its report template, as written.
    pub template: String,
    /// The template's name, as written.
    pub template_name: String,
    /// The options it sets.
    pub options: Vec<OptionForm>,
    /// The columns it spans (GnuCash's `wide`).
    pub colspan: u32,
    /// The rows it spans (GnuCash's `high`).
    pub rowspan: u32,
}

/// A style sheet as GnuCash wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSheetForm {
    /// The line the entry starts on.
    pub line: u32,
    /// The style sheet's name.
    pub name: String,
    /// Its template's name.
    pub template: String,
    /// The options it sets, or why they cannot be read.
    pub options: Result<Vec<OptionForm>, String>,
}

/// A top-level form that is not an entry of the kind expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed {
    /// The line it starts on.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

type Matched<T> = Result<T, String>;

/// Match a top-level form as a saved report configuration.
pub fn saved_report(form: &Form) -> Result<SavedReportForm, Malformed> {
    let malformed = |message: String| Malformed { line: form.line, message };
    let [define_gen, define_report] = let_body(&form.datum, &[]).map_err(malformed)? else {
        return Err(malformed(
            "expected (let () (define (options-gen) …) (gnc:define-report …))".to_owned(),
        ));
    };
    let keys = define_report_keys(define_report).map_err(malformed)?;
    let text = |key: &str| {
        keys.iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, v)| v.as_str())
            .map(str::to_owned)
    };
    let (Some(name), Some(guid), Some(template)) = (text("name"), text("report-guid"), text("parent-type")) else {
        return Err(malformed(
            "gnc:define-report needs 'name, 'report-guid and 'parent-type strings".to_owned(),
        ));
    };
    let (options, embedded) = match options_gen(define_gen, &template) {
        Ok((options, embedded)) => (Ok(options), embedded),
        Err(message) => (Err(message), None),
    };
    Ok(SavedReportForm {
        line: form.line,
        name,
        guid,
        template,
        options,
        embedded,
    })
}

/// Match a top-level form as a style sheet.
pub fn style_sheet(form: &Form) -> Result<StyleSheetForm, Malformed> {
    let malformed = |message: String| Malformed { line: form.line, message };
    let shape = || "expected (let ((template (gnc:html-style-sheet-template-find …))) (if template …))".to_owned();
    let (bindings, body) = let_parts(&form.datum).ok_or_else(|| malformed(shape()))?;
    let template = match (bindings, body) {
        ([binding], [_]) => binding_call(binding, "template", "gnc:html-style-sheet-template-find").and_then(one_string),
        _ => None,
    };
    let template = template.ok_or_else(|| malformed(shape()))?;
    let restored = body.first().and_then(|b| style_sheet_body(b, &template));
    let (name, options) = restored.ok_or_else(|| malformed(shape()))?;
    Ok(StyleSheetForm {
        line: form.line,
        name,
        template,
        options,
    })
}

/// The `(if template (let ((options …)) <forms> (gnc:restore-html-style-sheet "<name>" "<template>" options)))`
/// part: the name, and the option forms.
fn style_sheet_body(body: &Datum, template: &str) -> Option<(String, Matched<Vec<OptionForm>>)> {
    let [iff, test, inner] = body.as_list()? else { return None };
    if !iff.is_symbol("if") || !test.is_symbol("template") {
        return None;
    }
    let (bindings, forms) = let_parts(inner)?;
    let [binding] = bindings else { return None };
    let [var, call] = binding.as_list()? else { return None };
    let [generator] = call.as_list()? else { return None };
    let template_options = var.is_symbol("options")
        && generator.as_list().is_some_and(
            |c| matches!(c, [f, t] if f.is_symbol("gnc:html-style-sheet-template-options-generator") && t.is_symbol("template")),
        );
    let (restore, option_forms) = forms.split_last()?;
    let restore = restore.as_list()?;
    let [f, name, again, options] = restore else { return None };
    let restores = f.is_symbol("gnc:restore-html-style-sheet") && options.is_symbol("options") && again.as_str() == Some(template);
    if !(template_options && restores) {
        return None;
    }
    Some((name.as_str()?.to_owned(), option_list(option_forms)))
}

/// The forms after `(let <bindings>` when the bindings are exactly `expected` (here only `()`).
fn let_body<'a>(d: &'a Datum, expected: &[Datum]) -> Matched<&'a [Datum]> {
    let (bindings, body) = let_parts(d).ok_or("expected a let form")?;
    if bindings == expected {
        Ok(body)
    } else {
        Err("unexpected let bindings".to_owned())
    }
}

/// The bindings and body of `(let (<bindings>) <body>…)`.
fn let_parts(d: &Datum) -> Option<(&[Datum], &[Datum])> {
    let [l, bindings, body @ ..] = d.as_list()? else { return None };
    l.is_symbol("let").then_some(())?;
    Some((bindings.as_list()?, body))
}

/// The arguments of the call in a binding `(<var> (<function> <args>…))`.
fn binding_call<'a>(binding: &'a Datum, var: &str, function: &str) -> Option<&'a [Datum]> {
    let [v, call] = binding.as_list()? else { return None };
    let [f, args @ ..] = call.as_list()? else { return None };
    (v.is_symbol(var) && f.is_symbol(function)).then_some(args)
}

fn one_string(args: &[Datum]) -> Option<String> {
    match args {
        [s] => s.as_str().map(str::to_owned),
        _ => None,
    }
}

/// The keyword arguments of `(gnc:define-report 'key value …)`, every key one GnuCash writes.
fn define_report_keys(d: &Datum) -> Matched<Vec<(&str, &Datum)>> {
    const KEYS: [&str; 7] = [
        "version",
        "name",
        "report-guid",
        "parent-type",
        "options-generator",
        "menu-path",
        "renderer",
    ];
    let [f, args @ ..] = d.as_list().ok_or("expected (gnc:define-report …)")? else {
        return Err("expected (gnc:define-report …)".to_owned());
    };
    if !f.is_symbol("gnc:define-report") || args.len() % 2 != 0 {
        return Err("expected (gnc:define-report 'key value …)".to_owned());
    }
    args.chunks(2)
        .map(|kv| {
            let key = kv
                .first()
                .and_then(Datum::unquoted)
                .and_then(Datum::as_symbol)
                .ok_or("a gnc:define-report key is a quoted symbol")?;
            let value = kv.get(1).ok_or("a gnc:define-report key needs a value")?;
            if KEYS.contains(&key) {
                Ok((key, value))
            } else {
                Err(format!("unknown gnc:define-report key '{key}"))
            }
        })
        .collect()
}

/// The option forms, and a view's children.
type Body = (Vec<OptionForm>, Option<Vec<EmbeddedForm>>);

/// `(define (options-gen) (let ((options (gnc:report-template-new-options/report-guid "<template>" "<name>")) …)
/// <option forms> [<embedded report forms> <update form>] options))`.
fn options_gen(d: &Datum, template: &str) -> Matched<Body> {
    let shape = || "expected (define (options-gen) (let ((options …)) … options))".to_owned();
    let [define, head, body] = d.as_list().ok_or_else(shape)? else {
        return Err(shape());
    };
    let names_options_gen = head.as_list().is_some_and(|h| matches!(h, [n] if n.is_symbol("options-gen")));
    if !define.is_symbol("define") || !names_options_gen {
        return Err(shape());
    }
    let (bindings, forms) = let_parts(body).ok_or_else(shape)?;
    check_bindings(bindings, template)?;
    match forms.split_last() {
        Some((last, body)) if last.is_symbol("options") => options_body(body),
        _ => Err(shape()),
    }
}

/// A Multicolumn View's children option (`5.17:gnucash/report/reports/standard/view-column.scm:42`).
const REPORT_LIST: (&str, &str) = ("__general", "report-list");

const NEW_OPTIONS: &str = "gnc:report-template-new-options/report-guid";

/// The forms of `options-gen`'s body: option forms and, for a view, its embedded reports and the update form. Each
/// child takes its spans from the report-list at its position.
fn options_body(forms: &[Datum]) -> Matched<Body> {
    let Some((_, forms)) = forms.split_last().filter(|(last, _)| is_update_form(last)) else {
        return Ok((option_list(forms)?, None));
    };
    let at = forms.iter().position(is_embedded_form).unwrap_or(forms.len());
    let (option_forms, embedded_forms) = forms.split_at(at);
    let mut options = option_list(option_forms)?;
    let embedded = embedded_forms.iter().map(embedded_form).collect::<Matched<Vec<_>>>()?;
    let spans = match options.iter().position(|o| (o.section.as_str(), o.name.as_str()) == REPORT_LIST) {
        Some(i) => report_list_spans(&options.remove(i).value)?,
        None => Vec::new(),
    };
    if spans.len() != embedded.len() {
        return Err(format!(
            "the report-list places {} reports, and {} embedded reports follow it",
            spans.len(),
            embedded.len()
        ));
    }
    let children = embedded
        .into_iter()
        .zip(spans)
        .map(|(child, (colspan, rowspan))| EmbeddedForm { colspan, rowspan, ..child })
        .collect();
    Ok((options, Some(children)))
}

/// The form that puts the restored children's ids in the view's report-list, 5.17's or 3.x and 4.x's, matched whole.
fn is_update_form(d: &Datum) -> bool {
    const LOOKUPS: [(&str, &str); 2] = [
        (
            r#"(gnc-lookup-option (gnc:optiondb options) "__general" "report-list")"#,
            "(GncOption-get-value option)",
        ),
        (r#"(gnc:lookup-option options "__general" "report-list")"#, "(gnc:option-value option)"),
    ];
    LOOKUPS.iter().any(|(lookup, value)| {
        let text = format!(
            "(let* ((option {lookup}) (saved-report-list {value})) ((lambda (option) (if option ((gnc:option-setter option) \
             (map (lambda (x y) (cons x (cdr y))) new-embedded-report-ids saved-report-list)))) option))"
        );
        super::sexp::read_one(&text).is_ok_and(|expected| expected == *d)
    })
}

/// Whether `d` binds `options` as an embedded report's form does, where an option form binds `option`.
fn is_embedded_form(d: &Datum) -> bool {
    let_parts(d).is_some_and(|(bindings, _)| matches!(bindings, [b] if binding_call(b, "options", NEW_OPTIONS).is_some()))
}

/// One embedded report's form, its spans not yet known.
fn embedded_form(d: &Datum) -> Matched<EmbeddedForm> {
    let shape = || format!("not an embedded report form GnuCash writes: {}", short(d));
    let (bindings, body) = let_parts(d).ok_or_else(shape)?;
    let new_options = match bindings {
        [b] => binding_call(b, "options", NEW_OPTIONS),
        _ => None,
    };
    let Some([template, name]) = new_options else { return Err(shape()) };
    let (Some(template), Some(name)) = (template.as_str(), name.as_str()) else {
        return Err(shape());
    };
    let Some((_, option_forms)) = body.split_last().filter(|(restore, _)| restores(restore, template, name)) else {
        return Err(shape());
    };
    Ok(EmbeddedForm {
        template: template.to_owned(),
        template_name: name.to_owned(),
        options: option_list(option_forms)?,
        colspan: 0,
        rowspan: 0,
    })
}

/// `(set! new-embedded-report-ids (append new-embedded-report-ids (list (gnc:restore-report-by-guid-with-custom-template
/// #f "<template>" "<name>" "<custom template>" options))))`.
fn restores(d: &Datum, template: &str, name: &str) -> bool {
    const IDS: &str = "new-embedded-report-ids";
    let restore = |d: &Datum| match d.as_list() {
        Some([f, id, t, n, custom, options]) => {
            f.is_symbol("gnc:restore-report-by-guid-with-custom-template")
                && *id == Datum::Bool(false)
                && t.as_str() == Some(template)
                && n.as_str() == Some(name)
                && custom.as_str().is_some()
                && options.is_symbol("options")
        }
        _ => false,
    };
    let listed = |d: &Datum| matches!(d.as_list(), Some([l, r]) if l.is_symbol("list") && restore(r));
    let appended = |d: &Datum| matches!(d.as_list(), Some([a, ids, l]) if a.is_symbol("append") && ids.is_symbol(IDS) && listed(l));
    matches!(d.as_list(), Some([set, ids, a]) if set.is_symbol("set!") && ids.is_symbol(IDS) && appended(a))
}

/// The `(wide, high)` of each `(<id> <wide> <high> #f)` of a saved report-list (`5.17:bindings/guile/gnc-optiondb.i:304-321`).
fn report_list_spans(d: &Datum) -> Matched<Vec<(u32, u32)>> {
    let count = |d: &Datum| match d {
        Datum::Number(n) if n.is_integer() => n.to_string().parse::<u32>().ok(),
        _ => None,
    };
    let span = |item: &Datum| match item.as_list() {
        Some([id, wide, high, rest @ ..]) if count(id).is_some() && matches!(rest, [] | [Datum::Bool(false)]) => count(wide).zip(count(high)),
        _ => None,
    };
    let items = d
        .as_list()
        .ok_or_else(|| format!("report-list: expected ((<id> <wide> <high> #f) …), got {}", short(d)))?;
    items
        .iter()
        .map(|item| span(item).ok_or_else(|| format!("report-list: not a placement (<id> <wide> <high> #f): {}", short(item))))
        .collect()
}

/// `((options (gnc:report-template-new-options/report-guid "<template>" "<name>")) (new-embedded-report-ids '()))`,
/// the second binding written from 2.8 on.
fn check_bindings(bindings: &[Datum], template: &str) -> Matched<()> {
    let options = bindings
        .first()
        .and_then(|b| binding_call(b, "options", "gnc:report-template-new-options/report-guid"));
    let options_ok = matches!(options, Some([t, n]) if t.as_str() == Some(template) && n.as_str().is_some());
    let embedded_ok = match bindings.get(1..) {
        Some([]) => true,
        Some([b]) => {
            let empty = |a: &[Datum]| matches!(a, [Datum::List(l, None)] if l.is_empty());
            binding_call(b, "new-embedded-report-ids", "quote").is_some_and(empty)
        }
        _ => false,
    };
    if options_ok && embedded_ok {
        Ok(())
    } else {
        Err(format!(
            "expected (options (gnc:report-template-new-options/report-guid {template:?} …)) as the only option bindings"
        ))
    }
}

/// Every form must be an option form.
fn option_list(forms: &[Datum]) -> Matched<Vec<OptionForm>> {
    forms.iter().map(option_form).collect()
}

/// One option form, with either setter.
fn option_form(d: &Datum) -> Matched<OptionForm> {
    let shape = || format!("not an option form GnuCash writes: {}", short(d));
    let (bindings, body) = let_parts(d).ok_or_else(shape)?;
    let [binding] = bindings else { return Err(shape()) };
    let lookup = binding_call(binding, "option", "gnc:lookup-option").ok_or_else(shape)?;
    let [opts, section, name] = lookup else { return Err(shape()) };
    let (Some(section), Some(name)) = (section.as_str(), name.as_str()) else {
        return Err(shape());
    };
    if !opts.is_symbol("options") {
        return Err(shape());
    }
    let [call] = body else { return Err(shape()) };
    let [setter, arg] = call.as_list().ok_or_else(shape)? else {
        return Err(shape());
    };
    if !arg.is_symbol("option") {
        return Err(shape());
    }
    let value = setter_value(setter).ok_or_else(shape)?;
    let value = literal(value).ok_or_else(|| format!("option {section}/{name}: {} is not a literal value", short(value)))?;
    Ok(OptionForm {
        section: section.to_owned(),
        name: name.to_owned(),
        value: value.clone(),
    })
}

/// The value form inside `(lambda (o) (if o (gnc:option-set-value o <value>)))` or
/// `(lambda (option) (if option ((gnc:option-setter option) <value>)))`.
fn setter_value(setter: &Datum) -> Option<&Datum> {
    let [lambda, params, body] = setter.as_list()? else { return None };
    let [param] = params.as_list()? else { return None };
    let param = param.as_symbol()?;
    let [iff, test, call] = body.as_list()? else { return None };
    if !lambda.is_symbol("lambda") || !iff.is_symbol("if") || !test.is_symbol(param) {
        return None;
    }
    match call.as_list()? {
        [f, o, value] if f.is_symbol("gnc:option-set-value") && o.is_symbol(param) => Some(value),
        [f, value] => {
            let getter = f.as_list()?;
            matches!(getter, [g, o] if g.is_symbol("gnc:option-setter") && o.is_symbol(param)).then_some(value)
        }
        _ => None,
    }
}

/// A literal value: a string, boolean or number as itself, the datum a quote holds, or a budget lookup as itself. Any
/// other symbol or unquoted list would be a variable or a call: refused.
fn literal(value: &Datum) -> Option<&Datum> {
    match value {
        Datum::String(_) | Datum::Bool(_) | Datum::Number(_) => Some(value),
        _ if budget_lookup(value).is_some() => Some(value),
        _ => value.unquoted(),
    }
}

/// The GUID of `(gnc-budget-lookup "<guid>" (gnc-get-current-book))`, the value GnuCash saves for a budget option
/// (`5.17:bindings/guile/options.scm:150-160`).
pub(super) fn budget_lookup(value: &Datum) -> Option<&str> {
    let [lookup, guid, book] = value.as_list()? else { return None };
    let current_book = book
        .as_list()
        .is_some_and(|b| matches!(b, [f] if f.is_symbol("gnc-get-current-book")));
    (lookup.is_symbol("gnc-budget-lookup") && current_book)
        .then(|| guid.as_str())
        .flatten()
}

/// A datum's text, cut short for a message.
fn short(d: &Datum) -> String {
    let text = d.to_string();
    match text.char_indices().nth(80) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::super::sexp::read_all;
    use super::*;

    fn first(text: &str) -> Form {
        read_all(text).forms.remove(0)
    }

    /// What `gnc:report-template-serialize-internal` writes (5.17:gnucash/report/report-core.scm:620-643).
    const SAVED: &str = r#"
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Options for saved report "My Balance Sheet", based on template "c4173ac99b2b448289bf4d11c731af13"
(let ()
  (define (options-gen)
    (let
         (
           (options (gnc:report-template-new-options/report-guid "c4173ac99b2b448289bf4d11c731af13" "Balance Sheet"))
           (new-embedded-report-ids '()) ;; only used with Multicolumn View Reports
         )
; Section: Accounts

(let ((option (gnc:lookup-option options
                                 "Accounts"
                                 "Levels of Subaccounts")))
  ((lambda (o) (if o (gnc:option-set-value o '2))) option))


; Section: General

(let ((option (gnc:lookup-option options
                                 "General"
                                 "Balance Sheet Date")))
  ((lambda (o) (if o (gnc:option-set-value o '(relative . end-prev-fin-year)))) option))


      options
    )
  )
  (gnc:define-report
    'version 1
    'name "My Balance Sheet"
    'report-guid "0f9d3b6a1c2e4d5f8a7b6c5d4e3f2a1b"
    'parent-type "c4173ac99b2b448289bf4d11c731af13"
    'options-generator options-gen
    'menu-path (list gnc:menuname-custom)
    'renderer (gnc:report-template-renderer/report-guid "c4173ac99b2b448289bf4d11c731af13" "Balance Sheet")
  )
)
"#;

    #[test]
    fn reads_a_saved_report() {
        let entry = saved_report(&first(SAVED)).unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.guid.as_str()),
            ("My Balance Sheet", "0f9d3b6a1c2e4d5f8a7b6c5d4e3f2a1b")
        );
        assert_eq!(entry.template, "c4173ac99b2b448289bf4d11c731af13");
        assert_eq!(entry.line, 4);
        let options = entry.options.unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(
            (options[0].section.as_str(), options[0].name.as_str()),
            ("Accounts", "Levels of Subaccounts")
        );
        assert_eq!(options[1].value.to_string(), "(relative . end-prev-fin-year)");
    }

    #[test]
    fn reads_the_2_x_setter() {
        let setter = "(lambda (option) (if option ((gnc:option-setter option) '2)))";
        let old = SAVED.replace("(lambda (o) (if o (gnc:option-set-value o '2)))", setter);
        let options = saved_report(&first(&old)).unwrap().options.unwrap();
        assert_eq!(options[0].value.to_string(), "2");
    }

    #[test]
    fn refuses_calls_in_values() {
        let evil = SAVED.replace("'(relative . end-prev-fin-year)", "(system \"rm -rf ~\")");
        let entry = saved_report(&first(&evil)).unwrap();
        assert!(entry.options.unwrap_err().contains("not a literal value"));
        let symbol = SAVED.replace("'2", "two");
        assert!(saved_report(&first(&symbol)).unwrap().options.is_err());
        let extra = SAVED.replace("      options\n    )", "      (display \"hi\")\n      options\n    )");
        assert!(saved_report(&first(&extra)).unwrap().options.is_err());
    }

    /// The budget option's form, as `gnc:generate-restore-forms` writes it (5.17:bindings/guile/options.scm:150-160).
    #[test]
    fn reads_a_budget_lookup_as_data() {
        let lookup = "(gnc-budget-lookup \"0123456789abcdef0123456789abcdef\" (gnc-get-current-book))";
        let setter = format!("(lambda (option) (if option ((gnc:option-setter option) {lookup})))");
        let budget = SAVED.replace("(lambda (o) (if o (gnc:option-set-value o '2)))", &setter);
        let options = saved_report(&first(&budget)).unwrap().options.unwrap();
        assert_eq!(budget_lookup(&options[0].value), Some("0123456789abcdef0123456789abcdef"));
        let other_book = budget.replace("(gnc-get-current-book)", "(gnc-get-other-book)");
        assert!(
            saved_report(&first(&other_book))
                .unwrap()
                .options
                .unwrap_err()
                .contains("not a literal value")
        );
    }

    #[test]
    fn keeps_the_head_when_the_body_is_malformed() {
        let odd = SAVED.replace("(new-embedded-report-ids '())", "(new-embedded-report-ids (launch))");
        let entry = saved_report(&first(&odd)).unwrap();
        assert_eq!(entry.template, "c4173ac99b2b448289bf4d11c731af13");
        assert!(entry.options.is_err());
    }

    #[test]
    fn refuses_other_top_level_forms() {
        assert!(saved_report(&first("(define x 1)")).is_err());
        assert!(saved_report(&first("(let () (define (options-gen) options))")).is_err());
        let unknown_key = SAVED.replace("'version 1", "'evil 1");
        assert!(saved_report(&first(&unknown_key)).is_err());
    }

    /// The fixture's saved Multicolumn View, written as `gnc:report-serialize-embedded` writes one
    /// (5.17:gnucash/report/report-core.scm:567-618).
    fn view(edit: impl Fn(String) -> String) -> SavedReportForm {
        let text = include_str!("../../../../fixtures/gnucash-userdata/saved-reports-2.8");
        let at = text.find(";; Options for saved report \"Board pack\"").unwrap();
        saved_report(&first(&edit(text[at..].to_owned()))).unwrap()
    }

    #[test]
    fn reads_a_saved_view_and_its_embedded_reports() {
        let entry = view(|t| t);
        let names = |o: &[OptionForm]| o.iter().map(|o| format!("{}/{}", o.section, o.name)).collect::<Vec<_>>();
        assert_eq!(names(entry.options.as_ref().unwrap()), ["General/Report name", "General/Number of columns"]);
        let children = entry.embedded.unwrap();
        let shape = |c: &EmbeddedForm| (c.template.clone(), c.template_name.clone(), c.colspan, c.rowspan, c.options.len());
        assert_eq!(
            children.iter().map(shape).collect::<Vec<_>>(),
            [
                ("3298541c236b494998b236dfad6ad752".to_owned(), "Account Summary".to_owned(), 1, 1, 3),
                ("c4173ac99b2b448289bf4d11c731af13".to_owned(), "Balance Sheet".to_owned(), 1, 2, 2),
                ("d8ba4a2e89e8479ca9f6eccdeb164588".to_owned(), "Multicolumn View".to_owned(), 2, 1, 2),
            ]
        );
        assert_eq!(
            names(&children[2].options),
            ["General/Number of columns", "__general/report-list"],
            "a nested view's report-list stays an option"
        );
        assert_eq!(saved_report(&first(SAVED)).unwrap().embedded, None, "not a view");
    }

    #[test]
    fn reads_the_4_x_update_form() {
        let old = view(|t| {
            t.replace("(gnc-lookup-option (gnc:optiondb options) \"__general\"", "(gnc:lookup-option options \"__general\"")
                .replace("(GncOption-get-value option)", "(gnc:option-value option)")
        });
        assert_eq!(old.embedded.map(|c| c.len()), Some(3));
    }

    #[test]
    fn refuses_embedded_reports_not_as_gnucash_writes_them() {
        let refused = |from: &str, to: &str| view(|t| t.replacen(from, to, 1)).options.unwrap_err();
        assert!(refused("(13 1 2 #f) ", "").contains("places 2 reports, and 3 embedded reports"));
        assert!(refused("(14 2 1 #f)", "(14 2 1 x)").contains("not a placement"));
        assert!(refused("(14 2 1 #f)", "(14 2.5 1 #f)").contains("not a placement"));
        assert!(refused("'2)", "(launch))").contains("not a literal value"), "an embedded option's value");
        let other_template = "#f \"c4173ac99b2b448289bf4d11c731af13\" \"Income Statement\"";
        assert!(refused("#f \"c4173ac99b2b448289bf4d11c731af13\" \"Balance Sheet\"", other_template).contains("not an embedded report form"));
        assert!(refused("(set! new-embedded-report-ids", "(set! other-ids").contains("not an embedded report form"));
        let update = "(map (lambda (x y) (cons x (cdr y))) new-embedded-report-ids saved-report-list)";
        let wrong = refused(update, "(launch)");
        assert!(wrong.contains("not an option form"), "without the update form, nothing is a view's: {wrong}");
    }

    /// What `gnc:save-style-sheet-options` writes (5.17:gnucash/report/html-style-sheet.scm:145-166).
    const SHEET: &str = r#"(let ((template (gnc:html-style-sheet-template-find "Easy")))
  (if template
    (let ((options ((gnc:html-style-sheet-template-options-generator template))))

; Section: General

(let ((option (gnc:lookup-option options
                                 "General"
                                 "Preparer")))
  ((lambda (o) (if o (gnc:option-set-value o "A. Bookkeeper"))) option))

 (gnc:restore-html-style-sheet "My Easy" "Easy" options))))
"#;

    #[test]
    fn reads_a_style_sheet() {
        let sheet = style_sheet(&first(SHEET)).unwrap();
        assert_eq!((sheet.name.as_str(), sheet.template.as_str()), ("My Easy", "Easy"));
        let options = sheet.options.unwrap();
        assert_eq!(options[0].value, Datum::String("A. Bookkeeper".to_owned()));
    }

    #[test]
    fn style_sheet_template_names_must_agree() {
        let mismatch = SHEET.replace("\"My Easy\" \"Easy\"", "\"My Easy\" \"Plain\"");
        assert!(style_sheet(&first(&mismatch)).is_err());
        assert!(style_sheet(&first("(let ((template 1)) template)")).is_err());
    }
}
