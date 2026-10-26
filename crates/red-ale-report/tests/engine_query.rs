//! Every function of the query API (`design/reports.md` §5) on `fixtures/reports/balsheet-pnl.gnucash`, the book of
//! `5.17:gnucash/report/reports/standard/test/test-balsheet-pnl.scm:92-166` (built by
//! `scripts/report-fixtures/balsheet_pnl.py`). The expected values are worked out from that script: transactions
//! post at 10:59 UTC, each foreign transfer adds a price at midnight UTC of its day, and `check_book` there confirms
//! the balances in GnuCash 5.17.

mod common;

use common::{GUID, Host, fixture, registry};
use red_ale_core::ops::period::{AccountingPeriod, PeriodBound};
use red_ale_core::{Date, TimeZone};
use red_ale_report::settings::ReverseBalanced;
use serde_json::json;

/// Each line is `label=value`; the test compares the log line by line.
const QUERY: &str = r#"
fn render(params, book, opts, ctx) {
    let t1970_03_15 = 6307200; let t1970_06_20 = 14688000; let t1971 = 31536000; let t1971_06 = 44582400;
    let t1980 = 315532800; let t1980_03 = 320716800; let t1980_end = 347155199;
    let current = book.account("Root:Asset:Bank1:Current");
    let bank1 = book.account("Root:Asset:Bank1");
    let funds = book.account("Root:Asset:Broker:Funds");
    let income = book.account("Root:Income");
    let income_gbp = book.account("Root:Income:Income-GBP");
    let names = |list| list.map(|a| a.name).reduce(|a, b| a + "," + b, "");

    // Book
    print(`root=${book.root().name} ${book.root().type} ${book.root().parent} ${book.root().depth}`);
    print(`account-by-guid=${book.account(current.guid).fullName} missing=${book.account("No:Such")}`);
    print(`accounts=${book.accounts().len()} first=${book.accounts()[0].name}`);
    print(`stock=${names.call(book.accounts_of_types(["STOCK"]))} liabilities=${names.call(book.accounts_of_types(["LIABILITY"]))}`);
    let fund = book.commodity("FUNDS::FUNDS");
    print(`commodity=${fund.id} ${fund.namespace} ${fund.mnemonic} ${fund.fullname} ${fund.fraction} ${fund.isCurrency} ${fund.symbol} ${fund.uniqueName}`);
    let gbp = book.commodity("GBP");
    print(`gbp=${gbp.id} ${gbp.symbol} ${gbp.fraction} ${gbp.isCurrency} missing=${book.commodity("XYZ::NONE")}`);
    print(`trading=${book.uses_trading_accounts()} company=[${book.company_name()}]`);
    let prices = book.prices(fund, "USD");
    let p = prices[0];
    print(`prices=${prices.len()} newest=${p.value} ${p.time} ${p.commodity.id} ${p.currency.id} ${p.source} ${p.type} oldest=${prices[-1].value}`);
    print(`rate-before=${book.price_rate(fund, "USD", t1971_06, "before")} inverted=${book.price_rate("USD", fund, t1971_06, "before")}`);
    print(`rate-nearest=${book.price_rate(fund, "USD", t1970_06_20, "nearest")} latest=${book.price_rate(fund, "USD", 0, "latest")}`);
    print(`rate-same=${book.price_rate("USD", "USD", 0, "before")} none=${book.price_rate(fund, "GBP", -86400, "before")}`);
    print(`rate-gbp=${book.price_rate("GBP", "USD", t1980_03, "before")}`);
    let converted = book.price_convert(monetary(fund, 30), "USD", t1971_06, "before");
    print(`convert=${converted.amount} ${converted.commodity.id} ${converted}`);
    let all = book.root().descendants_sorted();
    print(`commodity-splits=${book.commodity_splits(all, (), fund, false).len()} until=${book.commodity_splits(all, t1970_03_15, "FUNDS::FUNDS", true).len()}`);
    print(`gbp-splits=${book.commodity_splits([book.account("Root:Asset:ForeignBank:ForeignSavings")], (), (), false).len()}`);
    let q = book.split_query(#{ accounts: [income, income_gbp], from: t1980, to: t1980_end });
    let earn = book.split_query(#{ accounts: [income, income_gbp], from: t1980, to: t1980_end,
                                    description: #{ text: "EARN", cased: false, regexp: false } });
    let cased = book.split_query(#{ accounts: [income_gbp], description: #{ text: "EARN", cased: true, regexp: false } });
    let regexp = book.split_query(#{ accounts: [income, income_gbp], description: #{ text: "^ear.*GBP", cased: false, regexp: true } });
    print(`split-query=${q.len()} earn=${earn.len()} cased=${cased.len()} regexp=${regexp.len()} none=${book.split_query(#{}).len()}`);
    print(`euro=${is_euro_currency("EUR")} ${is_euro_currency(gbp)} ${euro_convert(monetary("DEM", 100), "EUR")} ${euro_convert(monetary("USD", 1), "EUR")}`);

    // Account
    print(`current=${current.name}|${current.fullName}|${current.type}|${current.code}|${current.description}|${current.notes}|${current.placeholder}|${current.hidden}|${current.commodity.id}|${current.parent.name}|${current.depth}`);
    print(`children=${names.call(bank1.children())} sorted=${names.call(bank1.children_sorted())}`);
    print(`descendants=${book.root().descendants().len()} sorted-first=${names.call(book.account("Root:Asset").descendants_sorted())}`);
    print(`tree-depth=${bank1.tree_depth()} ${current.tree_depth()} ${book.root().tree_depth()}`);
    print(`splits=${funds.splits().len()} between=${funds.splits_between(t1970_03_15, t1971).len()} open=${funds.splits_between((), t1970_03_15).len()} with-children=${book.account("Root:Asset:Broker").splits_between((), (), true).len()}`);
    print(`balance-as-of=${current.balance_as_of(t1971)} ${current.balance_as_of(t1980_03)} ${income.balance_as_of(t1980_end + 1)} ${income_gbp.balance_as_of(t1980_end + 1)}`);
    print(`accumulate=${funds.accumulate_at([-86400, t1970_03_15, t1980], "balance")} ${funds.accumulate_at([t1980], "noclosingBalance")}`);
    print(`reverse=${income.reverse_balance(ctx)} ${current.reverse_balance(ctx)}`);

    // Split and Transaction
    let s = funds.splits()[0];
    let t = s.transaction;
    print(`split=${s.account.name} ${s.amount} ${s.value} [${s.memo}] [${s.action}] ${s.reconcile} ${s.reconcileDate} ${s.sharePrice} ${s.balance} ${s.noclosingBalance}`);
    print(`balances=${funds.splits().map(|x| x.balance).reduce(|a, b| a + "," + b, "")}`);
    print(`transaction=${t.description}|${t.currency.id}|${t.datePosted}|${t.num}|${t.notes}|${t.isClosing}|${t.isVoid}|${t.splitCount}|${t.splits().len()}`);
    print(`same=${t.splits().contains(s)} ${t == s.transaction} ${current == book.account(current.guid)} ${current != funds}`);
    #{ title: "query", sections: [] }
}
"#;

#[test]
fn every_query_function_reads_the_fixture() {
    let script = format!("fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Query\" }}] }}\nfn options(p, o, b) {{}}\n{QUERY}");
    let reg = registry(&[("query.rhai", &script)]);
    let mut host = Host::new(fixture("balsheet-pnl.gnucash"));
    host.settings.reverse_balanced_accounts = ReverseBalanced::Credit;
    let result = host.run(&reg, GUID, &json!({})).unwrap();
    let lines: Vec<String> = result.log.into_iter().map(|l| l.text).collect();
    let want = [
        "root=Root Account ROOT  0",
        "account-by-guid=Root:Asset:Bank1:Current missing=",
        "accounts=19 first=Root",
        "stock=,Funds liabilities=,Liability,Bank2,CreditCard,Loan",
        "commodity=FUNDS::FUNDS FUNDS FUNDS Funds 1000 false FUNDS FUNDS::FUNDS",
        "gbp=CURRENCY::GBP # 100 true missing=",
        "trading=false company=[]",
        "prices=12 newest=50005/104 157766400 FUNDS::FUNDS CURRENCY::USD user:xfer-dialog test oldest=200",
        "rate-before=30001/100 inverted=100/30001",
        "rate-nearest=250 latest=50005/104",
        "rate-same=1 none=0",
        "rate-gbp=17/10",
        "convert=90003/10 CURRENCY::USD $9,000.30",
        "commodity-splits=7 until=3",
        "gbp-splits=10",
        "split-query=3 earn=1 cased=0 regexp=1 none=0",
        "euro=true false €51.13 ",
        "current=Current|Root:Asset:Bank1:Current|ASSET||||false|false|CURRENCY::USD|Bank1|4",
        "children=,Savings,Bonds,Empty,Current sorted=,Bonds,Current,Empty,Savings",
        "descendants=19 sorted-first=,Bank1,Bonds,Current,Empty,Savings,Broker,Funds,ForeignBank,ForeignSavings,House",
        "tree-depth=2 1 5",
        "splits=7 between=4 open=3 with-children=8",
        "balance-as-of=2609 3029 -250 -600",
        r#"accumulate=[(), rational("30"), rational("30")] [rational("30")]"#,
        "reverse=true false",
        "split=Funds 10 2000 [] [] n  200 10 10",
        "balances=,10,20,30,40,30,20,30",
        "transaction=buy 10FUND at $200|CURRENCY::USD|39540|||false|false|2|2",
        "same=true true true true",
    ];
    assert_eq!(lines, want);
}

#[test]
fn time_follows_the_local_zone() {
    let script = format!(
        "fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Zone\" }}] }}\nfn options(p, o, b) {{}}\n\
         fn render(params, book, opts, ctx) {{ let l = ctx.local(ctx.now); let p = ctx.accounting_period();\n\
         #{{ title: `${{l.day}} ${{l.hour}} ${{ctx.start_of_day(ctx.now)}} ${{ctx.print_date(ctx.now)}} ${{p.start}} ${{p.end}}`, sections: [] }} }}"
    );
    let reg = registry(&[("zone.rhai", &script)]);
    let mut host = Host::new(fixture("balsheet-pnl.gnucash"));
    // UTC+14: 2024-03-15 12:00 UTC is 02:00 on 16 March.
    host.zone = TimeZone::fixed(14 * 3600).unwrap();
    let march = Date::from_ymd(2024, 3, 1).unwrap();
    host.period = AccountingPeriod { start: PeriodBound::Absolute(march), end: PeriodBound::Absolute(march.end_of_month()) };
    let title = host.run(&reg, GUID, &json!({})).unwrap().title;
    let day_start = 1_710_504_000 - 2 * 3600;
    assert_eq!(title, format!("16 2 {day_start} 03/16/24 {} {}", 1_709_251_200 - 14 * 3600, 1_711_929_599 - 14 * 3600));
}
