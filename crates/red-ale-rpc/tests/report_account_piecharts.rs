//! The five account pie charts (`standard/account-piecharts.rhai`, translating
//! `5.17:gnucash/report/reports/standard/account-piecharts.scm`, cited below as AP) run through `report.run` and
//! `report.export` on copies of the report fixtures.
//!
//! GnuCash 5.17's tests only check that these reports render (`standard/test/test-charts.scm:189`,
//! `test/test-stress-options.scm`), so every figure here is hand-worked from the Scheme, citing the lines it
//! follows. Two lines of reasoning recur:
//!
//! - Only the root's direct children of the report's types are walked (AP:337-340). In `test-data` and
//!   `balsheet-pnl` every account sits below a top-level ASSET account "Root", so the income, expense and liability
//!   pies find nothing there; `category-multi` (a top-level EXPENSE tree) and `category-liability` (top-level ASSET,
//!   CREDIT and INCOME accounts) give them figures.
//! - Above the depth limit an account's own balance is a slice; at it, the balance with its sub-accounts
//!   (AP:217-251, :409-410). Each account's sub-accounts are listed before it and later accounts before earlier ones,
//!   and the stable sort keeps that order between equal amounts (AP:443-446).
//!
//! The report settings are GnuCash's out-of-the-box ones: no reversed balances (the pies negate on their own,
//! AP:432-437) and the accounting period this calendar year, 2024 for the pinned now.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Value, json};

/// 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
const NOW: Time64 = Time64(1_710_504_000);

const INCOME: &str = "e1bd09b8a1dd49dd85760db9d82b045c";
const EXPENSE: &str = "9bf1892805cb4336be6320fe48ce5446";
const ASSETS: &str = "5c7fd8a1fe9a4cd38884ff54214aa88a";
const SECURITIES: &str = "e9418ff64f2c11e5b61d1c7508d793ed";
const LIABILITIES: &str = "3fe6dce77da24c66bdc8f8efdea7f9ac";
const TEMPLATES: [&str; 5] = [INCOME, EXPENSE, ASSETS, SECURITIES, LIABILITIES];

/// The fixtures and the date format of the GnuCash test each comes from (`fixtures/reports/*.expected.json`).
const FIXTURES: [(&str, &str); 6] = [
    ("test-data", "locale"),
    ("balsheet-pnl", "iso"),
    ("category-multi", "iso"),
    ("category-liability", "iso"),
    ("charts-daily", "iso"),
    ("charts-net", "iso"),
];

/// A fixture book's account GUID by its number: `a…N` is the Nth account the builder made (`fixtures/README.md`).
fn acct(n: u32) -> String {
    format!("a{n:031x}")
}

/// A copy of a fixture opened for writing, behind a dispatcher with a fixed clock.
struct Setup {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Setup {
    /// The fixture `name` with the report settings of GnuCash's tests but the default accounting period, dates
    /// printed in `date_format`.
    fn new(name: &str, date_format: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("reports")).unwrap();
        let config = json!({ "reports": { "directory": "reports", "reverseBalancedAccounts": "none", "dateFormat": date_format } });
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let book = dir.path().join("book.gnucash");
        std::fs::copy(
            format!("{}/../../fixtures/reports/{name}.gnucash", env!("CARGO_MANIFEST_DIR")),
            &book,
        )
        .unwrap();
        let mut s = Self {
            d: dispatcher(&dir.path().join("config.json")),
            dir,
        };
        s.call("book.open", json!({ "path": book.display().to_string() }));
        s
    }

    fn fixture(name: &str) -> Self {
        let (_, format) = FIXTURES.iter().find(|(n, _)| *n == name).unwrap();
        Self::new(name, format)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn run(&mut self, template: &str, options: &Value) -> Value {
        self.call("report.run", json!({ "report": template, "options": options }))
    }

    fn export(&mut self, template: &str, options: &Value) -> String {
        self.call("report.export", json!({ "report": template, "options": options }))["html"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// The one block of the report's one section.
    fn block(&mut self, template: &str, options: &Value) -> Value {
        let result = self.run(template, options);
        let blocks = result["sections"][0]["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1, "{result}");
        blocks[0].clone()
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

fn dispatcher(config: &Path) -> Dispatcher {
    let settings = Settings {
        config: ConfigStore::at(PathBuf::from(config)),
        ..Settings::default()
    };
    let mut d = Dispatcher::new(settings);
    let made = AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let seed = (0x00ed_u128 << 112) | (u128::from(made.fetch_add(1, Ordering::Relaxed)) << 80);
        Box::new(FixedEnv::new(NOW, seed))
    }));
    d
}

/// Text as the HTML renderer escapes it.
fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A pie as a hand-worked case expects it: the two title lines and each slice's legend label and display.
struct Pie<'a> {
    title: [&'a str; 2],
    slices: &'a [(&'a str, &'a str)],
}

/// Run `template` with `options` and check the pie's title, legend labels and slice displays, then check the
/// exported HTML shows each of them.
fn check_pie(s: &mut Setup, case: &str, template: &str, options: Value, want: &Pie<'_>) -> Value {
    let chart = s.block(template, &options);
    assert_eq!(chart["kind"], "chart", "{case}: {chart}");
    assert_eq!(chart["type"], "pie", "{case}");
    assert_eq!(chart["title"], json!(want.title), "{case}: title");
    let labels: Vec<&str> = want.slices.iter().map(|(l, _)| *l).collect();
    let displays: Vec<&str> = want.slices.iter().map(|(_, d)| *d).collect();
    assert_eq!(chart["labels"], json!(labels), "{case}: labels");
    let series = &chart["series"].as_array().unwrap()[0];
    assert_eq!(series["label"], "Accounts", "{case}: AP:528-532");
    assert_eq!(series["displays"], json!(displays), "{case}: slice values");
    assert_eq!(
        series["colors"].as_array().unwrap().len(),
        labels.len(),
        "{case}: one colour per slice (AP:531)"
    );
    let html = s.export(template, &options);
    let shown = want.title.iter().chain(labels.iter()).chain(displays.iter());
    shown.for_each(|text| assert!(html.contains(&escaped(text)), "{case}: the export shows {text:?}"));
    chart
}

/// Run `template` with `options` and check it gives the warning titled `title`, in the result and the export.
fn check_warning(s: &mut Setup, case: &str, template: &str, options: Value, title: &str) {
    let block = s.block(template, &options);
    assert_eq!(
        (block["kind"].as_str(), block["title"].as_str()),
        (Some("warning"), Some(title)),
        "{case}: {block}"
    );
    assert!(s.export(template, &options).contains(title), "{case}: the export shows the warning");
}

/// The link of slice `i`.
fn link(chart: &Value, i: usize) -> Value {
    chart["series"][0]["links"][i].clone()
}

fn account_link(n: u32) -> Value {
    json!({ "account": acct(n) })
}

/// A drill-down link's template, its Accounts and the options it changes beside them, as `(levels, name)`.
fn report_link(link: &Value) -> (&str, Vec<&str>, &Value, &Value) {
    let report = &link["report"];
    let accounts = report["options"]["Accounts"]["Accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let options = &report["options"];
    (
        report["template"].as_str().unwrap(),
        accounts,
        &options["Accounts"]["Levels of Subaccounts"],
        &options["General"]["Report name"],
    )
}

fn date(d: &str) -> Value {
    json!({ "absolute": d })
}

// ---------------------------------------------------------------------------------------------------------------
// test-data: Root (ASSET) > Asset > Bank, GBP Bank (GBP), Wallet, and Income, Income-GBP, Expenses, Liabilities and
// Equity beside Asset. Balances at the end of 2024: Bank $2,186, GBP Bank #608, Wallet $20
// (`standard/test/test-account-summary.scm:61-64`). The nearest price to 2024-12-31 is GBP in USD 3/2 of
// 2000-02-15, nearer than USD in GBP 5/7 of 2000-01-15.
// ---------------------------------------------------------------------------------------------------------------

/// Defaults (AP:87-163): End Date end-accounting-period (2024-12-31 23:59:59, AP:301-303), two levels. Root is at
/// depth 1 and shows its own balance, zero, which no slice shows (AP:444); Asset is at the depth limit and shows its
/// balance with its sub-accounts: $2,186 + #608 × 3/2 + $20 = $3,118 (AP:397-405, `gnc:sum-collector-commodity`
/// converting each commodity's total). The title's second line is "Balance at" the date and the total (AP:509-525),
/// the label the name, amount and `~2,1f` share (AP:535-556). Asset has sub-accounts, so its link reruns the report
/// on it and its children, one level deeper, named after it (AP:474-496).
#[test]
fn test_data_assets_defaults() {
    let mut s = Setup::fixture("test-data");
    let pie = Pie {
        title: ["Assets", "Balance at 12/31/24: $3,118.00"],
        slices: &[("Asset - $3,118.00 (100.0%)", "$3,118.00")],
    };
    let chart = check_pie(&mut s, "assets defaults", "Assets", json!({}), &pie);
    let asset = link(&chart, 0);
    let (template, accounts, levels, name) = report_link(&asset);
    assert_eq!(template, ASSETS);
    assert_eq!(accounts, [2, 3, 4, 5].map(acct), "Asset and its children (AP:479-488)");
    assert_eq!((levels, name), (&json!(3), &json!("Asset")), "AP:489-495");
    assert_eq!(
        chart["yAxis"],
        json!({ "label": "", "format": "currency", "commodity": "CURRENCY::USD" }),
        "AP:504-507"
    );
    assert_eq!(
        (chart["width"].clone(), chart["height"].clone()),
        (json!({ "percent": 100 }), json!({ "percent": 100 })),
        "AP:153-155"
    );
}

/// Three levels: Root and Asset show their own zero balances; Bank, GBP Bank and Wallet, at the limit, theirs. The
/// shares 2186/3118, 912/3118 and 20/3118 are 70.11 %, 29.25 % and 0.64 %; `~2,1f` drops the leading zero of a share
/// below 1 that the width of 2 leaves no room for (AP:552-554). Accounts without sub-accounts link to their
/// register (AP:481-482).
#[test]
fn test_data_assets_three_levels() {
    let mut s = Setup::fixture("test-data");
    let slices = [
        ("Bank - $2,186.00 (70.1%)", "$2,186.00"),
        ("GBP Bank - $912.00 (29.2%)", "$912.00"),
        ("Wallet - $20.00 (.6%)", "$20.00"),
    ];
    let pie = Pie {
        title: ["Assets", "Balance at 12/31/24: $3,118.00"],
        slices: &slices,
    };
    let chart = check_pie(
        &mut s,
        "three levels",
        "Assets",
        json!({ "Accounts": { "Levels of Subaccounts": 3 } }),
        &pie,
    );
    assert_eq!(
        chart["series"][0]["links"],
        json!([account_link(3), account_link(4), account_link(5)])
    );
    let long = [
        ("Root:Asset:Bank - $2,186.00 (70.1%)", "$2,186.00"),
        ("Root:Asset:GBP Bank - $912.00 (29.2%)", "$912.00"),
        ("Root:Asset:Wallet - $20.00 (.6%)", "$20.00"),
    ];
    let options = json!({ "Accounts": { "Levels of Subaccounts": 3 }, "Display": { "Show long names": true } });
    check_pie(
        &mut s,
        "long names",
        "Assets",
        options,
        &Pie {
            title: pie.title,
            slices: &long,
        },
    );
}

/// "Maximum Slices" 2: past it, the first slice stays and the rest become "Other", $912 + $20 = $932, 29.89 %
/// (AP:451-457). Its link reruns the report with every option copied and the accounts set to those folded
/// (AP:459-470), which then shows GBP Bank and Wallet alone: 912/932 is 97.85 %, 20/932 2.15 %.
#[test]
fn test_data_assets_other_slice() {
    let mut s = Setup::fixture("test-data");
    let options = json!({ "Accounts": { "Levels of Subaccounts": 3 }, "Display": { "Maximum Slices": 2 } });
    let slices = [("Bank - $2,186.00 (70.1%)", "$2,186.00"), ("Other - $932.00 (29.9%)", "$932.00")];
    let pie = Pie {
        title: ["Assets", "Balance at 12/31/24: $3,118.00"],
        slices: &slices,
    };
    let chart = check_pie(&mut s, "other", "Assets", options, &pie);
    let other = link(&chart, 1);
    let (template, accounts, levels, name) = report_link(&other);
    assert_eq!((template, accounts), (ASSETS, vec![acct(4).as_str(), acct(5).as_str()]));
    assert_eq!((levels, name), (&json!(3), &json!("Assets")), "the options are copied as they were");
    assert_eq!(other["report"]["options"]["Display"]["Maximum Slices"], json!(2));
    let slices = [("GBP Bank - $912.00 (97.9%)", "$912.00"), ("Wallet - $20.00 (2.1%)", "$20.00")];
    let pie = Pie {
        title: ["Assets", "Balance at 12/31/24: $932.00"],
        slices: &slices,
    };
    check_pie(&mut s, "the other slice's report", ASSETS, other["report"]["options"].clone(), &pie);
}

/// "Report's currency" GBP: Asset's collector holds $2,206 and #608 (AP:351-356); the dollars convert at the
/// nearest price, 1/(3/2), to #1,470.67, rounded to pence before adding (`gnc:sum-collector-commodity`), and GBP
/// prints with its user symbol `#`.
#[test]
fn test_data_assets_in_pounds() {
    let mut s = Setup::fixture("test-data");
    let pie = Pie {
        title: ["Assets", "Balance at 12/31/24: #2,078.67"],
        slices: &[("Asset - #2,078.67 (100.0%)", "#2,078.67")],
    };
    let chart = check_pie(
        &mut s,
        "in pounds",
        "Assets",
        json!({ "General": { "Report's currency": "CURRENCY::GBP" } }),
        &pie,
    );
    assert_eq!(chart["yAxis"]["commodity"], "CURRENCY::GBP");
}

/// Securities sums each selected account's own balance by its commodity (AP:257-283), ignoring depth: USD is Bank
/// and Wallet, $2,206 (70.75 %), GBP is GBP Bank, $912 (29.25 %). It has no links (AP:474).
#[test]
fn test_data_securities_defaults() {
    let mut s = Setup::fixture("test-data");
    let slices = [("USD - $2,206.00 (70.8%)", "$2,206.00"), ("GBP - $912.00 (29.2%)", "$912.00")];
    let pie = Pie {
        title: ["Securities", "Balance at 12/31/24: $3,118.00"],
        slices: &slices,
    };
    let chart = check_pie(&mut s, "securities", "Securities", json!({}), &pie);
    assert!(chart["series"][0].get("links").is_none(), "no drill-down by commodity");
}

/// The income, expense and liability pies find no top-level account of their types (AP:337-340): their accounts
/// are selected (AP:115-122) but nothing is walked, so they warn of no data (AP:472-473, :560-563).
#[test]
fn test_data_income_expense_and_liability_pies_have_no_data() {
    let mut s = Setup::fixture("test-data");
    ["Income Accounts", "Expense Accounts", "Liabilities"]
        .into_iter()
        .for_each(|t| check_warning(&mut s, t, t, json!({}), "No data"));
}

// ---------------------------------------------------------------------------------------------------------------
// balsheet-pnl: Root (ASSET) > Asset > Bank1 (> Savings $100, Bonds $2,000, Empty, Current), House $100,000,
// ForeignBank (> ForeignSavings, GBP), Broker $2,000 (> Funds, FUNDS); Liability, Equity and Income beside Asset.
// At the end of 2024 Current holds $3,029, ForeignSavings #700 and Funds 30 FUNDS; the nearest prices are USD in GBP
// 10/17 of 1980-02-01 (GBP $1.70: $1,190) and FUNDS 50005/104 of 1975 ($14,424.52 rounded, 1500150/104).
// ---------------------------------------------------------------------------------------------------------------

/// Defaults: Asset with its sub-accounts, $107,129 + $1,190 + $14,424.52 = $122,743.52.
#[test]
fn balsheet_assets_defaults() {
    let mut s = Setup::fixture("balsheet-pnl");
    let pie = Pie {
        title: ["Assets", "Balance at 2024-12-31: $122,743.52"],
        slices: &[("Asset - $122,743.52 (100.0%)", "$122,743.52")],
    };
    let chart = check_pie(&mut s, "defaults", "Assets", json!({}), &pie);
    let asset = link(&chart, 0);
    let (_, accounts, levels, _) = report_link(&asset);
    assert_eq!(accounts, [2, 3, 8, 9, 11].map(acct), "Asset and its children");
    assert_eq!(levels, &json!(3));
}

/// "Price Source" at 1971-01-01: Asset with its sub-accounts holds every asset of the book, so its slice is the
/// Balance Sheet's Total Assets, which GnuCash's own test checks for each source (`standard/test/
/// test-balsheet-pnl.scm:180`, `:192`, `:198`, `:204`, `:210`); the exchange function comes from the same
/// `gnc:case-exchange-fn` at the same end of day (AP:359-360). "Last up through report date" finds the same prices
/// as the nearest ones, GBP 150/101 and FUNDS 30001/100 of that morning.
#[test]
fn balsheet_assets_by_price_source() {
    let mut s = Setup::fixture("balsheet-pnl");
    let sources = [
        ("pricedb-nearest", "$116,006.33"),
        ("pricedb-before", "$116,006.33"),
        ("weighted-average", "$114,071.66"),
        ("average-cost", "$113,100.00"),
        ("pricedb-latest", "$121,473.52"),
    ];
    sources.into_iter().for_each(|(source, total)| {
        let options = json!({ "General": { "End Date": date("1971-01-01"), "Price Source": source } });
        let (title, label) = (format!("Balance at 1971-01-01: {total}"), format!("Asset - {total} (100.0%)"));
        check_pie(
            &mut s,
            source,
            "Assets",
            options,
            &Pie {
                title: ["Assets", &title],
                slices: &[(&label, total)],
            },
        );
    });
}

/// All levels: the tree is five deep (`gnc:get-current-account-tree-depth`, AP:361-363), so every account shows its
/// own balance. Of equal amounts, Broker comes before Bonds: the walk lists later siblings' data first (AP:237-244)
/// and the sort is stable. Shares: 81.47, 11.75, 2.47, 1.63, 1.63, 0.97 and 0.08 %.
#[test]
fn balsheet_assets_all_levels() {
    let mut s = Setup::fixture("balsheet-pnl");
    let slices = [
        ("House - $100,000.00 (81.5%)", "$100,000.00"),
        ("Funds - $14,424.52 (11.8%)", "$14,424.52"),
        ("Current - $3,029.00 (2.5%)", "$3,029.00"),
        ("Broker - $2,000.00 (1.6%)", "$2,000.00"),
        ("Bonds - $2,000.00 (1.6%)", "$2,000.00"),
        ("ForeignSavings - $1,190.00 (1.0%)", "$1,190.00"),
        ("Savings - $100.00 (.1%)", "$100.00"),
    ];
    let pie = Pie {
        title: ["Assets", "Balance at 2024-12-31: $122,743.52"],
        slices: &slices,
    };
    let chart = check_pie(
        &mut s,
        "all levels",
        "Assets",
        json!({ "Accounts": { "Levels of Subaccounts": "all" } }),
        &pie,
    );
    let leaves = [8, 12, 7, 11, 5, 10, 4].map(account_link);
    let mut links = chart["series"][0]["links"].as_array().unwrap().clone();
    let broker = links.remove(3);
    assert_eq!(report_link(&broker).1, [acct(11), acct(12)], "Broker has a sub-account");
    assert_eq!(report_link(&broker).2, &json!(6), "min 6 (tree depth + 1), AP:489-490");
    assert_eq!(links, [&leaves[..3], &leaves[4..]].concat());
}

/// All levels and "Maximum Slices" 2: House, then the six others as "Other", $22,743.52 (18.53 %). Its report keeps
/// "Maximum Slices" 2, so Funds (63.42 %) and another "Other" of $8,319.00 (36.58 %) follow.
#[test]
fn balsheet_assets_other_slice_and_its_report() {
    let mut s = Setup::fixture("balsheet-pnl");
    let options = json!({ "Accounts": { "Levels of Subaccounts": "all" }, "Display": { "Maximum Slices": 2 } });
    let slices = [
        ("House - $100,000.00 (81.5%)", "$100,000.00"),
        ("Other - $22,743.52 (18.5%)", "$22,743.52"),
    ];
    let pie = Pie {
        title: ["Assets", "Balance at 2024-12-31: $122,743.52"],
        slices: &slices,
    };
    let chart = check_pie(&mut s, "other", "Assets", options, &pie);
    let other = link(&chart, 1);
    assert_eq!(
        report_link(&other).1,
        [12, 7, 11, 5, 10, 4].map(acct),
        "the folded accounts, in slice order"
    );
    let slices = [
        ("Funds - $14,424.52 (63.4%)", "$14,424.52"),
        ("Other - $8,319.00 (36.6%)", "$8,319.00"),
    ];
    let pie = Pie {
        title: ["Assets", "Balance at 2024-12-31: $22,743.52"],
        slices: &slices,
    };
    check_pie(&mut s, "the other slice's report", ASSETS, other["report"]["options"].clone(), &pie);
}

/// Securities: USD $107,129 (87.28 %), FUNDS $14,424.52 (11.75 %), GBP $1,190 (0.97 %). Sorted by name the
/// mnemonics order FUNDS, GBP, USD (AP:194-205), and by code too, a security's code being its mnemonic.
#[test]
fn balsheet_securities() {
    let mut s = Setup::fixture("balsheet-pnl");
    let slices = [
        ("USD - $107,129.00 (87.3%)", "$107,129.00"),
        ("FUNDS - $14,424.52 (11.8%)", "$14,424.52"),
        ("GBP - $1,190.00 (1.0%)", "$1,190.00"),
    ];
    let pie = Pie {
        title: ["Securities", "Balance at 2024-12-31: $122,743.52"],
        slices: &slices,
    };
    check_pie(&mut s, "securities", "Securities", json!({}), &pie);
    let by_name = [slices[1], slices[2], slices[0]];
    ["alphabetical", "acct-code"].into_iter().for_each(|sort| {
        let options = json!({ "Display": { "Sort Method": sort } });
        check_pie(
            &mut s,
            sort,
            "Securities",
            options,
            &Pie {
                title: pie.title,
                slices: &by_name,
            },
        );
    });
    let slices = [
        ("USD - $107,129.00 (87.3%)", "$107,129.00"),
        ("Other - $15,614.52 (12.7%)", "$15,614.52"),
    ];
    let options = json!({ "Display": { "Maximum Slices": 2 } });
    let chart = check_pie(
        &mut s,
        "other",
        "Securities",
        options,
        &Pie {
            title: pie.title,
            slices: &slices,
        },
    );
    assert!(
        chart["series"][0].get("links").is_none(),
        "no report for the other securities (AP:458)"
    );
}

/// Securities at 1971-01-01 at the latest prices: the figures of the Balance Sheet's recursive run, $106,709 in
/// USD, #200 = $340.00 and 30 FUNDS = $14,424.52 (`standard/test/test-balsheet-pnl.scm:278-281`), total $121,473.52
/// (`:314`). Shares 87.85, 11.87 and 0.28 %.
#[test]
fn balsheet_securities_latest_1971() {
    let mut s = Setup::fixture("balsheet-pnl");
    let slices = [
        ("USD - $106,709.00 (87.8%)", "$106,709.00"),
        ("FUNDS - $14,424.52 (11.9%)", "$14,424.52"),
        ("GBP - $340.00 (.3%)", "$340.00"),
    ];
    let pie = Pie {
        title: ["Securities", "Balance at 1971-01-01: $121,473.52"],
        slices: &slices,
    };
    let options = json!({ "General": { "End Date": date("1971-01-01"), "Price Source": "pricedb-latest" } });
    check_pie(&mut s, "latest", "Securities", options, &pie);
}

/// Security long names are the commodities' full names (AP:173-176): ISO 4217's for the currencies, the book's for
/// FUNDS; sorted by name, those names order the slices.
#[test]
fn balsheet_securities_long_names() {
    let mut s = Setup::fixture("balsheet-pnl");
    let slices = [
        ("US Dollar - $107,129.00 (87.3%)", "$107,129.00"),
        ("Funds - $14,424.52 (11.8%)", "$14,424.52"),
        ("Pound Sterling - $1,190.00 (1.0%)", "$1,190.00"),
    ];
    let title = ["Securities", "Balance at 2024-12-31: $122,743.52"];
    check_pie(
        &mut s,
        "long names",
        "Securities",
        json!({ "Display": { "Show long names": true } }),
        &Pie { title, slices: &slices },
    );
    let by_name = [slices[1], slices[2], slices[0]];
    let options = json!({ "Display": { "Show long names": true, "Sort Method": "alphabetical" } });
    check_pie(
        &mut s,
        "long names by name",
        "Securities",
        options,
        &Pie { title, slices: &by_name },
    );
}

/// No top-level income or liability account: no data. No expense account at all: none selected (AP:441, :565-568).
#[test]
fn balsheet_income_expense_and_liability_pies() {
    let mut s = Setup::fixture("balsheet-pnl");
    check_warning(&mut s, "income", "Income Accounts", json!({}), "No data");
    check_warning(&mut s, "liabilities", "Liabilities", json!({}), "No data");
    check_warning(&mut s, "expenses", "Expense Accounts", json!({}), "No accounts selected");
}

// ---------------------------------------------------------------------------------------------------------------
// category-multi: Expenses > Groceries, Rent, Auto (> Tax, Parking, Petrol), all top-level EXPENSE, and Assets > Bank.
// Each of the seven expense accounts has day N of March 2024 at $N, so each holds $496 for the month.
// ---------------------------------------------------------------------------------------------------------------

fn march(extra: Value) -> Value {
    let mut options = json!({ "General": { "Start Date": date("2024-03-01"), "End Date": date("2024-03-31") } });
    merge(&mut options, extra);
    options
}

fn merge(into: &mut Value, extra: Value) {
    let Value::Object(extra) = extra else { return };
    extra.into_iter().for_each(|(section, values)| {
        let target = into.as_object_mut().unwrap().entry(section).or_insert_with(|| json!({}));
        values.as_object().unwrap().iter().for_each(|(k, v)| {
            target.as_object_mut().unwrap().insert(k.clone(), v.clone());
        });
    });
}

const MARCH: &str = "2024-03-01 to 2024-03-31";

/// Defaults: the accounting period, 2024 (AP:90-93). Two levels: Expenses' own $496, its children at the limit,
/// Auto with its sub-accounts $1,984 (57.14 %), the others $496 (14.29 %). The walk lists Groceries, Rent, Auto,
/// then Expenses (AP:229-244); the stable sort keeps Groceries, Rent and Expenses in that order. Auto and Expenses
/// have sub-accounts and link to their reports; Groceries and Rent to their registers.
#[test]
fn category_expenses_defaults() {
    let mut s = Setup::fixture("category-multi");
    let slices = [
        ("Auto - $1,984.00 (57.1%)", "$1,984.00"),
        ("Groceries - $496.00 (14.3%)", "$496.00"),
        ("Rent - $496.00 (14.3%)", "$496.00"),
        ("Expenses - $496.00 (14.3%)", "$496.00"),
    ];
    let pie = Pie {
        title: ["Expense Accounts", "2024-01-01 to 2024-12-31: $3,472.00"],
        slices: &slices,
    };
    let chart = check_pie(&mut s, "defaults", "Expense Accounts", json!({}), &pie);
    let auto = link(&chart, 0);
    let (template, accounts, levels, name) = report_link(&auto);
    assert_eq!(accounts, [4, 5, 6, 7].map(acct), "Auto and its children");
    assert_eq!((template, levels, name), (EXPENSE, &json!(3), &json!("Auto")));
    assert_eq!((link(&chart, 1), link(&chart, 2)), (account_link(2), account_link(3)));
    assert_eq!(report_link(&link(&chart, 3)).1, [1, 2, 3, 4].map(acct));
}

/// Auto's link: the report on Auto and its children, named "Auto", three levels. Auto is at depth 2 and shows its
/// own $496; Tax, Parking and Petrol, at the limit, theirs: four quarters.
#[test]
fn category_expenses_drill_down() {
    let mut s = Setup::fixture("category-multi");
    let chart = s.block(EXPENSE, &march(json!({})));
    let options = link(&chart, 0)["report"]["options"].clone();
    let slices = [
        ("Tax - $496.00 (25.0%)", "$496.00"),
        ("Parking - $496.00 (25.0%)", "$496.00"),
        ("Petrol - $496.00 (25.0%)", "$496.00"),
        ("Auto - $496.00 (25.0%)", "$496.00"),
    ];
    check_pie(
        &mut s,
        "auto",
        EXPENSE,
        options,
        &Pie {
            title: ["Auto", &format!("{MARCH}: $1,984.00")],
            slices: &slices,
        },
    );
}

/// "Levels of Subaccounts" 1 and 3. At one level Expenses shows everything below it. At three every account shows
/// its own $496, in the walk's order: Auto's children, Auto, Rent, Groceries, then Expenses.
#[test]
fn category_expenses_levels() {
    let mut s = Setup::fixture("category-multi");
    let total = format!("{MARCH}: $3,472.00");
    let pie = Pie {
        title: ["Expense Accounts", &total],
        slices: &[("Expenses - $3,472.00 (100.0%)", "$3,472.00")],
    };
    let chart = check_pie(
        &mut s,
        "one level",
        "Expense Accounts",
        march(json!({ "Accounts": { "Levels of Subaccounts": 1 } })),
        &pie,
    );
    assert_eq!(report_link(&link(&chart, 0)).2, &json!(2));
    let names = ["Tax", "Parking", "Petrol", "Auto", "Rent", "Groceries", "Expenses"];
    let labels: Vec<String> = names.iter().map(|n| format!("{n} - $496.00 (14.3%)")).collect();
    let slices: Vec<(&str, &str)> = labels.iter().map(|l| (l.as_str(), "$496.00")).collect();
    let three = json!({ "Accounts": { "Levels of Subaccounts": 3 } });
    let pie = Pie {
        title: ["Expense Accounts", &total],
        slices: &slices,
    };
    check_pie(&mut s, "three levels", "Expense Accounts", march(three.clone()), &pie);
    let mut other = three;
    merge(&mut other, json!({ "Display": { "Maximum Slices": 2 } }));
    let slices = [("Tax - $496.00 (14.3%)", "$496.00"), ("Other - $2,976.00 (85.7%)", "$2,976.00")];
    check_pie(
        &mut s,
        "other",
        "Expense Accounts",
        march(other),
        &Pie {
            title: ["Expense Accounts", &total],
            slices: &slices,
        },
    );
}

/// "Sort Method": by code, which every account leaves empty, the stable sort keeps the walk's order; by name,
/// bytewise (AP:180-191); with long names, by full name.
#[test]
fn category_expenses_sort_methods() {
    let mut s = Setup::fixture("category-multi");
    let total = format!("{MARCH}: $3,472.00");
    let label = |n: &str, amount: &str, share: &str| format!("{n} - {amount} ({share}%)");
    let (auto, groceries, rent, expenses) = (
        label("Auto", "$1,984.00", "57.1"),
        label("Groceries", "$496.00", "14.3"),
        label("Rent", "$496.00", "14.3"),
        label("Expenses", "$496.00", "14.3"),
    );
    let by_code = [
        (groceries.as_str(), "$496.00"),
        (rent.as_str(), "$496.00"),
        (auto.as_str(), "$1,984.00"),
        (expenses.as_str(), "$496.00"),
    ];
    let options = march(json!({ "Display": { "Sort Method": "acct-code" } }));
    check_pie(
        &mut s,
        "by code",
        "Expense Accounts",
        options,
        &Pie {
            title: ["Expense Accounts", &total],
            slices: &by_code,
        },
    );
    let by_name = [by_code[2], by_code[3], by_code[0], by_code[1]];
    let options = march(json!({ "Display": { "Sort Method": "alphabetical" } }));
    check_pie(
        &mut s,
        "by name",
        "Expense Accounts",
        options,
        &Pie {
            title: ["Expense Accounts", &total],
            slices: &by_name,
        },
    );
    let auto = [
        "Expenses:Auto",
        "Expenses:Auto:Parking",
        "Expenses:Auto:Petrol",
        "Expenses:Auto:Tax",
    ];
    let full = [&["Expenses"][..], &auto[..], &["Expenses:Groceries", "Expenses:Rent"][..]].concat();
    let labels: Vec<String> = full.iter().map(|n| label(n, "$496.00", "14.3")).collect();
    let slices: Vec<(&str, &str)> = labels.iter().map(|l| (l.as_str(), "$496.00")).collect();
    let display = json!({ "Sort Method": "alphabetical", "Show long names": true });
    let options = march(json!({ "Accounts": { "Levels of Subaccounts": 3 }, "Display": display }));
    check_pie(
        &mut s,
        "by full name",
        "Expense Accounts",
        options,
        &Pie {
            title: ["Expense Accounts", &total],
            slices: &slices,
        },
    );
}

/// "Show Average" (AP:365-386): the amounts times one over the span from the start to just after the end, in
/// exact fractions (`5.17:bindings/guile/date-utilities.scm:194-251`). March 2024 is one month, 31/366 of a year
/// (×366/31: $1,984 → $23,424, $496 → $5,856) and 31/7 weeks (×7/31: $448 and $112). The title says which.
#[test]
fn category_expenses_averages() {
    let mut s = Setup::fixture("category-multi");
    let cases = [
        (
            "MonthDelta",
            "Expense Accounts Monthly Average",
            "$3,472.00",
            "$1,984.00",
            "$496.00",
        ),
        (
            "YearDelta",
            "Expense Accounts Yearly Average",
            "$40,992.00",
            "$23,424.00",
            "$5,856.00",
        ),
        ("WeekDelta", "Expense Accounts Weekly Average", "$784.00", "$448.00", "$112.00"),
    ];
    cases.into_iter().for_each(|(average, title, total, auto, other)| {
        let labels = [
            format!("Auto - {auto} (57.1%)"),
            format!("Groceries - {other} (14.3%)"),
            format!("Rent - {other} (14.3%)"),
            format!("Expenses - {other} (14.3%)"),
        ];
        let slices = [
            (labels[0].as_str(), auto),
            (labels[1].as_str(), other),
            (labels[2].as_str(), other),
            (labels[3].as_str(), other),
        ];
        let line = format!("{MARCH}: {total}");
        let options = march(json!({ "General": { "Show Average": average } }));
        check_pie(
            &mut s,
            average,
            "Expense Accounts",
            options,
            &Pie {
                title: [title, &line],
                slices: &slices,
            },
        );
    });
}

/// "Show Totals" off leaves the amounts out of the title and labels; "Show Percents" off the shares (AP:517-525,
/// :543-555).
#[test]
fn category_expenses_legend_options() {
    let mut s = Setup::fixture("category-multi");
    let no_totals = [
        ("Auto (57.1%)", "$1,984.00"),
        ("Groceries (14.3%)", "$496.00"),
        ("Rent (14.3%)", "$496.00"),
        ("Expenses (14.3%)", "$496.00"),
    ];
    let options = march(json!({ "Display": { "Show Totals": false } }));
    check_pie(
        &mut s,
        "no totals",
        "Expense Accounts",
        options,
        &Pie {
            title: ["Expense Accounts", MARCH],
            slices: &no_totals,
        },
    );
    let no_percents = [
        ("Auto - $1,984.00", "$1,984.00"),
        ("Groceries - $496.00", "$496.00"),
        ("Rent - $496.00", "$496.00"),
        ("Expenses - $496.00", "$496.00"),
    ];
    let options = march(json!({ "Display": { "Show Percents": false } }));
    let total = format!("{MARCH}: $3,472.00");
    check_pie(
        &mut s,
        "no percents",
        "Expense Accounts",
        options,
        &Pie {
            title: ["Expense Accounts", &total],
            slices: &no_percents,
        },
    );
}

/// "Accounts": Groceries alone; the walk passes Expenses, which is not selected (AP:344-345). The period: 10 to 12
/// March, $10 + $11 + $12 = $33 an account, $132 for Auto with its children. "Plot Width" and "Plot Height" size
/// the chart (AP:526-527).
#[test]
fn category_expenses_accounts_period_and_size() {
    let mut s = Setup::fixture("category-multi");
    let options = march(json!({ "Accounts": { "Accounts": [acct(2)] } }));
    let pie = Pie {
        title: ["Expense Accounts", &format!("{MARCH}: $496.00")],
        slices: &[("Groceries - $496.00 (100.0%)", "$496.00")],
    };
    check_pie(&mut s, "groceries", "Expense Accounts", options, &pie);
    let options = json!({ "General": { "Start Date": date("2024-03-10"), "End Date": date("2024-03-12") } });
    let slices = [
        ("Auto - $132.00 (57.1%)", "$132.00"),
        ("Groceries - $33.00 (14.3%)", "$33.00"),
        ("Rent - $33.00 (14.3%)", "$33.00"),
        ("Expenses - $33.00 (14.3%)", "$33.00"),
    ];
    let pie = Pie {
        title: ["Expense Accounts", "2024-03-10 to 2024-03-12: $231.00"],
        slices: &slices,
    };
    check_pie(&mut s, "three days", "Expense Accounts", options, &pie);
    let chart = s.block(
        "Expense Accounts",
        &json!({ "Display": { "Plot Width": { "pixels": 400 }, "Plot Height": { "percent": 50 } } }),
    );
    assert_eq!(
        (chart["width"].clone(), chart["height"].clone()),
        (json!({ "pixels": 400 }), json!({ "percent": 50 }))
    );
}

/// Nothing in 2023: no data, under the averaged title. No income account exists, so none is selected.
#[test]
fn category_warnings() {
    let mut s = Setup::fixture("category-multi");
    let options = json!({ "General": { "Start Date": date("2023-01-01"), "End Date": date("2023-12-31"), "Show Average": "YearDelta" } });
    check_warning(&mut s, "2023", "Expense Accounts", options.clone(), "No data");
    assert_eq!(s.run("Expense Accounts", &options)["title"], "Expense Accounts Yearly Average");
    check_warning(&mut s, "income", "Income Accounts", json!({}), "No accounts selected");
}

// ---------------------------------------------------------------------------------------------------------------
// category-liability: top-level account-1 (ASSET), account-2 (CREDIT) and account-3 (INCOME); a $100 loan on
// 2016-10-01, then on the 1st of each month January to May 2017 $10 of income and on the 2nd a $9 repayment.
// ---------------------------------------------------------------------------------------------------------------

/// Liabilities negate the balance (AP:432-437, :640): -$100 + 5 × $9 = $55 owed; on 1 March 2017, after two
/// repayments, $82.
#[test]
fn category_liabilities() {
    let mut s = Setup::fixture("category-liability");
    let pie = Pie {
        title: ["Liabilities", "Balance at 2024-12-31: $55.00"],
        slices: &[("account-2 - $55.00 (100.0%)", "$55.00")],
    };
    let chart = check_pie(&mut s, "defaults", "Liabilities", json!({}), &pie);
    assert_eq!(link(&chart, 0), account_link(2));
    let pie = Pie {
        title: ["Liabilities", "Balance at 2017-03-01: $82.00"],
        slices: &[("account-2 - $82.00 (100.0%)", "$82.00")],
    };
    check_pie(
        &mut s,
        "march 2017",
        "Liabilities",
        json!({ "General": { "End Date": date("2017-03-01") } }),
        &pie,
    );
}

/// Income negates too (AP:605): $50 in 2017; a monthly average over twelve months, $4.17 (50/12 rounded); a weekly
/// one over 365/7 weeks, $0.96 (350/365). In the default period, 2024, there is none.
#[test]
fn category_income() {
    let mut s = Setup::fixture("category-liability");
    let year = json!({ "General": { "Start Date": date("2017-01-01"), "End Date": date("2017-12-31") } });
    let line = "2017-01-01 to 2017-12-31";
    let cases = [
        ("None", "Income Accounts", "$50.00"),
        ("MonthDelta", "Income Accounts Monthly Average", "$4.17"),
        ("WeekDelta", "Income Accounts Weekly Average", "$0.96"),
        ("YearDelta", "Income Accounts Yearly Average", "$50.00"),
    ];
    cases.into_iter().for_each(|(average, title, amount)| {
        let mut options = year.clone();
        merge(&mut options, json!({ "General": { "Show Average": average } }));
        let (line, label) = (format!("{line}: {amount}"), format!("account-3 - {amount} (100.0%)"));
        check_pie(
            &mut s,
            average,
            "Income Accounts",
            options,
            &Pie {
                title: [title, &line],
                slices: &[(&label, amount)],
            },
        );
    });
    check_warning(&mut s, "2024", "Income Accounts", json!({}), "No data");
}

// ---------------------------------------------------------------------------------------------------------------
// Every template
// ---------------------------------------------------------------------------------------------------------------

/// The templates keep 5.17's names, GUIDs, menus and tips (AP:36-63, :573-642).
#[test]
fn templates_keep_gnucash_identities() {
    let mut s = Setup::fixture("test-data");
    let list = s.call("report.list", json!({}));
    let menus = [
        (INCOME, "Income Accounts", "_Income & Expense", "Income Piechart"),
        (EXPENSE, "Expense Accounts", "_Income & Expense", "Expense Piechart"),
        (ASSETS, "Assets", "_Assets & Liabilities", "Asset Piechart"),
        (SECURITIES, "Securities", "_Assets & Liabilities", "Security Piechart"),
        (LIABILITIES, "Liabilities", "_Assets & Liabilities", "Liability Piechart"),
    ];
    menus.into_iter().for_each(|(guid, name, menu, menu_name)| {
        let t = list["reports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["guid"] == guid)
            .unwrap_or_else(|| panic!("{name}: {list}"));
        assert_eq!(
            (t["name"].as_str(), t["menuPath"].clone(), t["menuName"].as_str()),
            (Some(name), json!([menu]), Some(menu_name))
        );
        assert_eq!(t["script"], "standard/account-piecharts.rhai");
    });
}

/// The options, in GnuCash's order (sections by name, options by key), with 5.17's keys, types and defaults
/// (AP:87-163).
#[test]
fn options_follow_gnucash() {
    let mut s = Setup::fixture("category-multi");
    let described = |s: &mut Setup, report: &str| -> Vec<(String, String, String, Value)> {
        let options = s.call("report.options", json!({ "report": report }));
        options["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    format!("{}/{}", o["section"].as_str().unwrap(), o["name"].as_str().unwrap()),
                    o["key"].as_str().unwrap().to_owned(),
                    o["type"].as_str().unwrap().to_owned(),
                    o["default"].clone(),
                )
            })
            .collect()
    };
    let expense = described(&mut s, "Expense Accounts");
    let names: Vec<&str> = expense.iter().map(|(n, ..)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "Accounts/Accounts",
            "Accounts/Levels of Subaccounts",
            "Display/Show long names",
            "Display/Show Totals",
            "Display/Show Percents",
            "Display/Maximum Slices",
            "Display/Plot Width",
            "Display/Plot Height",
            "Display/Sort Method",
            "General/Report name",
            "General/Stylesheet",
            "General/Add options summary",
            "General/Start Date",
            "General/End Date",
            "General/Report's currency",
            "General/Price Source",
            "General/Show Average",
        ]
    );
    let find = |list: &[(String, String, String, Value)], name: &str| list.iter().find(|(n, ..)| n == name).cloned().unwrap();
    let every = json!([1, 4, 6, 7, 5, 2, 3].map(acct));
    assert_eq!(
        find(&expense, "Accounts/Accounts").3,
        every,
        "every expense account in display order (AP:115-122)"
    );
    assert_eq!(find(&expense, "Accounts/Levels of Subaccounts").3, json!(2));
    assert_eq!(find(&expense, "Display/Maximum Slices").2, "numberRange");
    assert_eq!(find(&expense, "Display/Sort Method").3, json!("amount"));
    assert_eq!(find(&expense, "General/Price Source").3, json!("pricedb-nearest"));
    assert_eq!(
        find(&expense, "General/Show Average"),
        ("General/Show Average".into(), "f".into(), "multichoice".into(), json!("None"))
    );
    assert_eq!(find(&expense, "General/Start Date").1, "aa");
    let securities = described(&mut s, "Securities");
    let absent = ["Accounts/Levels of Subaccounts", "General/Show Average", "General/Start Date"];
    assert!(
        securities.iter().all(|(n, ..)| !absent.contains(&n.as_str())),
        "no depth and no interval (AP:90-128)"
    );
    assert_eq!(find(&securities, "General/End Date").1, "a", "a report date (AP:94-96)");
}

/// Running every template on every fixture leaves the Book as it was, not dirty, with no file written beside it;
/// two exports give the same bytes, and the HTML holds no script. The charts fixtures stand for GnuCash's only
/// test of these reports, that each renders on that book (`standard/test/test-charts.scm:189`).
#[test]
fn every_template_reads_only_and_exports_deterministically() {
    FIXTURES.iter().for_each(|(fixture, _)| {
        let mut s = Setup::fixture(fixture);
        let before = s.d.context().book().unwrap().clone();
        let files = s.files();
        let all_levels = json!({ "Accounts": { "Levels of Subaccounts": "all" } });
        let runs = TEMPLATES
            .iter()
            .flat_map(|t| [(*t, json!({})), (*t, if *t == SECURITIES { json!({}) } else { all_levels.clone() })]);
        runs.for_each(|(t, options)| {
            s.run(t, &options);
            let (a, b) = (s.export(t, &options), s.export(t, &options));
            assert_eq!(a, b, "{fixture} {t}: two exports differ");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{fixture} {t}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{fixture}: the Book is unchanged");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{fixture}");
        assert_eq!(s.files(), files, "{fixture}: no journal or other file was written");
    });
}
