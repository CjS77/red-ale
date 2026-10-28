//! `lib/trep-engine`, first half (`5.17:gnucash/report/trep-engine.scm`, TE): the options, the parameters, the
//! warnings before the query, the split query, the split filter and the custom sort, run through a test template on
//! `fixtures/reports/transaction.gnucash`, the book of GnuCash's `test-transaction.scm` (TT;
//! `design/notes/report-transaction-engine.md` §4). Where `fixtures/reports/transaction.expected.json` holds a check
//! the splits decide before the table is built (the grand total of one amount column, which splits a column lists and
//! in what order, no table at all), the test reads it from there; the rest are worked by hand from TE.
//!
//! Bank's splits in 1969-1970, in split order: trn2 (31 Dec 1969, +10 from Income, void, notes3), trn3 (31 Dec 1969,
//! +29 from Income, cleared 1 Mar 1970), trn1 (1 Jan 1970, -5 to Expenses, memo-3), trn4 (1 Feb, -15 to Expenses,
//! memo-1, notes2), trn7 (10 Feb, -8 to Liabilities, memo-2, notes1, reconciled 1 Mar). Liabilities: trn5 (10 Jan,
//! -10 to Expenses, void), trn6 (10 Jan, -11 to Expenses, notes1), trn7.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::OnceLock;

use common::{EMPTY_RESULT, GUID, Host, fixture, registry_with};
use red_ale_core::SlotValue;
use red_ale_core::book_options::OPT_NUM_SOURCE;
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::{Map, Value, json};

/// The test template: the engine's options (then `OPTIONS`), and a render that prints what `trep_splits` returns
/// with `HOOKS`, one line per split: date posted, num, description, memo, notes, account, the other split's account
/// name, amount (the void former amount of a void transaction, as the amount cells show it, TE:1262-1264) and
/// reconcile state; then the total of the amounts, then what `EXTRA` prints.
const TEMPLATE: &str = r#"
fn templates() { [#{ guid: "GUID", name: "Test" }] }
fn options(params, o, book) {
    import "lib/trep-engine" as trep;
    trep::trep_options_generator(o, book);
    OPTIONS
}
fn render(params, book, opts, ctx) {
    import "lib/trep-engine" as trep;
    let r = trep::trep_splits(book, opts, ctx, HOOKS);
    if r.warning != () { print(`warning=${r.warning.title}|${r.warning.text}`); }
    if r.export_error != () { print(`export-error=${r.export_error}`); }
    if r.splits != () {
        let total = rational(0);
        for s in r.splits {
            let t = s.transaction;
            let amount = if t.isVoid { s.voidFormerAmount } else { s.amount };
            total += amount;
            let other = if s.otherSplit == () { "" } else { s.otherSplit.account.name };
            print(`${ctx.print_date_iso(t.datePosted)}|${t.num}|${t.description}|${s.memo}|${t.notes}|${s.account.fullName}|` +
                  `${other}|${monetary(s.account.commodity, amount)}|${s.reconcile}`);
        }
        print(`total=${monetary(book.commodity("USD"), total)}`);
    }
    EXTRA
    RESULT
}
"#;

/// What a run printed.
#[derive(Debug, Default)]
struct Outcome {
    warning: Option<String>,
    export_error: Option<String>,
    splits: Vec<Vec<String>>,
    total: Option<String>,
    extra: Vec<String>,
}

impl Outcome {
    /// Field `i` of each split (0 date, 1 num, 2 description, 3 memo, 4 notes, 5 account, 6 other account, 7 amount,
    /// 8 reconcile state).
    fn column(&self, i: usize) -> Vec<&str> {
        self.splits.iter().map(|s| s[i].as_str()).collect()
    }

    fn descriptions(&self) -> Vec<&str> {
        self.column(2)
    }
}

/// The template with its placeholders filled.
fn script(options: &str, hooks: &str, extra: &str) -> String {
    TEMPLATE
        .replace("\"GUID\"", &format!("\"{GUID}\""))
        .replace("OPTIONS", options)
        .replace("HOOKS", hooks)
        .replace("EXTRA", extra)
        .replace("RESULT", EMPTY_RESULT)
}

fn run_script(host: &Host, script: &str, options: &Value) -> Outcome {
    let reg = registry_with(Library::builtin(), &[("test.rhai", script)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let result = host.run(&reg, GUID, options).unwrap_or_else(|e| panic!("{e:?}"));
    result.log.into_iter().map(|l| l.text).fold(Outcome::default(), |mut o, line| {
        if let Some(w) = line.strip_prefix("warning=") {
            o.warning = Some(w.to_owned());
        } else if let Some(e) = line.strip_prefix("export-error=") {
            o.export_error = Some(e.to_owned());
        } else if let Some(t) = line.strip_prefix("total=") {
            o.total = Some(t.to_owned());
        } else if line.matches('|').count() == 8 {
            o.splits.push(line.split('|').map(str::to_owned).collect());
        } else {
            o.extra.push(line);
        }
        o
    })
}

fn host() -> &'static Host {
    static HOST: OnceLock<Host> = OnceLock::new();
    HOST.get_or_init(|| Host::new(fixture("transaction.gnucash")))
}

fn run(options: &Value) -> Outcome {
    run_script(host(), &script("", "#{}", ""), options)
}

fn expected() -> &'static Value {
    static EXPECTED: OnceLock<Value> = OnceLock::new();
    EXPECTED.get_or_init(|| read_json("fixtures/reports/transaction.expected.json"))
}

fn read_json(path: &str) -> Value {
    let path = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// An account of `transaction.gnucash` by its `tree:full name`, as a GUID.
fn account(name: &str) -> Value {
    let guid = expected()["accounts"][name].clone();
    assert!(guid.is_string(), "no account {name}");
    guid
}

/// The options of a run of an `expected.json` by section and name, each account name looked up in `accounts`.
fn options_of(run: &Value, accounts: &Value) -> Value {
    let mut out: Map<String, Value> = Map::new();
    run["options"].as_object().unwrap().iter().for_each(|(key, value)| {
        let (section, name) = key.split_once('/').unwrap();
        let value = match value {
            Value::Array(names) => Value::Array(names.iter().map(|n| accounts[n.as_str().unwrap()].clone()).collect()),
            v => v.clone(),
        };
        let entry = out.entry(section).or_insert_with(|| json!({}));
        entry.as_object_mut().unwrap().insert(name.to_owned(), value);
    });
    Value::Object(out)
}

fn expected_run(id: &str) -> &'static Value {
    expected()["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no run {id}"))
}

/// A run of `transaction.expected.json` and the expected strings of its check at TT line `line`.
fn run_expected(id: &str, line: u64) -> (Outcome, Vec<String>) {
    let run = expected_run(id);
    let check = run["checks"].as_array().unwrap().iter().find(|c| c["line"] == line).unwrap();
    let want = check["expected"].as_array().map_or_else(Vec::new, |a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect());
    (self::run(&options_of(run, &expected()["accounts"])), want)
}

/// Base testing options (TT:127-143) on Bank, from 1969-01-01 to 1970-12-31, with `more`.
fn bank_1969_1970(more: &Value) -> Value {
    let mut o = json!({
        "Accounts": { "Accounts": [account("trep:Root.Asset.Bank")] },
        "General": { "Start Date": { "absolute": "1969-01-01" }, "End Date": { "absolute": "1970-12-31" } },
    });
    merge(&mut o, more);
    o
}

/// The sorting runs' options (TT:709-721): every Bank split of 1969-1970, void ones too, no subtotals, sorted by
/// `primary` alone.
fn sorted_by(primary: &str, order: &str) -> Value {
    bank_1969_1970(&json!({
        "Filter": { "Void Transactions": "both" },
        "Sorting": { "Primary Key": primary, "Primary Sort Order": order, "Primary Subtotal": false,
                     "Primary Subtotal for Date Key": "none", "Secondary Key": "none", "Secondary Subtotal": false },
    }))
}

fn merge(into: &mut Value, more: &Value) {
    more.as_object().unwrap().iter().for_each(|(section, names)| {
        let entry = into.as_object_mut().unwrap().entry(section).or_insert_with(|| json!({}));
        names.as_object().unwrap().iter().for_each(|(n, v)| {
            entry.as_object_mut().unwrap().insert(n.clone(), v.clone());
        });
    });
}

/// Each option the engine declares on `host`'s book, in order: section, key, name, type, default and help, then the
/// choices.
fn declared(host: &Host) -> Vec<String> {
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script("", "#{}", ""))], Limits::default());
    let db = reg.options(reg.find(GUID).unwrap(), &host.ctx()).unwrap_or_else(|e| panic!("{e:?}"));
    db.describe()
        .into_iter()
        .flat_map(|o| {
            let head = format!("{}/{} {} {} default={} help={}", o.section, o.key, o.name, o.option_type, o.default, o.help);
            let choices = o
                .choices
                .map(|cs| cs.iter().map(|c| format!("{}={}", c.key, c.label)).collect::<Vec<_>>().join(", "));
            std::iter::once(head).chain(choices)
        })
        .collect()
}

fn check(got: &[String], want: &[&str]) {
    got.iter().zip(want).for_each(|(g, w)| assert_eq!(g, w, "{got:#?}"));
    assert_eq!(got.len(), want.len(), "{got:#?}");
}

/// `gnc:trep-options-generator` (TE:521-1012): GnuCash's sections, keys, names, types, defaults, help texts and
/// choices, sorted as GnuCash sorts them, with the three General options every template gets and the internal
/// `__trep` option.
#[test]
fn the_options_are_gnucash_s() {
    check(&declared(host()), OPTIONS);
}

/// What `the_options_are_gnucash_s` expects.
const OPTIONS: &[&str] = &[
    "Accounts/a Accounts accountList default=[] help=Report on these accounts.",
    "Accounts/c Filter Type multichoice default=\"none\" help=Filter account.",
    "none=Do not do any filtering, include=Include Transactions to/from Filter Accounts, exclude=Exclude Transactions \
     to/from Filter Accounts",
    "Accounts/c1 Filter By… accountList default=[] help=Filter on these accounts.",
    "Currency/a Common Currency boolean default=false help=Convert all transactions into a common currency.",
    "Currency/b Show original currency amount boolean default=false help=Also show original currency amounts",
    "Currency/c Report's currency currency default=\"CURRENCY::USD\" help=Select the currency to display the values of this \
     report in.",
    "Currency/d Price Source multichoice default=\"pricedb-nearest\" help=The source of price information.",
    "average-cost=Average cost of purchases weighted by volume, weighted-average=Weighted average of all transactions in \
     the past, pricedb-before=Last up through report date, pricedb-nearest=Closest to report date, pricedb-latest=Most \
     recent",
    "Display/a Date boolean default=true help=Display the date?",
    "Display/a2 Reconciled Date boolean default=false help=Display the reconciled date?",
    "Display/a3 Date Entered boolean default=false help=Display the entered date?",
    "Display/b Num boolean default=true help=Display the check number?",
    "Display/c Description boolean default=true help=Display the description?",
    "Display/d Memo boolean default=true help=Display the memo?",
    "Display/d2 Notes boolean default=true help=Display the notes if the memo is unavailable?",
    "Display/d5 Invoice boolean default=false help=Display invoice details",
    "Display/e Account Name boolean default=false help=Display the account name?",
    "Display/f Use Full Account Name boolean default=true help=Display the full account name?",
    "Display/g Account Code boolean default=false help=Display the account code?",
    "Display/h Detail Level multichoice default=\"single\" help=Amount of detail to display per transaction.",
    "multi-line=One split per line, single=One transaction per line",
    "Display/h5 Other Account Name boolean default=true help=Display the other account name? If this is a split \
     transaction, this parameter is guessed.",
    "Display/i Use Full Other Account Name boolean default=false help=Display the full account name?",
    "Display/j Other Account Code boolean default=false help=Display the other account code?",
    "Display/k Shares boolean default=false help=Display the number of shares?",
    "Display/l Price boolean default=false help=Display the shares price?",
    "Display/l5 Link boolean default=false help=Display the transaction linked document",
    "Display/m Amount multichoice default=\"double\" help=Display the amount?",
    "none=Hide, single=Single Column, double=Two Columns",
    "Display/m1 Sign Reverses multichoice default=\"global\" help=Reverse amount display for certain account types.",
    "global=Use Global Preference, none=Don't change any displayed amounts, income-expense=Income and Expense, \
     credit-accounts=Credit Accounts",
    "Display/m2 Enable Links boolean default=true help=Enable hyperlinks in amounts.",
    "Display/m5 Subtotal Table boolean default=false help=Display a subtotal summary table.",
    "Display/n Account Balance boolean default=false help=Display the balance of the underlying account on each line?",
    "Display/o Grand Total boolean default=true help=Display a grand total section at the bottom?",
    "Display/o2 Running Totals multichoice default=\"none\" help=Display running totals as per report sort order?",
    "none=None, all=Grand Total and Subtotals, grand=Grand Total Only, sub=Subtotals Only",
    "Filter/a5 Account Name Filter string default=\"\" help=Show only accounts whose full name matches this filter e.g. \
     ':Travel' will match Expenses:Travel:Holiday and Expenses:Business:Travel. It can be left blank, which will disable \
     the filter.",
    "Filter/a6 Use regular expressions for account name filter boolean default=false help=By default the account filter \
     will search substring only. Set this to true to enable full POSIX regular expressions capabilities. 'Car|Flights' will \
     match both Expenses:Car and Expenses:Flights. Use a period (.) to match a single character e.g. '20../.' will match \
     'Travel 2017/1 London'. ",
    "Filter/a7 Account Name Filter excludes matched strings boolean default=false help=If this option is selected, accounts \
     matching filter are excluded.",
    "Filter/i1 Transaction Filter string default=\"\" help=Show only transactions where description, notes, or memo matches \
     this filter.\ne.g. '#gift' will find all transactions with #gift in description, notes or memo. It can be left blank, \
     which will disable the filter.",
    "Filter/i2 Use regular expressions for transaction filter boolean default=false help=By default the transaction filter \
     will search substring only. Set this to true to enable full POSIX regular expressions capabilities. '#work|#family' \
     will match both tags within description, notes or memo.",
    "Filter/i3 Transaction Filter excludes matched strings boolean default=false help=If this option is selected, \
     transactions matching filter are excluded.",
    "Filter/i4 Transaction Filter is case insensitive boolean default=false help=If this option is selected, transactions \
     matching filter is not case sensitive.",
    "Filter/j1 Reconciled Status multichoice default=\"all\" help=Filter by reconcile status.",
    "all=Show All Transactions, unreconciled=Unreconciled only, cleared=Cleared only, reconciled=Reconciled only",
    "Filter/k Void Transactions multichoice default=\"non-void-only\" help=How to handle void transactions.",
    "non-void-only=Non-void only, void-only=Void only, both=Both (and include void transactions in totals)",
    "Filter/l Closing transactions multichoice default=\"exclude-closing\" help=By default most users should not include \
     closing transactions in a transaction report. Closing transactions are transfers from income and expense accounts to \
     equity, and must usually be excluded from periodic reporting.",
    "exclude-closing=Exclude closing transactions, include-both=Show both closing and regular transactions, \
     closing-only=Show closing transactions only",
    "General/0a Report name string default=\"Test\" help=Enter a descriptive name for this report.",
    "General/0b Stylesheet multichoice default=\"Default\" help=Select a stylesheet for the report.",
    "Default=Default, Easy=Easy",
    "General/0c Add options summary multichoice default=\"never\" help=Add summary of options.",
    "always=Always, never=Never",
    "General/a5 Date Filter multichoice default=\"posted\" help=Specify date to filter by…",
    "posted=Date Posted, reconciled=Reconciled Date, entered=Date Entered",
    "General/aa Start Date date default={\"relative\":\"start-accounting-period\"} help=Start of reporting period.",
    "General/ab End Date date default={\"relative\":\"end-accounting-period\"} help=End of reporting period.",
    "General/g Table for Exporting boolean default=false help=Formats the table suitable for cut & paste exporting with \
     extra cells.",
    "Sorting/a Primary Key multichoice default=\"account-name\" help=Sort by this criterion first.",
    "account-name=Account Name, account-code=Account Code, date=Date, reconciled-date=Reconciled Date, \
     reconciled-status=Reconciled Status, register-order=Register Order, corresponding-acc-name=Other Account Name, \
     corresponding-acc-code=Other Account Code, amount=Amount, description=Description, number=Number, t-number=Transaction \
     Number, memo=Memo, notes=Notes, none=None",
    "Sorting/e Primary Sort Order multichoice default=\"ascend\" help=Order of primary sorting.",
    "ascend=Ascending, descend=Descending",
    "Sorting/e2 Primary Subtotal for Date Key multichoice default=\"monthly\" help=Do a date subtotal.",
    "none=None, daily=Daily, weekly=Weekly, monthly=Monthly, quarterly=Quarterly, yearly=Yearly",
    "Sorting/e5 Primary Subtotal boolean default=true help=Subtotal according to the primary key?",
    "Sorting/f Secondary Key multichoice default=\"register-order\" help=Sort by this criterion second.",
    "account-name=Account Name, account-code=Account Code, date=Date, reconciled-date=Reconciled Date, \
     reconciled-status=Reconciled Status, register-order=Register Order, corresponding-acc-name=Other Account Name, \
     corresponding-acc-code=Other Account Code, amount=Amount, description=Description, number=Number, t-number=Transaction \
     Number, memo=Memo, notes=Notes, none=None",
    "Sorting/i Secondary Sort Order multichoice default=\"ascend\" help=Order of Secondary sorting.",
    "ascend=Ascending, descend=Descending",
    "Sorting/i2 Secondary Subtotal for Date Key multichoice default=\"monthly\" help=Do a date subtotal.",
    "none=None, daily=Daily, weekly=Weekly, monthly=Monthly, quarterly=Quarterly, yearly=Yearly",
    "Sorting/i5 Secondary Subtotal boolean default=false help=Subtotal according to the secondary key?",
    "Sorting/j1 Show Full Account Name boolean default=false help=Show the full account name for subtotals and subheadings?",
    "Sorting/j2 Show Account Code boolean default=false help=Show the account code for subtotals and subheadings?",
    "Sorting/j3 Show Account Description boolean default=false help=Show the account description for subheadings?",
    "Sorting/j4 Show Informal Debit/Credit Headers boolean default=false help=Show the informal headers for debit/credit \
     accounts?",
    "Sorting/j5 Add indenting columns boolean default=true help=Add indenting columns with grouping and subtotals?",
    "Sorting/j6 Show subtotals only (hide transactional data) boolean default=false help=Show subtotals only, hiding \
     transactional detail?",
    "__trep/ unique-transactions internal default=false help=",
];

/// In a book that uses split actions for numbers, "Num" is "Num/Action", "Trans Number" exists and the `number`
/// key is "Number/Action" (TE:245-256, :910-912, :930-933).
#[test]
fn split_action_books_rename_the_num_options() {
    let mut book = fixture("transaction.gnucash");
    book.slots.set(OPT_NUM_SOURCE, SlotValue::String("t".into()));
    let got = declared(&Host::new(book));
    let lines = |prefix: &str| got.iter().filter(|l| l.contains(prefix)).cloned().collect::<Vec<_>>();
    assert_eq!(lines("Display/b"), [
        "Display/b Num/Action boolean default=true help=Display the check number?",
        "Display/b2 Trans Number boolean default=false help=Display the trans number?",
    ]);
    assert_eq!(lines("number=Number/Action, t-number").len(), 2, "{got:#?}");
}

/// The template's defaults select no account: the no-account warning (TT:99-102; TE:2501-2510, :2524).
#[test]
fn the_default_options_select_no_account() {
    let o = run(&json!({}));
    assert_eq!(o.warning.as_deref(), Some("No accounts selected|This report requires accounts to be selected in the report options."));
    assert_eq!(o.export_error.as_deref(), Some("No accounts, or regexp error"));
    assert!(o.splits.is_empty() && o.total.is_none(), "{o:?}");
}

/// TT:241-252: Bank's 36 splits of the calendar year, $190 a month, in account-name then split order.
#[test]
fn the_base_options_select_bank_s_year() {
    let (o, want) = run_expected("general options", 250);
    assert_eq!(o.total.as_deref(), Some(want[0].as_str()));
    assert_eq!(o.splits.len(), 36);
    let dates = o.column(0);
    assert!(dates.windows(2).all(|w| w[0] <= w[1]), "{dates:?}");
    assert_eq!(&dates[..4], ["2024-01-03", "2024-01-09", "2024-01-15", "2024-02-03"]);
}

/// The filter runs of TT:293-452: each grand total of a single amount column is the sum of the splits kept.
#[test]
fn the_filters_keep_the_splits_test_transaction_totals() {
    [
        ("accounts filter expenses", 301),
        ("accounts filter expen.es regex", 313),
        ("accounts filter exclude expen.es regex", 321),
        ("accounts filter exclude expen.es", 328),
        ("transaction filter to ponies", 338),
        ("transaction filter not.s? regex", 350),
        ("negate transaction filter not.s?", 356),
        ("transaction filter insensitive", 365),
        ("transaction filter regex case insensitive", 372),
        ("unreconciled", 382),
        ("cleared", 388),
        ("reconciled", 394),
        ("including bank-income accts only", 405),
        ("bank exclude bank-income accts", 411),
        ("void only", 421),
        ("both void and non-void", 427),
        ("filter closing - exclude closing txns ", 438),
        ("filter closing - include closing only", 444),
        ("filter closing - include both", 450),
    ]
    .into_iter()
    .for_each(|(id, line)| {
        let (o, want) = run_expected(id, line);
        assert_eq!(o.total.as_deref(), Some(want[0].as_str()), "{id}: {o:#?}");
    });
}

/// TT:305-309, :342-346, :566-571: no table. An account filter that leaves no account gives the no-account warning;
/// no split left gives "No matching transactions found" (TE:2599-2605). Under `exclude`, Filter By… Expenses drops
/// the multisplit, whose other splits include Expenses (fact 11 of the note: the split's own account never counts).
#[test]
fn filters_that_leave_nothing_give_a_warning() {
    let no_match = "No matching transactions found|No transactions were found that match the time interval and account \
                    selection specified in the Options panel.";
    let (o, want) = run_expected("accounts filter expen.es", 307);
    assert!(want.is_empty());
    assert!(o.warning.unwrap().starts_with("No accounts selected|"));
    [("transaction filter not.s?", 344), ("multiline, filtered out", 569)].into_iter().for_each(|(id, line)| {
        let (o, want) = run_expected(id, line);
        assert!(want.is_empty());
        assert_eq!(o.warning.as_deref(), Some(no_match), "{id}");
        assert_eq!(o.export_error.as_deref(), Some("No splits found"), "{id}");
        assert_eq!(o.splits.len(), 0, "{id}");
    });
}

/// TT:709-767: the order of Bank's 1969-1970 splits under each primary key, as the cells TT reads show them (empty
/// cells drop out). The `date`, `number`, `memo`, `corresponding-acc-name` and `amount` keys sort through the query;
/// `reconciled-status` and `notes` through the custom sort.
#[test]
fn each_sort_key_orders_as_test_transaction_expects() {
    let non_empty = |v: Vec<&str>| v.into_iter().filter(|s| !s.is_empty()).map(str::to_owned).collect::<Vec<_>>();
    // The Memo/Notes cell: the memo, or the notes when the memo is empty (TE:1167-1177).
    let memo_notes = |o: &Outcome| {
        non_empty(o.splits.iter().map(|s| if s[3].is_empty() { s[4].as_str() } else { s[3].as_str() }).collect())
    };
    [
        ("sorting=date", 724, 0),
        ("sorting=number", 730, 1),
        ("sorting=reconciled-status", 736, 2),
        ("sorting=memo", 742, 3),
        ("sorting=corresponding-acc-name", 753, 6),
        ("sorting=trans-notes", 759, 3),
        ("sorting=amount", 765, 7),
    ]
    .into_iter()
    .for_each(|(id, line, field)| {
        let (o, want) = run_expected(id, line);
        let got = if field == 3 { memo_notes(&o) } else { non_empty(o.column(field)) };
        assert_eq!(got, want, "{id}");
    });
    // TT:746-749 reads the Account column sorted with `string<?`: every split here is Bank's.
    let (o, _) = run_expected("sorting=account-name", 748);
    assert_eq!(o.column(5), ["Root:Asset:Bank"; 5]);
}

/// TT:1007-1021: the Invoice cells of the invoice tree's A/P and A/R splits, which the query's account-name sort puts
/// before trep's Bank: the two top-level accounts named "Root" compare equal, so the names below decide (§14), and
/// A/Payable's come first, each account's in split order.
#[test]
fn invoices_follow_the_query_s_account_name_sort() {
    let run = expected_run("show invoice");
    let extra = r#"
        for s in r.splits {
            let ids = [];
            for x in s.transaction.splits() {
                if x.account.type in ["PAYABLE", "RECEIVABLE"] && x.lot != () && x.lot.invoice != () { ids.insert(0, x.lot.invoice.id); }
            }
            print(`invoices=${ids}`);
        }
    "#;
    let o = run_script(host(), &script("", "#{}", extra), &options_of(run, &expected()["accounts"]));
    let want = run["checks"][0]["expected"].as_array().unwrap();
    let got: Vec<String> = o.extra.iter().map(|l| l.trim_start_matches("invoices=").replace(['[', ']', '"'], "")).collect();
    assert_eq!(got, want.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>());
    assert_eq!(o.column(5)[..4], ["Root:A/Payable"; 4]);
}

/// Hand-worked sorts (§4.6 of the note): each remaining key, descending through the query and through the custom
/// sort, and Primary Key "None", which keeps split order where GnuCash leaves hash order (§14). A query sort's ties
/// fall to split order (TE:2541-2549); a custom sort's to the order before it, the query's split order.
#[test]
fn hand_worked_sort_keys_and_orders() {
    let order = |key: &str, ord: &str| run(&sorted_by(key, ord)).descriptions().iter().map(|d| (*d).to_owned()).collect::<Vec<_>>();
    let all = ["desc-2", "desc-3", "desc-1", "desc-4", "desc-7"];
    assert_eq!(order("register-order", "ascend"), all);
    assert_eq!(order("none", "descend"), all);
    assert_eq!(order("date", "descend"), ["desc-7", "desc-4", "desc-1", "desc-2", "desc-3"]);
    assert_eq!(order("description", "descend"), ["desc-7", "desc-4", "desc-3", "desc-2", "desc-1"]);
    assert_eq!(order("t-number", "descend"), ["desc-7", "desc-4", "desc-3", "desc-2", "desc-1"]);
    // trn2 is void: its value, which the query sorts by, is 0 (TT:763-767 shows the former amount).
    assert_eq!(order("amount", "descend"), ["desc-3", "desc-2", "desc-1", "desc-7", "desc-4"]);
    // Never reconciled reads as date 0; trn3 and trn7 share 1 March 1970.
    assert_eq!(order("reconciled-date", "ascend"), ["desc-2", "desc-1", "desc-4", "desc-3", "desc-7"]);
    // Only Expenses has a code, EXP-001.
    assert_eq!(order("corresponding-acc-code", "ascend"), ["desc-2", "desc-3", "desc-7", "desc-1", "desc-4"]);
    assert_eq!(order("reconciled-status", "descend"), ["desc-1", "desc-4", "desc-3", "desc-7", "desc-2"]);
    assert_eq!(order("notes", "descend"), ["desc-2", "desc-4", "desc-7", "desc-3", "desc-1"]);
    // "None" first leaves the secondary key unused as well.
    let o = run(&bank_1969_1970(&json!({ "Filter": { "Void Transactions": "both" },
        "Sorting": { "Primary Key": "none", "Secondary Key": "description", "Secondary Sort Order": "descend" } })));
    assert_eq!(o.descriptions(), all);
}

/// Date subtotals sort through the custom sort (TE:2431-2437): by the group of the key's date, then the secondary
/// key, then split order; a group keeps split order inside it even when descending. A reconciled-date key groups
/// by the reconcile date's month.
#[test]
fn date_subtotals_sort_by_group() {
    let yearly = run(&bank_1969_1970(&json!({ "Filter": { "Void Transactions": "both" }, "Sorting": {
        "Primary Key": "date", "Primary Subtotal for Date Key": "yearly", "Primary Sort Order": "descend" } })));
    assert_eq!(yearly.descriptions(), ["desc-1", "desc-4", "desc-7", "desc-2", "desc-3"]);
    let reconciled = run(&bank_1969_1970(&json!({ "Sorting": {
        "Primary Key": "reconciled-date", "Primary Subtotal for Date Key": "monthly", "Secondary Key": "amount",
        "Secondary Sort Order": "descend" } })));
    // Never reconciled is January 1970 at UTC, before March: trn1 and trn4 by value descending, then trn3, trn7.
    assert_eq!(reconciled.descriptions(), ["desc-1", "desc-4", "desc-3", "desc-7"]);
    // Primary account name with a monthly secondary: whole full names compared, then months, then split order.
    let trep: Vec<Value> = ["trep:Root.Asset.Bank", "trep:Root.Expenses", "trep:Root.Income", "trep:Root.Liabilities"]
        .into_iter()
        .map(account)
        .collect();
    let o = run(&json!({
        "Accounts": { "Accounts": trep },
        "General": { "Start Date": { "absolute": "1969-01-01" }, "End Date": { "absolute": "1970-12-31" } },
        "Sorting": { "Secondary Key": "date" },
    }));
    let rows: Vec<String> = o.splits.iter().map(|s| format!("{} {}", s[5], s[1])).collect();
    assert_eq!(rows, [
        "Root:Asset:Bank trn3", "Root:Asset:Bank trn1", "Root:Asset:Bank trn4", "Root:Asset:Bank trn7",
        "Root:Expenses trn1", "Root:Expenses trn6", "Root:Expenses trn4", "Root:Income trn3", "Root:Liabilities trn6",
        "Root:Liabilities trn7",
    ]);
}

/// The other date sources (TE:2566-2578): under "Reconciled Date" only a reconciled split is checked against the
/// dates and the query leaves posted dates alone; under "Date Entered" the entered date is checked.
#[test]
fn date_filters_on_reconcile_and_entered_dates() {
    let liabilities = |from: &str, to: &str, source: &str| {
        run(&json!({
            "Accounts": { "Accounts": [account("trep:Root.Liabilities")] },
            "General": { "Start Date": { "absolute": from }, "End Date": { "absolute": to }, "Date Filter": source },
        }))
        .column(1)
        .iter()
        .map(|n| (*n).to_owned())
        .collect::<Vec<_>>()
    };
    assert_eq!(liabilities("1970-03-01", "1970-03-31", "reconciled"), ["trn6", "trn7"]);
    assert_eq!(liabilities("1970-04-01", "1970-04-30", "reconciled"), ["trn6"]);
    assert_eq!(liabilities("1970-03-01", "1970-03-31", "posted"), Vec::<String>::new());
    assert_eq!(liabilities("2024-03-15", "2024-03-15", "entered"), ["trn6", "trn7"]);
    assert_eq!(liabilities("2024-03-16", "2024-12-31", "entered"), Vec::<String>::new());
}

/// Regular expressions that do not compile stop the report before the query (TE:2512-2524), unless the account
/// filter, falling back to a substring, already left no account: the no-account warning comes first.
#[test]
fn bad_regular_expressions_give_warnings() {
    let filter = |more: Value| run(&bank_1969_1970(&json!({ "Filter": more })));
    let o = filter(json!({ "Account Name Filter": "Bank(", "Use regular expressions for account name filter": true,
                           "Account Name Filter excludes matched strings": true }));
    assert_eq!(o.warning.as_deref(), Some("Error invalid-account-regex|"));
    assert_eq!(o.export_error.as_deref(), Some("No accounts, or regexp error"));
    let o = filter(json!({ "Account Name Filter": "Bank(", "Use regular expressions for account name filter": true }));
    assert!(o.warning.unwrap().starts_with("No accounts selected|"));
    let o = filter(json!({ "Transaction Filter": "desc-(", "Use regular expressions for transaction filter": true }));
    assert_eq!(o.warning.as_deref(), Some("Error invalid-transaction-regex|"));
}

/// TT's filters do not reach these (§4.6): `unreconciled` with `void-only` leaves no state (TE:2259-2262); Filter
/// By… looks only at a split's other splits (TE:2491-2499); the transaction filter reads notes and memos too.
#[test]
fn hand_worked_filters() {
    let o = run(&bank_1969_1970(&json!({ "Filter": { "Reconciled Status": "unreconciled", "Void Transactions": "void-only" } })));
    assert!(o.splits.is_empty() && o.warning.unwrap().starts_with("No matching transactions found|"));
    let bank = account("trep:Root.Asset.Bank");
    let o = run(&bank_1969_1970(&json!({ "Accounts": { "Filter By…": [bank], "Filter Type": "include" } })));
    assert!(o.splits.is_empty(), "{o:?}");
    let o = run(&bank_1969_1970(&json!({ "Accounts": { "Filter By…": [account("trep:Root.Expenses")], "Filter Type": "include" } })));
    assert_eq!(o.descriptions(), ["desc-1", "desc-4"]);
    let o = run(&bank_1969_1970(&json!({ "Filter": { "Transaction Filter": "MEMO-", "Transaction Filter is case insensitive": true } })));
    assert_eq!(o.descriptions(), ["desc-1", "desc-4", "desc-7"]);
    let o = run(&bank_1969_1970(&json!({ "Filter": { "Transaction Filter": "^notes[12]$",
        "Use regular expressions for transaction filter": true, "Transaction Filter excludes matched strings": true } })));
    assert_eq!(o.descriptions(), ["desc-3", "desc-1"]);
}

/// The renderer's keyword arguments (TE:2179-2201): accounts in place of the Accounts option, a split filter, a
/// split date with its behaviour for #f, and `__trep`/`unique-transactions` set by a derived report. An unknown
/// hook is refused.
#[test]
fn hooks_and_unique_transactions() {
    let options = bank_1969_1970(&json!({}));
    let with = |opts: &str, hooks: &str| run_script(host(), &script(opts, hooks, ""), &options);
    let liabilities = r#"#{ custom_source_accounts: [book.account("Root:Liabilities")] }"#;
    assert_eq!(with("", liabilities).column(1), ["trn6", "trn7"]);
    let positive = "#{ custom_split_filter: |s| s.amount > 0 }";
    assert_eq!(with("", positive).column(1), ["trn3"]);
    // The reconcile date as the split's date: only trn3 and trn7 have one, in March 1970.
    let dated = |include: bool| format!("#{{ split_to_date: |s| s.reconcileDate, split_to_date_include_false: {include} }}");
    assert_eq!(with("", &dated(false)).column(1), ["trn3", "trn7"]);
    // With #f kept, every Bank split of the book is in: the query leaves posted dates alone (TE:2537-2538).
    let all = with("", &dated(true));
    assert_eq!((&all.column(1)[..4], all.splits.len()), (&["trn3", "trn1", "trn4", "trn7"][..], 41));
    let unique = r#"o.set_default("__trep", "unique-transactions", true);"#;
    let accounts = json!({ "Accounts": { "Accounts": [account("trep:Root.Asset.Bank"), account("trep:Root.Expenses")] } });
    let two = |opts: &str| {
        let mut o = options.clone();
        merge(&mut o, &accounts);
        run_script(host(), &script(opts, "#{}", ""), &o).column(5).len()
    };
    assert_eq!((two(""), two(unique)), (7, 5));
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script("", "#{ split_filter: 1 }", ""))], Limits::default());
    let err = host().run(&reg, GUID, &options).unwrap_err();
    assert!(format!("{err:?}").contains("unknown trep-renderer hook split_filter"), "{err:?}");
}

/// The parameters ticket 5's table reads (TE:2289-2429), and the renderer-fn and split-sortvalue maps applied to a
/// split: account keys give the account, date subtotals their group and label from the date posted.
#[test]
fn parameters_and_key_functions() {
    let extra = r#"
        let p = r.parameters;
        print(`bal-bf=${p.bal_bf} table=${r.subtotal_table} only=${p.subtotals_only} signs=${p.reversed_signs} ` +
              `common=${p.common_currency} matched=${p.accounts_matched.len()} single=${p.amount_single}`);
        print(`primary=${p.primary_key_renderer_fn} secondary=${p.secondary_key_renderer_fn} ` +
              `running=${p.running_prime}/${p.running_sec}/${p.running_grand_total}`);
        if p.primary_key_renderer_fn == () { return #{ title: "t", sections: [] }; }
        let s = r.splits[0];
        print(`render=${trep::apply_renderer_fn(ctx, p.primary_key_renderer_fn, s).fullName} ` +
              `value=${trep::apply_split_sortvalue(ctx, p.primary_key_split_sortvalue, s)}`);
        for ds in ["daily", "weekly", "monthly", "quarterly", "yearly"] {
            let f = #{ key: "date", date_subtotal: ds, split_action: false };
            print(`${ds}=${trep::apply_split_sortvalue(ctx, f, s)} ${trep::apply_renderer_fn(ctx, f, s)}`);
        }
        for key in ["reconciled-status", "corresponding-acc-name", "description", "date", "amount"] {
            let f = #{ key: key, date_subtotal: (), split_action: false };
            print(`${key}=${trep::apply_split_sortvalue(ctx, f, s)} ${trep::apply_renderer_fn(ctx, f, s)}`);
        }
    "#;
    let options = bank_1969_1970(&json!({ "Display": { "Sign Reverses": "none", "Running Totals": "sub" },
                                          "Sorting": { "Secondary Key": "date" } }));
    let o = run_script(host(), &script("", "#{}", extra), &options);
    check(&o.extra, &[
        "bal-bf=true table=false only=false signs=true common=false matched=1 single=false",
        "primary=#{\"date_subtotal\": (), \"key\": \"account-name\", \"split_action\": false} secondary=#{\"date_subtotal\": \
         \"monthly\", \"key\": \"date\", \"split_action\": false} running=true/true/false",
        "render=Root:Asset:Bank value=Root:Asset:Bank",
        "daily=984865 12/31/69",
        "weekly=-1 12/28/69 to 01/03/70",
        "monthly=196912 December 1969",
        "quarterly=19694 Q4 1969",
        "yearly=1969 1969",
        "reconciled-status=4 Cleared",
        "corresponding-acc-name=Root:Income Root:Income",
        "description=desc-3 desc-3",
        "date=-46860 ",
        "amount=29 ",
    ]);
    let o = run_script(host(), &script("", "#{}", extra), &bank_1969_1970(&json!({
        "Filter": { "Transaction Filter": "desc" }, "Display": { "Subtotal Table": true, "Amount": "single" },
        "Currency": { "Common Currency": true }, "Sorting": { "Show subtotals only (hide transactional data)": true } })));
    assert_eq!(o.extra[0], "bal-bf=false table=true only=true signs=global common=CURRENCY::USD matched=1 single=true");
    // The Subtotal Table reads the Primary Subtotal option (TE:2281-2287); without it, and without a secondary
    // subtotal, no key has a renderer.
    let o = run_script(host(), &script("", "#{}", extra), &bank_1969_1970(&json!({
        "Display": { "Subtotal Table": true }, "Sorting": { "Primary Subtotal": false } })));
    assert_eq!(o.extra, ["bal-bf=false table=false only=false signs=global common=false matched=1 single=false",
                         "primary= secondary= running=false/false/false"]);
}

/// A smoke run on SQLite (`fixtures/sqlite/reports/reconcile.gnucash`, GnuCash's SQLite copy of the reconcile book;
/// the transaction book has none): the Reconciliation Report's sort and date filter (RR:34-58) on Bank and
/// Liabilities in March 1970 select the same splits as on XML, grouped as TT:1110-1121 totals them: Reconciled $8,
/// Cleared $29, Unreconciled $31 (the absolute amounts the Funds In and Out columns show).
#[test]
fn the_reconcile_book_reads_the_same_on_sqlite() {
    let expected = read_json("fixtures/reports/reconcile.expected.json");
    let options = json!({
        "Accounts": { "Accounts": [expected["accounts"]["Root.Asset.Bank"], expected["accounts"]["Root.Liabilities"]] },
        "General": { "Start Date": { "absolute": "1970-03-01" }, "End Date": { "absolute": "1970-03-31" }, "Date Filter": "reconciled" },
        "Sorting": { "Primary Key": "reconciled-status", "Secondary Key": "date", "Secondary Subtotal for Date Key": "none" },
    });
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("reconcile.gnucash");
    std::fs::copy(format!("{}/../../fixtures/sqlite/reports/reconcile.gnucash", env!("CARGO_MANIFEST_DIR")), &copy).unwrap();
    let (sqlite, _, _) = red_ale_sql::open_read_only(&copy).unwrap();
    let [xml, sql] = [fixture("reconcile.gnucash"), sqlite].map(|book| {
        let o = run_script(&Host::new(book), &script("", "#{}", ""), &options);
        o.splits.iter().map(|s| format!("{} {} {}", s[8], s[1], s[7])).collect::<Vec<_>>()
    });
    assert_eq!(xml, ["y trn7 -$8.00", "c trn3 $29.00", "n trn1 -$5.00", "n trn6 -$11.00", "n trn4 -$15.00"]);
    assert_eq!(sql, xml);
}

// The second half: `trep_renderer`, the document. The Transaction Report's own tests (red-ale-rpc's
// `report_transaction.rs`) check every table assertion of TT; these check what that report cannot reach: the CSV
// rows and export errors (the report declares no export), the renderer's hooks and the Sign Reverses choices against
// a global preference.

/// A template whose render is `trep_renderer` with `HOOKS`, offering a CSV export as the GST Statement does.
const RENDERER: &str = r#"
fn templates() { [#{ guid: "GUID", name: "Test", exportTypes: ["csv"] }] }
fn options(params, o, book) {
    import "lib/trep-engine" as trep;
    trep::trep_options_generator(o, book);
}
fn render(params, book, opts, ctx) {
    import "lib/trep-engine" as trep;
    trep::trep_renderer(book, opts, ctx, HOOKS)
}
"#;

/// `trep_renderer`'s result on `host` with `hooks` and `options`, for the export `export` or none, as JSON.
fn render_on(host: &Host, hooks: &str, options: &Value, export: Option<&str>) -> Value {
    let script = RENDERER.replace("\"GUID\"", &format!("\"{GUID}\"")).replace("HOOKS", hooks);
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let mut ctx = host.ctx();
    ctx.export = export;
    let t = reg.find(GUID).unwrap();
    let result = reg.run(t, &[options], None, &ctx).unwrap_or_else(|e| panic!("{e:?}"));
    serde_json::to_value(result).unwrap()
}

/// The only table of a result.
fn the_table(result: &Value) -> Value {
    let blocks = result["sections"][0]["blocks"].as_array().unwrap();
    let tables: Vec<&Value> = blocks.iter().filter(|b| b["kind"] == "table").collect();
    assert_eq!(tables.len(), 1, "{result:#}");
    tables[0].clone()
}

/// A table's column headings and each row's cells as they show: display or text, anchor texts joined by `,`,
/// `-` for an empty cell.
fn shown(table: &Value) -> Vec<String> {
    let cell = |c: &Value| match c {
        Value::Null => "-".to_owned(),
        c if c["anchors"].is_array() => c["anchors"].as_array().unwrap().iter().map(|a| a["text"].as_str().unwrap()).collect::<Vec<_>>().join(","),
        c => c["display"].as_str().or_else(|| c["text"].as_str()).unwrap().to_owned(),
    };
    let head = table["columns"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap()).collect::<Vec<_>>().join("|");
    let rows = table["rows"].as_array().unwrap().iter().map(|r| r["cells"].as_array().unwrap().iter().map(cell).collect::<Vec<_>>().join("|"));
    std::iter::once(head).chain(rows).collect()
}

/// TT:1025-1050: the CSV of the grand totals, in GBP converted at the 2000 prices and in the original USD: the
/// dates in ISO form, each amount as a double prints, no export error. GnuCash's `lists->csv` cases (TT:1055-1069)
/// are `render::csv`'s own tests.
#[test]
fn csv_rows_are_the_grand_totals() {
    let run = expected_run("csv-export");
    let result = render_on(host(), "#{}", &options_of(run, &expected()["accounts"]), Some("csv"));
    let checks = run["checks"].as_array().unwrap();
    assert!(result.get("exportError").is_none(), "{}", checks[0]["name"]);
    let rows: red_ale_report::result::ExportRows = serde_json::from_value(result["export"].clone()).unwrap();
    assert_eq!(red_ale_report::render::csv(&rows), checks[1]["expected"].as_str().unwrap(), "{}", checks[1]["name"]);
    assert_eq!(checks.len(), 2);
}

/// GnuCash's export errors (TE:2524, :2605, :2641), set only for an export: no account, no split, two amount
/// columns. A run that is not for an export carries neither rows nor an error.
#[test]
fn csv_export_errors() {
    let error = |options: &Value| render_on(host(), "#{}", options, Some("csv"))["exportError"].clone();
    assert_eq!(error(&json!({})), "No accounts, or regexp error");
    let bank = bank_1969_1970(&json!({}));
    assert_eq!(error(&bank), "CSV disabled for double column amounts");
    assert_eq!(error(&bank_1969_1970(&json!({ "Filter": { "Transaction Filter": "nothing matches" } }))), "No splits found");
    let single = bank_1969_1970(&json!({ "Display": { "Amount": "single" } }));
    let rows = render_on(host(), "#{}", &single, Some("csv"))["export"].clone();
    assert_eq!(rows, json!([["from", "1969-01-01"], ["to", "1970-12-31"], ["Amount", { "num": "1", "denom": "1" }]]));
    let plain = render_on(host(), "#{}", &single, None);
    assert!(plain.get("export").is_none() && plain.get("exportError").is_none(), "{plain:#}");
}

/// `#:custom-calculated-cells` (TE:1557-1573): cells as maps with closures replace the defaults, here the
/// Reconciliation Report's Funds In and Out (RR:69-98), whose subtotal rows do not merge (no start-dual column);
/// the legacy vectors (TE:1014-1026) are upgraded; a cell lacking a key falls back to the defaults, logged.
#[test]
fn custom_calculated_cells() {
    let funds = r#"#{ custom_calculated_cells: |options| [
        trep::calc_cell("Funds In", |s, tr| if s.amount > 0 { monetary(s.account.commodity, s.amount) }, false, true, false, (), true),
        trep::calc_cell("Funds Out", |s, tr| if s.amount > 0 { () } else { monetary(s.account.commodity, -s.amount) }, false, true, false, (), false),
    ] }"#;
    let options = bank_1969_1970(&json!({ "Sorting": { "Primary Subtotal": false }, "Display": { "Grand Total": true } }));
    let table = the_table(&render_on(host(), funds, &options, None));
    let rows = shown(&table);
    assert_eq!(rows[0], "Date|Num|Description|Memo/Notes|Transfer from/to|Funds In|Funds Out");
    assert_eq!(rows[1], "12/31/69|trn3|desc-3|-|Income|$29.00|-");
    assert_eq!(rows.last().unwrap(), "Grand Total|$29.00|$28.00");
    let legacy = r#"#{ custom_calculated_cells: |options| [["Value", |s| monetary(s.account.commodity, s.value), true, true, false, ()]] }"#;
    let rows = shown(&the_table(&render_on(host(), legacy, &options, None)));
    assert_eq!((rows[0].as_str(), rows.last().unwrap().as_str()), ("Date|Num|Description|Memo/Notes|Transfer from/to|Value", "Grand Total|$1.00"));
    let broken = r#"#{ custom_calculated_cells: |options| [#{ heading: "Broken" }] }"#;
    let result = render_on(host(), broken, &options, None);
    assert_eq!(shown(&the_table(&result))[0], "Date|Num|Description|Memo/Notes|Transfer from/to|Debit|Credit");
    assert!(result["log"][0]["text"].as_str().unwrap().starts_with("field calc_fn missing in cell"), "{:#}", result["log"]);
}

/// Sign Reverses (TE:1072-1080, :2345) with the global preference reversing credit accounts: "global" and "none"
/// both follow it, since GnuCash turns "none" into #t, which `sign-reverse-list` lacks; "income-expense" reverses
/// Income and Expenses. Income's -$29 of trn3 and Liabilities' $8 of trn7, single column.
#[test]
fn sign_reverses_none_reads_the_global_preference() {
    let accounts = ["trep:Root.Income", "trep:Root.Liabilities"].map(account);
    let amounts = |choice: &str| {
        let options = bank_1969_1970(&json!({
            "Accounts": { "Accounts": accounts }, "Display": { "Amount": "single", "Sign Reverses": choice },
            "Filter": { "Transaction Filter": "desc-[37]", "Use regular expressions for transaction filter": true },
        }));
        let rows = shown(&the_table(&render_on(host(), "#{}", &options, None)));
        rows.iter().filter(|r| r.contains("desc-")).map(|r| r.rsplit('|').next().unwrap().to_owned()).collect::<Vec<_>>()
    };
    assert_eq!(amounts("global"), ["$29.00", "-$8.00"]);
    assert_eq!(amounts("none"), ["$29.00", "-$8.00"]);
    assert_eq!(amounts("income-expense"), ["$29.00", "$8.00"]);
    assert_eq!(amounts("credit-accounts"), ["$29.00", "-$8.00"]);
}

/// `#:empty-report-message` (TE:2526-2531) follows the warnings before the query, not "No matching transactions".
#[test]
fn the_empty_report_message_follows_the_no_account_warning() {
    let hooks = r#"#{ empty_report_message: "Please select the account from Report Options." }"#;
    let kinds = |options: &Value| {
        let result = render_on(host(), hooks, options, None);
        assert_eq!(result["title"], "Test");
        result["sections"][0]["blocks"].as_array().unwrap().iter().map(|b| b["kind"].as_str().unwrap().to_owned()).collect::<Vec<_>>()
    };
    assert_eq!(kinds(&json!({})), ["warning", "text"]);
    let bad_regex = bank_1969_1970(&json!({ "Filter": { "Transaction Filter": "(", "Use regular expressions for transaction filter": true } }));
    assert_eq!(kinds(&bad_regex), ["warning", "text"]);
    assert_eq!(kinds(&bank_1969_1970(&json!({ "Filter": { "Transaction Filter": "nothing matches" } }))), ["warning"]);
    assert_eq!(kinds(&bank_1969_1970(&json!({}))), ["text", "table"]);
}
