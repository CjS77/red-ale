//! `lib/collectors` (`5.17:gnucash/report/report-utilities.scm:289-455`, `:1018-1022`;
//! `5.17:gnucash/report/commodity-utilities.scm:900-919`) on hand-made amounts in the commodities of
//! `fixtures/reports/balsheet-pnl.gnucash` (USD, GBP and FUNDS). Expected orders follow the association list: `add`
//! puts a new commodity on the front (RU:346-354) and `merge` walks the other collector from its front (RU:356-361).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{Host, fixture};
use libs::{check, lines};

/// Prints a collector as `MNEMONIC:total` pairs in its order.
const SHOW: &str = r#"
import "lib/collectors" as collectors;
let usd = book.commodity("USD"); let gbp = book.commodity("GBP"); let funds = book.commodity("FUNDS::FUNDS");
let show = |c| c.format(|comm, amt| `${comm.mnemonic}:${amt}`);
"#;

fn run(body: &str) -> Vec<String> {
    lines(&Host::new(fixture("balsheet-pnl.gnucash")), &format!("{SHOW}\n{body}"))
}

#[test]
fn add_puts_new_commodities_first_and_merge_reverses() {
    let got = run(r#"
        let c = collectors::make_commodity_collector();
        c.add(usd, 1); c.add(gbp, rational(2)); c.add(usd, rational("3.5"));
        print(`add=${show.call(c)}`);
        let merged = collectors::make_commodity_collector(); merged.merge(c);
        print(`merge=${show.call(merged)}`);
        let minus = collectors::make_commodity_collector(); minus.minusmerge(c);
        print(`minusmerge=${show.call(minus)}`);
        print(`negated=${show.call(collectors::commodity_collector_get_negated(c))}`);
        print(`minus-one=${show.call(collectors::collector_minus([c]))}`);
        let d = collectors::make_commodity_collector(); d.add(funds, 10); d.add(usd, 1);
        print(`plus=${show.call(collectors::collector_plus([c, d]))} empty=${collectors::collector_plus([])}`);
        print(`minus=${show.call(collectors::collector_minus([c, d, d]))}`);
        c.add(funds, "not a number");
        print(`not-a-number=${show.call(c)}`);
        c.reset();
        print(`reset=${show.call(c)}`);
    "#);
    check(&got, &[
        "add=[\"GBP:2\", \"USD:9/2\"]",
        // merge walks c from its front, putting each new commodity first.
        "merge=[\"USD:9/2\", \"GBP:2\"]",
        "minusmerge=[\"USD:-9/2\", \"GBP:-2\"]",
        // gnc:commodity-collector-get-negated minus-merges once (RU:418-421) …
        "negated=[\"USD:-9/2\", \"GBP:-2\"]",
        // … gnc:collector- of one collector merges it into a sum first, then minus-merges (RU:436-445).
        "minus-one=[\"GBP:-2\", \"USD:-9/2\"]",
        // plus merges c (USD, GBP), then d (USD, FUNDS): FUNDS is new.
        "plus=[\"FUNDS:10\", \"USD:11/2\", \"GBP:2\"] empty=[]",
        // c merged (USD, GBP), minus the sum of d and d (USD 2, FUNDS 20), which walks USD then FUNDS.
        "minus=[\"FUNDS:-20\", \"USD:5/2\", \"GBP:2\"]",
        // A value collector ignores what is not a number (RU:293), but the commodity is added.
        "not-a-number=[\"FUNDS:0\", \"GBP:2\", \"USD:9/2\"]",
        "reset=[]",
    ]);
}

#[test]
fn lookups_signs_and_zeros() {
    let got = run(r#"
        let c = collectors::make_commodity_collector();
        c.add(usd, rational("1.25")); c.add(gbp, 0);
        let p = c.getpair(usd, true);
        print(`getpair=${p[0].mnemonic} ${p[1]} missing=${c.getpair(funds, false)[1]}`);
        print(`getmonetary=${c.getmonetary(usd, false)} ${c.getmonetary(usd, true)} ${c.getmonetary(funds, true)}`);
        print(`strict=${c.getmonetary_strict(usd, false)} ${c.getmonetary_strict(funds, false)}`);
        print(`allzero=${collectors::commodity_collector_allzero(c)} ${collectors::commodity_collector_allzero([])}`);
        c.remove_zeros();
        print(`remove-zeros=${show.call(c)}`);
        let z = collectors::make_commodity_collector(); z.add(usd, 1); z.add(usd, -1);
        print(`allzero-after-sum=${collectors::commodity_collector_allzero(z)}`);
        print(`monetaries-add=${show.call(collectors::monetaries_add([monetary(usd, 1), monetary(gbp, 2), monetary(usd, 3)]))}`);
        let v = collectors::make_value_collector(); v += 5; v += rational(1, 2);
        print(`value=${v}`);
        print(`not-all-zeros=${collectors::not_all_zeros([0, [rational(0), 1]])} ${collectors::not_all_zeros([0, [rational(0)]])} ` +
              `${collectors::not_all_zeros(monetary(usd, 1))} ${collectors::not_all_zeros(rational(-1))}`);
        let u = collectors::make_commodity_collector();
        print(`uniform=${collectors::uniform_commodity(u, usd)} ${collectors::uniform_commodity(c, usd)} ` +
              `${collectors::uniform_commodity(c, gbp)} ${collectors::uniform_commodity(z, usd)}`);
        u.add(usd, 1); u.add(gbp, 1);
        print(`uniform-two=${collectors::uniform_commodity(u, usd)}`);
    "#);
    check(&got, &[
        "getpair=USD -5/4 missing=0",
        "getmonetary=$1.25 -$1.25 0. FUNDS",
        "strict=$1.25 ",
        "allzero=false true",
        "remove-zeros=[\"USD:5/4\"]",
        "allzero-after-sum=true",
        "monetaries-add=[\"GBP:2\", \"USD:4\"]",
        "value=11/2",
        // gnc:not-all-zeros looks into lists and is false for anything but numbers (RU:1018-1022).
        "not-all-zeros=true false false true",
        "uniform=true true false true",
        "uniform-two=false",
    ]);
}

/// gnc:sum-collector-commodity (CU:900-910) rounds each exchanged amount to the domestic SCU with banker's rounding
/// before adding: at 1/8, 1 FUNDS is $0.125 → $0.12 and 3 GBP $0.375 → $0.38, $0.50 in all (half-up would give
/// $0.51).
#[test]
fn sum_collector_commodity_rounds_each_amount_to_the_scu() {
    let got = run(r#"
        let c = collectors::make_commodity_collector();
        c.add(funds, 1); c.add(gbp, 3);
        let eighth = |m, d| monetary(d, m.amount * rational(1, 8));
        let sum = collectors::sum_collector_commodity(c, usd, eighth);
        print(`sum=${sum.amount} ${sum}`);
        print(`none=${collectors::sum_collector_commodity((), usd, eighth)} ${collectors::sum_collector_commodity(c, usd, ())}`);
        print(`empty=${collectors::sum_collector_commodity([], usd, eighth)}`);
    "#);
    check(&got, &["sum=1/2 $0.50", "none= ", "empty=$0.00"]);
}

/// gnc:monetary+ (RU:460-465) adds Monetaries of one commodity, and throws for any other number of commodities.
#[test]
fn monetary_plus_wants_one_commodity() {
    let got = run(r#"
        print(`${collectors::monetary_plus([monetary(usd, 1), monetary(usd, rational(1, 2))])}`);
        for ms in [[monetary(usd, 1), monetary(gbp, 1)], []] {
            try { collectors::monetary_plus(ms); } catch (e) { print(`error: ${e}`); }
        }
    "#);
    check(&got, &[
        "$1.50",
        "error: monetary_plus expects 1 currency: [monetary(\"CURRENCY::USD\", \"1\"), monetary(\"CURRENCY::GBP\", \"1\")]",
        "error: monetary_plus expects 1 currency: []",
    ]);
}
