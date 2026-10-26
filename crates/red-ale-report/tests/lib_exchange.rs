//! `lib/exchange` (`5.17:gnucash/report/commodity-utilities.scm:64-898`, CU) on
//! `fixtures/reports/balsheet-pnl.gnucash`, the book of `standard/test/test-balsheet-pnl.scm:92-166`, and on
//! hand-made lists.
//!
//! The book's exchanges (`scripts/report-fixtures/balsheet_pnl.py`): in 1970 ten GBP transfers in USD, 1,000 GBP for
//! $1,499 in absolute amounts and +200 GBP for $291 net, and seven FUNDS trades in USD, 70 units for $16,480 absolute
//! and +30 for $6,100 net; on 1980-02-01 a GBP transaction paying 100 GBP into the USD account Current for $170. Its
//! cross-commodity split is Current's (GBP transaction, USD account), which display order puts before Funds and
//! ForeignSavings. Each transfer also adds a price, and 1971-1975 add yearly prices. GnuCash's own expected totals
//! for each price source are the Balance Sheet's at 1971-01-01 (`test-balsheet-pnl.scm:178-211`) and the Income
//! Statement's revenue for 1980 (`:362-405`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{Host, fixture};
use libs::{check, lines};

const PRELUDE: &str = r#"
import "lib/exchange" as exchange;
import "lib/collectors" as collectors;
let usd = book.commodity("USD"); let gbp = book.commodity("GBP"); let funds = book.commodity("FUNDS::FUNDS");
let eur = monetary("EUR", 0).commodity; let dem = monetary("DEM", 0).commodity; let frf = monetary("FRF", 0).commodity;
let end_of = |y, m, d| ctx.make_time(#{ year: y, month: m, day: d, hour: 23, minute: 59, second: 59 });
let at = |y, m, d| ctx.make_time(#{ year: y, month: m, day: d, hour: 10, minute: 59 });
let names = |alist| alist.map(|e| `${e[0].mnemonic}=${e[1]}`);
let assets = book.accounts_of_types(["ASSET", "BANK", "CASH", "STOCK", "MUTUAL", "CURRENCY"]);
"#;

fn run(body: &str) -> Vec<String> {
    lines(&Host::new(fixture("balsheet-pnl.gnucash")), &format!("{PRELUDE}\n{body}"))
}

/// The Balance Sheet's Total Assets: every asset account's balance at the report date, summed into USD by the
/// price source's exchange function (`standard/balance-sheet.scm:336-342`, `:311-313`).
#[test]
fn case_exchange_fn_gives_gnucash_total_assets_for_each_price_source() {
    let got = run(r#"
        let report_date = end_of.call(1971, 1, 1);
        let balance = collectors::monetaries_add(assets.map(|a| monetary(a.commodity, a.balance_as_of(report_date))));
        for source in ["pricedb-nearest", "weighted-average", "average-cost", "pricedb-latest", "pricedb-before", "no-such"] {
            let fx = exchange::case_exchange_fn(book, source, usd, report_date);
            print(`${source}=${collectors::sum_collector_commodity(balance, usd, fx)}`);
        }
        // A second before the 1971 prices: the latest price before is the 1970 transfers', the nearest the 1971 one.
        let before_1971 = end_of.call(1970, 12, 31);
        for source in ["pricedb-before", "pricedb-nearest"] {
            let fx = exchange::case_exchange_fn(book, source, usd, before_1971);
            print(`${source} 1970=${collectors::sum_collector_commodity(balance, usd, fx)}`);
        }
    "#);
    check(&got, &[
        "pricedb-nearest=$116,006.33",
        "weighted-average=$114,071.66",
        "average-cost=$113,100.00",
        "pricedb-latest=$121,473.52",
        "pricedb-before=$116,006.33",
        // GnuCash falls back to the nearest price for a source it does not know (CU:786-795).
        "no-such=$116,006.33",
        // $106,709 + 200 × $1.65 + 30 × $250.
        "pricedb-before 1970=$114,539.00",
        "pricedb-nearest 1970=$116,006.33",
    ]);
}

/// The Income Statement's revenue for 1980 at each price source: −$250 and −600 GBP, the GBP at $1.70 by the
/// price database, at the 1,100 GBP for $1,669 of every exchange for the weighted average, and at the net 100 GBP for
/// $121 for the average cost (`test-balsheet-pnl.scm:373-405`).
#[test]
fn revenue_follows_the_1980_exchange() {
    let got = run(r#"
        let from = ctx.make_time(#{ year: 1980, month: 1, day: 1, hour: 0, minute: 0 });
        let to = end_of.call(1981, 1, 1);
        let income = book.account("Root:Income");
        let total = collectors::monetaries_add(([income] + income.descendants_sorted())
            .map(|a| monetary(a.commodity, a.balance_as_of(to) - a.balance_as_of(from))));
        print(`income=${total.format(|c, a| monetary(c, a).to_string())}`);
        for source in ["pricedb-nearest", "weighted-average", "average-cost", "pricedb-latest"] {
            print(`${source}=${collectors::sum_collector_commodity(total, usd, exchange::case_exchange_fn(book, source, usd, to))}`);
        }
        print(`totals=${names.call(exchange::get_exchange_totals(book, usd, to))}`);
        print(`cost-totals=${names.call(exchange::get_exchange_cost_totals(book, usd, to))}`);
        print(`alist=${names.call(exchange::make_exchange_alist(book, usd, to))}`);
        print(`cost-alist=${names.call(exchange::make_exchange_cost_alist(book, usd, to))}`);
        print(`alist-1971=${names.call(exchange::make_exchange_alist(book, usd, end_of.call(1971, 1, 1)))}`);
        print(`cost-alist-1971=${names.call(exchange::make_exchange_cost_alist(book, usd, end_of.call(1971, 1, 1)))}`);
        print(`in-gbp=${names.call(exchange::make_exchange_alist(book, gbp, to))}`);
    "#);
    check(&got, &[
        "income=[\"-#600.00\", \"-$250.00\"]",
        "pricedb-nearest=-$1,270.00",
        "weighted-average=-$1,160.36",
        "average-cost=-$976.00",
        "pricedb-latest=-$1,270.00",
        // Current's split comes first and starts GBP's pair inside USD's entry, so FUNDS is put in front of it.
        "totals=[\"FUNDS=[rational(\\\"70\\\"), rational(\\\"16480\\\")]\", \"GBP=[rational(\\\"1100\\\"), rational(\\\"1669\\\")]\"]",
        "cost-totals=[\"FUNDS=[rational(\\\"30\\\"), rational(\\\"6100\\\")]\", \"GBP=[rational(\\\"100\\\"), rational(\\\"121\\\")]\"]",
        "alist=[\"FUNDS=1648/7\", \"GBP=1669/1100\"]",
        "cost-alist=[\"FUNDS=610/3\", \"GBP=121/100\"]",
        "alist-1971=[\"GBP=1499/1000\", \"FUNDS=1648/7\"]",
        "cost-alist-1971=[\"GBP=291/200\", \"FUNDS=610/3\"]",
        // In GBP: Current's split puts USD in GBP's entry; FUNDS, 70 for $16,480, goes through USD's 1,100 GBP for
        // $1,669 and is put first (CU:395-428).
        "in-gbp=[\"FUNDS=1812800/11683\", \"USD=1100/1669\"]",
    ]);
}

/// gnc:resolve-unknown-comm (CU:313-432) on hand-made sumlists in USD. An entry `[C, [[P, [a, b]], ...]]` says `a`
/// of P went for `b` of C; the report-list's `[C, [a, b]]` says `a` of C went for `b` of the report commodity.
#[test]
fn resolve_unknown_comm_follows_each_branch() {
    let got = run(r#"
        let r = |s| rational(s);
        let show = |l| l.map(|e| `${e[0].mnemonic}=${e[1][0]}:${e[1][1]}`);
        let known = [usd, [[gbp, [r.call(2), r.call(3)]]]];
        // Through C: 10 FUNDS for 5 GBP, and 2 GBP for $3, is 10 FUNDS for $15/2.
        print(`pair-a=${show.call(exchange::resolve_unknown_comm([[gbp, [[funds, [r.call(10), r.call(5)]]]], known], usd))}`);
        // Through P: 2 GBP for 9 FRF is 9 FRF for $3.
        print(`pair-b=${show.call(exchange::resolve_unknown_comm([[frf, [[gbp, [r.call(2), r.call(9)]]]], known], usd))}`);
        // The report commodity on the wrong side is turned round.
        print(`wrong-side=${show.call(exchange::resolve_unknown_comm([known, [frf, [[usd, [r.call(4), r.call(6)]]]]], usd))}`);
        // Both known, or neither: left out (GnuCash warns).
        let both = [usd, [[gbp, [r.call(2), r.call(3)]], [funds, [r.call(1), r.call(1)]]]];
        print(`ambiguous=${show.call(exchange::resolve_unknown_comm([[gbp, [[funds, [r.call(1), r.call(1)]]]], both], usd))}`);
        print(`unknown=${show.call(exchange::resolve_unknown_comm([[frf, [[dem, [r.call(1), r.call(1)]]]], known], usd))}`);
        // A report-list with a commodity twice: assoc takes the first.
        let twice = [usd, [[gbp, [r.call(2), r.call(3)]], [gbp, [r.call(1), r.call(100)]]]];
        print(`first-match=${show.call(exchange::resolve_unknown_comm([[funds, [[gbp, [r.call(4), r.call(1)]]]], twice], usd))}`);
        // Through the euro: 1.95583 DEM is €1, so 4 GBP for 1.95583 DEM is 4 GBP for €1.
        let in_eur = [[eur, []], [dem, [[gbp, [r.call(4), r.call("1.95583")]]]]];
        print(`euro=${show.call(exchange::resolve_unknown_comm(in_eur, eur))}`);
        // A zero known domestic total gives 0.
        let zero = [usd, [[gbp, [r.call(0), r.call(3)]]]];
        print(`zero=${show.call(exchange::resolve_unknown_comm([[gbp, [[funds, [r.call(10), r.call(5)]]]], zero], usd))}`);
    "#);
    check(&got, &[
        "pair-a=[\"FUNDS=10:15/2\", \"GBP=2:3\"]",
        "pair-b=[\"FRF=9:3\", \"GBP=2:3\"]",
        "wrong-side=[\"FRF=6:4\", \"GBP=2:3\"]",
        "ambiguous=[\"GBP=2:3\", \"FUNDS=1:1\"]",
        "unknown=[\"GBP=2:3\"]",
        // 4 GBP for 1 FUNDS, at the first GBP entry's $3 for 2.
        "first-match=[\"FUNDS=1:6\", \"GBP=2:3\", \"GBP=1:100\"]",
        "euro=[\"GBP=4:1\"]",
        "zero=[\"FUNDS=10:0\", \"GBP=0:3\"]",
    ]);
}

/// The euro comes before everything else (CU:635-678): 100 DEM is €51.13 at the fixed 1.95583, and €1/3 exchanged
/// into euros is rounded to the cent by the euro conversion before the same-commodity check.
#[test]
fn euro_currencies_exchange_at_fixed_rates() {
    let got = run(r#"
        print(`dem=${exchange::exchange_by_euro(monetary(dem, 100), eur, ())} ` +
              `usd=[${exchange::exchange_by_euro(monetary(dem, 100), usd, ())}]`);
        print(`numeric=${exchange::exchange_by_euro_numeric(dem, 100, eur, ())}`);
        let fx = exchange::make_exchange_function([[gbp, rational(2)]]);
        print(`alist-euro=${exchange::apply_exchange_fn(fx, monetary(eur, rational(1, 3)), eur).amount}`);
        print(`alist-same=${exchange::apply_exchange_fn(fx, monetary(usd, rational(1, 3)), usd).amount}`);
        print(`alist-rate=${exchange::apply_exchange_fn(fx, monetary(gbp, 3), usd)} ` +
              `missing=${exchange::apply_exchange_fn(fx, monetary(funds, 3), usd)}`);
        print(`if-same=${exchange::exchange_if_same(monetary(usd, 1), usd)} [${exchange::exchange_if_same(monetary(usd, 1), gbp)}]`);
        print(`closure=${exchange::apply_exchange_fn(|f, d| monetary(d, f.amount * 2), monetary(gbp, 1), usd)}`);
    "#);
    check(&got, &[
        "dem=€51.13 usd=[]",
        "numeric=€51.13",
        "alist-euro=33/100",
        "alist-same=1/3",
        "alist-rate=$6.00 missing=$0.00",
        "if-same=$1.00 []",
        "closure=$2.00",
    ]);
}

/// gnc:case-price-fn (CU:797-810): one unit's price in USD by each source, after the 1970 transfers and the 1971
/// prices (GBP 150/101, FUNDS 300.01), and at their latest: GBP's of 1980-02-01 (170/100), FUNDS's of 1975
/// (500.05 × 100/104).
#[test]
fn case_price_fn_prices_one_unit() {
    let got = run(r#"
        let when = end_of.call(1971, 1, 1);
        for source in ["pricedb-nearest", "pricedb-before", "pricedb-latest", "weighted-average", "average-cost"] {
            let pf = exchange::case_price_fn(book, source, usd, when);
            print(`${source}=${exchange::apply_price_fn(pf, gbp)} ${exchange::apply_price_fn(pf, funds)}`);
        }
        let before = exchange::case_price_fn(book, "pricedb-before", usd, end_of.call(1970, 12, 31));
        print(`before-1971=${exchange::apply_price_fn(before, gbp)} ${exchange::apply_price_fn(before, funds)}`);
        print(`none=${exchange::apply_price_fn(exchange::case_price_fn(book, "pricedb-nearest", gbp, when), funds)}`);
        print(`closure=${exchange::apply_price_fn(|c| c.mnemonic, gbp)}`);
    "#);
    check(&got, &[
        "pricedb-nearest=150/101 30001/100",
        "pricedb-before=150/101 30001/100",
        "pricedb-latest=17/10 50005/104",
        "weighted-average=1499/1000 1648/7",
        "average-cost=291/200 610/3",
        "before-1971=33/20 250",
        // No FUNDS price in GBP: the database's indirect price through USD, 300.01 / (150/101)
        // (5.17:libgnucash/engine/gnc-pricedb.cpp:2479-2501).
        "none=3030101/15000",
        "closure=GBP",
    ]);
}

/// gnc:case-exchange-time-fn (CU:829-887): the exchange of 3 GBP and 3 FUNDS at the date of each, by each source.
/// On 1970-03-10 GBP's running average is $562 for 400 (281/200) and its cost $278 for 200 (139/100); FUNDS's both
/// are $6,350 for 30. On 1970-03-25 the nearest price is 1 April's, the latest before 1 March's. The latest prices
/// are converted at the cent, as gnc-pricedb-convert-balance-latest-price does: 3 × 500.05 × 100/104 is $1,442.45.
#[test]
fn case_exchange_time_fn_exchanges_at_each_date() {
    let got = run(r#"
        let sources = ["weighted-average", "average-cost", "pricedb-nearest", "pricedb-before", "pricedb-latest", "no-such"];
        for when in [at.call(1970, 3, 10), at.call(1970, 3, 25)] {
            for source in sources {
                let fx = exchange::case_exchange_time_fn(book, ctx, source, usd, [gbp, funds, gbp], end_of.call(1971, 1, 1));
                let g = exchange::apply_exchange_time_fn(fx, monetary(gbp, 3), usd, when).amount;
                let f = exchange::apply_exchange_time_fn(fx, monetary(funds, 3), usd, when).amount;
                print(`${ctx.local(when).day} ${source}=${g} ${f}`);
            }
        }
        // One price table per day, kept in the memo.
        let fx = exchange::case_exchange_time_fn(book, ctx, "average-cost", usd, [], ());
        exchange::apply_exchange_time_fn(fx, monetary(gbp, 1), usd, at.call(1970, 3, 10));
        exchange::apply_exchange_time_fn(fx, monetary(funds, 1), usd, at.call(1970, 3, 10) + 3600);
        exchange::apply_exchange_time_fn(fx, monetary(gbp, 1), usd, at.call(1970, 3, 11));
        print(`memo=${fx.memo.len()}`);
        // The weighted average prices only the commodities of the list: FUNDS is not in it.
        let only_gbp = exchange::case_exchange_time_fn(book, ctx, "weighted-average", usd, [gbp], ());
        print(`unlisted=${exchange::apply_exchange_time_fn(only_gbp, monetary(funds, 3), usd, at.call(1970, 3, 10))}`);
        // 1980-02-01's 100 GBP for $170 joins the average.
        print(`1980=${exchange::apply_exchange_time_fn(only_gbp, monetary(gbp, 1), usd, at.call(1980, 3, 1)).amount}`);
        print(`not-monetary=[${exchange::apply_exchange_time_fn(only_gbp, 3, usd, at.call(1980, 3, 1))}]`);
    "#);
    check(&got, &[
        "10 weighted-average=843/200 635",
        "10 average-cost=417/100 635",
        "10 pricedb-nearest=9/2 675",
        "10 pricedb-before=9/2 675",
        "10 pricedb-latest=51/10 28849/20",
        "10 no-such=9/2 675",
        "25 weighted-average=843/200 635",
        "25 average-cost=417/100 635",
        "25 pricedb-nearest=93/20 732",
        "25 pricedb-before=9/2 675",
        "25 pricedb-latest=51/10 28849/20",
        "25 no-such=93/20 732",
        "memo=2",
        "unlisted=$0.00",
        "1980=1669/1100",
        "not-monetary=[]",
    ]);
}

/// The splits with two commodities, and the price lists made from them (CU:64-275).
#[test]
fn commodity_splits_and_price_lists() {
    let got = run(r#"
        let accts = book.root().descendants_sorted();
        let all = exchange::get_all_commodity_splits(book, accts, ());
        let funds_splits = exchange::get_match_commodity_splits(book, accts, (), funds);
        let sorted = exchange::get_match_commodity_splits_sorted(book, accts, (), ());
        let until_1971 = exchange::get_all_commodity_splits(book, accts, end_of.call(1971, 1, 1));
        print(`counts=${all.len()} ${funds_splits.len()} ${until_1971.len()}`);
        print(`unsorted-first=${all[0].transaction.description} sorted-first=${sorted[0].transaction.description}`);
        let prices = |l| l.map(|p| `${p[1]}`);
        print(`totalavg=${prices.call(exchange::get_commodity_totalavg_prices(book, accts, (), funds, usd))}`);
        print(`inst=${prices.call(exchange::get_commodity_inst_prices(book, accts, (), funds, usd))}`);
        let gbp_inst = exchange::get_commodity_inst_prices(book, accts, (), gbp, usd);
        print(`gbp-inst=${gbp_inst.len()} ${gbp_inst[0][1]} ${gbp_inst[-1][1]} ${ctx.print_time_iso(gbp_inst[-1][0])}`);
        let list = exchange::get_commoditylist_totalavg_prices(book, [funds, gbp, funds], usd, end_of.call(1970, 3, 1));
        print(`list=${list.map(|e| `${e[0].mnemonic}:${prices.call(e[1])}`)}`);
        let pl = [[at.call(1970, 1, 1), 1], [at.call(1970, 1, 3), 3], [at.call(1970, 1, 5), 5]];
        let near = |d| exchange::pricelist_price_find_nearest(pl, d);
        print(`nearest=${near.call(at.call(1969, 1, 1))} ${near.call(at.call(1970, 1, 2) - 1)} ${near.call(at.call(1970, 1, 2))} ` +
              `${near.call(at.call(1970, 1, 4) + 1)} ${near.call(at.call(1971, 1, 1))} [${exchange::pricelist_price_find_nearest([], 0)}]`);
    "#);
    check(&got, &[
        "counts=18 7 17",
        "unsorted-first=earn 100GBP into $170 sorted-first=buy 100GBP at $1.30",
        "totalavg=[\"200\", \"205\", \"635/3\", \"879/4\", \"1143/5\", \"233\", \"1648/7\"]",
        "inst=[\"200\", \"210\", \"225\", \"244\", \"264\", \"255\", \"250\"]",
        "gbp-inst=11 13/10 17/10 1980-02-01 10:59:00",
        "list=[\"FUNDS:[\\\"200\\\", \\\"205\\\", \\\"635/3\\\"]\", \
         \"GBP:[\\\"13/10\\\", \\\"27/20\\\", \\\"103/75\\\", \\\"281/200\\\"]\"]",
        // Halfway is a tie, which takes the later price (CU:289).
        "nearest=1 1 3 5 5 []",
    ]);
}
