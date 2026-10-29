//! `lib/accounts` (`5.17:gnucash/report/report-utilities.scm:149-257`, RU; `5.17:bindings/engine.i:241-254`) on
//! `fixtures/reports/balsheet-pnl.gnucash`, whose tree is `standard/test/test-balsheet-pnl.scm:92-108`'s.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{Host, fixture};
use libs::{check, lines, lines_with};

const PRELUDE: &str = r#"
import "lib/accounts" as accounts;
let usd = book.commodity("USD"); let gbp = book.commodity("GBP"); let funds = book.commodity("FUNDS::FUNDS");
let all = book.root().descendants_sorted();
let names = |l| l.map(|a| a.name);
let acct = |n| book.account(n);
"#;

fn host() -> Host {
    Host::new(fixture("balsheet-pnl.gnucash"))
}

fn run(body: &str) -> Vec<String> {
    lines(&host(), &format!("{PRELUDE}\n{body}"))
}

/// The tree, in display order: the top-level ASSET account Root holds everything; Funds is the one STOCK account,
/// ForeignBank, ForeignSavings and Income-GBP are in GBP, Funds in FUNDS, the rest in USD.
#[test]
fn predicates_and_lists_by_type() {
    let got = run(r#"
        print(`all=${names.call(all)}`);
        print(`has-shares=${names.call(all.filter(|a| accounts::account_has_shares(a)))}`);
        print(`stock=${names.call(all.filter(|a| accounts::account_is_stock(a)))}`);
        print(`inc-exp=${names.call(all.filter(|a| accounts::account_is_inc_exp(a)))}`);
        print(`filter=${names.call(accounts::filter_accountlist_type(["STOCK", "INCOME"], [()] + all))}`);
        let kinds = accounts::decompose_accountlist(all);
        print(`kinds=${kinds.keys()}`);
        for k in ["ASSET", "LIABILITY", "EQUITY", "INCOME", "EXPENSE", "TRADING"] {
            print(`${k}=${names.call(kinds[k])}`);
        }
    "#);
    check(
        &got,
        &[
            "all=[\"Root\", \"Asset\", \"Bank1\", \"Bonds\", \"Current\", \"Empty\", \"Savings\", \"Broker\", \"Funds\", \
         \"ForeignBank\", \"ForeignSavings\", \"House\", \"Liability\", \"Bank2\", \"CreditCard\", \"Loan\", \"Income\", \
         \"Income-GBP\", \"Equity\"]",
            "has-shares=[\"Funds\"]",
            "stock=[\"Funds\"]",
            "inc-exp=[\"Income\", \"Income-GBP\"]",
            // A null account is skipped (RU:180).
            "filter=[\"Funds\", \"Income\", \"Income-GBP\"]",
            "kinds=[\"ASSET\", \"EQUITY\", \"EXPENSE\", \"INCOME\", \"LIABILITY\", \"TRADING\"]",
            "ASSET=[\"Root\", \"Asset\", \"Bank1\", \"Bonds\", \"Current\", \"Empty\", \"Savings\", \"Broker\", \"Funds\", \
         \"ForeignBank\", \"ForeignSavings\", \"House\"]",
            "LIABILITY=[\"Liability\", \"Bank2\", \"CreditCard\", \"Loan\"]",
            "EQUITY=[\"Equity\"]",
            "INCOME=[\"Income\", \"Income-GBP\"]",
            "EXPENSE=[]",
            "TRADING=[]",
        ],
    );
}

/// gnc:accounts-get-commodities builds its list from the end, so a commodity stands where it last appears
/// (RU:235-240); the sorted form orders by nice symbol, then namespace, bytewise here. The fixture's GBP symbol is
/// "#", which sorts before "$".
#[test]
fn commodities_of_accounts() {
    let got = run(r#"
        let mn = |l| l.map(|c| c.mnemonic);
        print(`all=${mn.call(accounts::accounts_get_commodities(all, ()))} ` +
              `no-usd=${mn.call(accounts::accounts_get_commodities(all, usd))}`);
        let some = [acct.call("Root:Asset:Broker:Funds"), acct.call("Root:Asset:Bank1:Bonds"),
                    acct.call("Root:Asset:ForeignBank:ForeignSavings"), acct.call("Root:Asset:Broker:Funds")];
        print(`some=${mn.call(accounts::accounts_get_commodities(some, ()))} none=${accounts::accounts_get_commodities([], ())}`);
        print(`symbols=${[usd, gbp, funds].map(|c| c.symbol)}`);
        print(`sorted=${mn.call(accounts::accounts_get_commodities_sorted(all, ()))} ` +
              `no-usd=${mn.call(accounts::accounts_get_commodities_sorted(all, usd))}`);
    "#);
    check(
        &got,
        &[
            "all=[\"FUNDS\", \"GBP\", \"USD\"] no-usd=[\"FUNDS\", \"GBP\"]",
            "some=[\"USD\", \"GBP\", \"FUNDS\"] none=[]",
            "symbols=[\"$\", \"#\", \"FUNDS\"]",
            "sorted=[\"GBP\", \"USD\", \"FUNDS\"] no-usd=[\"GBP\", \"FUNDS\"]",
        ],
    );
}

/// Depths count the book's root as 0 for an account and 1 for a tree: Bonds is at depth 4, under Root, Asset and
/// Bank1, and the tree is 5 deep (RU:163-168, :255-257).
#[test]
fn depths_descendants_and_name_order() {
    let got = run(r#"
        print(`tree=${accounts::get_current_account_tree_depth(book)}`);
        print(`children-depth=${accounts::accounts_get_children_depth([acct.call("Root:Asset")])} ` +
              `${accounts::accounts_get_children_depth([acct.call("Root:Asset:House"), acct.call("Root:Liability:Bank2")])} ` +
              `${accounts::accounts_get_children_depth([acct.call("Root:Equity")])}`);
        let picked = [acct.call("Root:Asset:Bank1"), acct.call("Root:Asset:Bank1:Current"), acct.call("Root:Asset:Broker")];
        print(`descendants=${names.call(accounts::accounts_and_all_descendants(picked))}`);
        let unsorted = [acct.call("Root:Asset:House"), acct.call("Root:Asset:Bank1"), acct.call("Root:Asset")];
        unsorted.sort(|a, b| if accounts::account_full_name_lt(a, b) { -1 } else if accounts::account_full_name_lt(b, a) { 1 } else { 0 });
        print(`by-name=${names.call(unsorted)}`);
    "#);
    check(
        &got,
        &[
            "tree=5",
            "children-depth=4 4 2",
            "descendants=[\"Bank1\", \"Bonds\", \"Current\", \"Empty\", \"Savings\", \"Broker\", \"Funds\"]",
            "by-name=[\"Asset\", \"Bank1\", \"House\"]",
        ],
    );
}

/// gnc:account-get-type-string-plural (RU:210-231).
#[test]
fn plural_type_names() {
    let got = run(r#"
        let types = ["BANK", "CREDIT", "ASSET", "MUTUAL", "INCOME", "EQUITY", "MONEYMRKT", "PAYABLE", "TRADING", "ROOT"];
        print(`${types.map(|t| accounts::account_get_type_string_plural(t))}`);
    "#);
    check(
        &got,
        &[
            "[\"Bank\", \"Credits\", \"Assets\", \"Mutual Funds\", \"Income\", \"Equities\", \"Money Market\", \
                   \"Accounts Payable\", \"Trading Accounts\", ()]",
        ],
    );
}

/// `xaccAccountTypeGetFundamental` (`5.17:libgnucash/engine/Account.cpp:4483-4513`) of every type the query API
/// names.
#[test]
fn fundamental_types() {
    let got = run(r#"
        let types = ["BANK", "STOCK", "MONEYMRKT", "CHECKING", "SAVINGS", "MUTUAL", "CURRENCY", "CASH", "ASSET", "RECEIVABLE",
                     "CREDIT", "LIABILITY", "PAYABLE", "CREDITLINE", "INCOME", "EXPENSE", "EQUITY", "TRADING", "ROOT"];
        print(`${types.map(|t| accounts::account_type_fundamental(t))}`);
    "#);
    let asset = ["\"ASSET\""; 10].join(", ");
    let liability = ["\"LIABILITY\""; 4].join(", ");
    check(
        &got,
        &[&format!("[{asset}, {liability}, \"INCOME\", \"EXPENSE\", \"EQUITY\", \"NONE\", \"NONE\"]")],
    );
}

/// `a.compare_order(b)` is `xaccAccountOrder` (`5.17:libgnucash/engine/Account.cpp:2363-2413`) across the tree, not
/// display order: sorted by it, the ASSET accounts go by name whatever their parents (Root, the top, near the end),
/// where depth first each parent leads its children; Funds, a STOCK account, ranks before every ASSET one.
#[test]
fn account_order_across_the_tree() {
    let got = run(r#"
        let assets = all.filter(|a| a.type == "ASSET");
        let sorted = assets;
        sorted.sort(|a, b| a.compare_order(b));
        print(`flat=${names.call(sorted)}`);
        print(`tree=${names.call(assets)}`);
        let funds = acct.call("Root:Asset:Broker:Funds");
        let house = acct.call("Root:Asset:House");
        print(`cmp=${funds.compare_order(house)} ${house.compare_order(funds)} ${house.compare_order(house)}`);
    "#);
    check(
        &got,
        &[
            "flat=[\"Asset\", \"Bank1\", \"Bonds\", \"Broker\", \"Current\", \"Empty\", \"ForeignBank\", \"ForeignSavings\", \
             \"House\", \"Root\", \"Savings\"]",
            "tree=[\"Root\", \"Asset\", \"Bank1\", \"Bonds\", \"Current\", \"Empty\", \"Savings\", \"Broker\", \"ForeignBank\", \
             \"ForeignSavings\", \"House\"]",
            "cmp=-1 1 0",
        ],
    );
}

/// `(apply max '())` is an error in Guile; so is asking for the depth of no accounts.
#[test]
fn children_depth_of_no_accounts_is_an_error() {
    let err = lines_with(
        &host(),
        "",
        "import \"lib/accounts\" as accounts; accounts::accounts_get_children_depth([]);",
    )
    .unwrap_err();
    assert!(format!("{err:?}").contains("accounts_get_children_depth: no accounts"), "{err:?}");
}
