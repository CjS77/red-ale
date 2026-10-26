//! CSV price import, hand-worked from `gnc-import-price.cpp` and `gnc-imp-props-price.cpp`: GnuCash has no tests of
//! it.

use std::sync::Arc;

use proptest::prelude::*;
use red_ale_core::ops::prices::{EDITOR_SOURCE, FQ_SOURCE};
use red_ale_core::{Commodity, Date, FixedEnv, Time64};
use serde_json::{Value, json};

use super::*;
use crate::csv::{DateContext, MonetaryLocale, tokenize};
use crate::matcher::ImportSettings;
use crate::pipeline::Format;

struct Fx {
    book: Book,
    env: FixedEnv,
}

fn acme() -> CommodityRef {
    CommodityRef::new("NASDAQ", "ACME")
}

fn usd() -> CommodityRef {
    CommodityRef::currency("USD")
}

fn eur() -> CommodityRef {
    CommodityRef::currency("EUR")
}

fn day(d: u32) -> Date {
    Date::from_ymd(2024, 3, d).unwrap()
}

fn fx() -> Fx {
    let mut env = FixedEnv::new(Time64(1_711_000_000), 1);
    let mut book = Book::new_empty(&mut env);
    book.commodities.insert(acme(), Arc::new(Commodity::new(acme(), Some("Acme Corp".to_owned()), 10_000)));
    Fx { book, env }
}

impl Fx {
    fn price(&mut self, commodity: CommodityRef, currency: CommodityRef, time: Time64, source: &str, value: Numeric) -> Guid {
        let guid = self.env.new_guid();
        let price = Price { guid, commodity, currency, time, source: Some(source.to_owned()), price_type: None, value };
        add_price(&mut self.book, price, &TimeZone::utc()).unwrap();
        guid
    }

    /// Preview `text` with `layout` as the service does.
    fn propose(&self, layout: Value, text: &str, overwrite: bool) -> Result<Proposal, Vec<String>> {
        let layout: PriceLayout = serde_json::from_value::<PriceLayout>(layout).unwrap().sanitized();
        let defaults = Defaults::resolve(&self.book, &layout).map_err(|e| vec![e.to_string()])?;
        let rows = tokenize(text, &layout.csv).unwrap();
        let ctx = PriceContext {
            date_format: layout.csv.date_format,
            currency_format: layout.csv.currency_format,
            dates: DateContext { today: day(26), locale_date: None },
            monetary: MonetaryLocale::default(),
        };
        let import = PriceImport::parse(&self.book, rows, &layout, &defaults, &ctx);
        let problems = import.file_errors(&defaults);
        if !problems.is_empty() {
            return Err(problems);
        }
        let (prices, errors, ignored_lines) = propose(&self.book, &TimeZone::utc(), &import, overwrite, 0);
        Ok(Proposal { ignored_lines, errors, prices, ..Proposal::empty(Format::CsvPrices, ImportSettings::default(), None) })
    }

    fn apply(&mut self, p: &Proposal, changes: &[Change]) -> DomainResult<Applied> {
        apply(&mut self.book, &mut self.env, p, changes)
    }
}

fn all_columns() -> Value {
    json!({ "columns": ["Date", "Amount", "From Symbol", "From Namespace", "Currency To"] })
}

fn actions(p: &Proposal) -> Vec<&str> {
    p.prices.iter().map(|x| x.action.as_str()).collect()
}

fn errors(p: &Proposal) -> Vec<(usize, Option<usize>, &str)> {
    p.errors.iter().map(|e| (e.line, e.column, e.message.as_str())).collect()
}

#[test]
fn every_column_makes_a_price_as_gnucash_does() {
    let mut f = fx();
    let p = f.propose(all_columns(), "2024-03-01,$12.3456789,ACME,NASDAQ,USD\n2024-03-02,0.9,EUR,CURRENCY,USD\n", false).unwrap();
    assert_eq!(actions(&p), ["add", "add"]);
    let first = &p.prices[0];
    assert_eq!((first.commodity.clone(), first.currency.clone(), first.date), (acme(), usd(), day(1)));
    assert!(first.amount.same_repr(Numeric::new(123_456_789, 10_000_000)), "kept as written: {}", first.amount);
    assert!(first.value.same_repr(Numeric::new(12_345_679, 1_000_000)), "USD's 100 × 10000, half up: {}", first.value);
    assert_eq!(first.time, day(1).neutral_time());
    let applied = f.apply(&p, &[]).unwrap();
    assert_eq!(applied.prices.0.len(), 2);
    let stored = f.book.prices.get(&applied.prices.0[0]).unwrap();
    assert_eq!((stored.source.as_deref(), stored.price_type.as_deref()), (Some("user:price"), Some("last")));
    assert!(stored.value.same_repr(Numeric::new(12_345_679, 1_000_000)));
}

#[test]
fn defaults_stand_in_for_columns() {
    let f = fx();
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "CURRENCY::EUR", "columns": ["Date", "Amount"], "dateFormat": "d-m-y" });
    let p = f.propose(layout, "01/03/2024,10\n", false).unwrap();
    assert_eq!((p.prices[0].commodity.clone(), p.prices[0].currency.clone(), p.prices[0].date), (acme(), eur(), day(1)));
    let layout = json!({ "toCurrency": "CURRENCY::USD", "columns": ["Date", "From Symbol", "From Namespace", "Amount"] });
    assert_eq!(f.propose(layout, "2024-03-01,ACME,NASDAQ,10\n", false).unwrap().prices.len(), 1);
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "columns": ["Date", "Amount", "Currency To"] });
    let p = f.propose(layout, "2024-03-01,10,CURRENCY::EUR\n2024-03-01,10,NASDAQ::ACME\n", false).unwrap();
    assert_eq!(p.prices[0].currency, eur(), "a unique name works in the column");
    assert_eq!(errors(&p), [(2, Some(3), "Currency To: 'Currency To' can not be the same as 'Commodity From'.")]);
    let layout = json!({ "columns": ["Date", "Amount", "From Symbol", "From Namespace", "Currency To"] });
    let p = f.propose(layout, "2024-03-01,10,NASDAQ::ACME,CURRENCY,USD\n", false).unwrap();
    assert_eq!(p.prices[0].commodity, acme(), "a unique name in From Symbol wins over the namespace");
}

#[test]
fn layout_and_column_problems_stop_the_import() {
    let f = fx();
    let e = f.propose(json!({ "columns": ["Amount", "From Symbol"] }), "10,ACME\n", false).unwrap_err();
    assert_eq!(
        e,
        [
            "Please select a date column.",
            "Please select a 'Currency to' column or set a Currency in the 'Currency To' field.",
            "Please select a 'From Namespace' column or set a Commodity in the 'Commodity From' field.",
        ]
    );
    let e = f.propose(json!({ "fromCommodity": "NASDAQ::ACME", "columns": ["Date", "Amount", "From Symbol", "Currency To"] }), "x\n", false).unwrap_err();
    assert!(e[0].contains("exclude each other"), "{e:?}");
    let e = f.propose(json!({ "fromCommodity": "NASDAQ::NOPE", "columns": ["Date", "Amount"] }), "x\n", false).unwrap_err();
    assert_eq!(e, ["commodity NASDAQ::NOPE not found"]);
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "EUR", "columns": ["Date", "Amount"] });
    assert_eq!(f.propose(layout, "2024-03-01,1\n", false).unwrap().prices[0].currency, eur(), "a bare currency code");
    let e = f.propose(json!({ "toCurrency": "NASDAQ::ACME", "columns": ["Date", "Amount"] }), "x\n", false).unwrap_err();
    assert_eq!(e, ["the layout's toCurrency NASDAQ::ACME is not a currency"]);
    let e = f.propose(json!({ "fromCommodity": "CURRENCY::USD", "toCurrency": "CURRENCY::USD", "columns": ["Date", "Amount"] }), "x,y\n", false).unwrap_err();
    assert_eq!(e, ["'Commodity From' can not be the same as 'Currency To'."]);
    assert_eq!(f.propose(all_columns(), "\n\n", false).unwrap_err()[0], "No valid data found in the selected file. It may be empty or the selected encoding is wrong.");
    let e = f.propose(json!({ "skipStartLines": 1, "columns": ["Date", "Amount", "From Symbol", "From Namespace", "Currency To"] }), "h\n", false).unwrap_err();
    assert_eq!(e, ["No lines are selected for importing. Please reduce the number of lines to skip."]);
}

#[test]
fn error_rows_name_the_column_and_gnucash_s_reason() {
    let f = fx();
    let text = "2024-03-01,,ACME,NASDAQ,USD\n\
                nope,1,ACME,NASDAQ,USD\n\
                2024-03-01,abc,ACME,NASDAQ,USD\n\
                2024-03-01,1,NOPE,NASDAQ,USD\n\
                2024-03-01,1,ACME,NYSEX,USD\n\
                2024-03-01,1,ACME,NASDAQ,XXZ\n\
                2024-03-01,1,EUR,CURRENCY,NASDAQ::ACME\n\
                2024-03-01,1,USD,CURRENCY,USD\n\
                2024-03-01,1,ACME\n\
                2024-03-01,-1.5,ACME,NASDAQ,USD\n";
    let p = f.propose(all_columns(), text, false).unwrap();
    assert_eq!(
        errors(&p),
        [
            (1, Some(2), "Amount: Column value can not be empty."),
            (2, Some(1), "Date: Value can't be parsed into a date using the selected date format."),
            (3, Some(2), "Amount: Value doesn't appear to contain a valid number."),
            (4, Some(4), "From Namespace: Value can't be parsed into a valid commodity."),
            (5, Some(4), "From Namespace: Value can't be parsed into a valid namespace."),
            (6, Some(5), "Currency To: Value can't be parsed into a valid commodity."),
            (7, Some(5), "Currency To: Value parsed into an invalid currency for a currency column type."),
            (8, Some(5), "Currency To: 'Currency To' can not be the same as 'Commodity From'."),
            (9, None, "No 'Currency to'."),
        ]
    );
    assert_eq!(p.prices.len(), 1, "a negative price is accepted, as GnuCash accepts it");
    assert_eq!(p.prices[0].line, 10);
}

#[test]
fn same_day_prices_in_either_direction_are_duplicates_or_replaced() {
    let mut f = fx();
    let usd_in_eur = f.price(usd(), eur(), day(1).neutral_time().plus_seconds(3600), EDITOR_SOURCE, Numeric::new(9, 10));
    let acme_usd = f.price(acme(), usd(), day(2).neutral_time(), "Finance::Quote", Numeric::new(100, 1));
    let text = "2024-03-01,1.1,EUR,CURRENCY,USD\n2024-03-02,101,ACME,NASDAQ,USD\n2024-03-03,102,ACME,NASDAQ,USD\n";
    let p = f.propose(all_columns(), text, false).unwrap();
    assert_eq!(actions(&p), ["skip", "skip", "add"]);
    assert_eq!(p.prices[0].existing, Some(Existing::Book(usd_in_eur)), "the price of USD in EUR counts for EUR in USD");
    assert_eq!(p.prices[1].existing, Some(Existing::Book(acme_usd)));
    let p = f.propose(all_columns(), text, true).unwrap();
    assert_eq!(actions(&p), ["update", "update", "add"], "over-writing replaces whatever the source");
    let applied = f.apply(&p, &[]).unwrap();
    assert_eq!((applied.prices.0.len(), applied.prices.1.len(), applied.prices.2.len()), (1, 2, 0));
    assert!(!f.book.prices.contains_key(&usd_in_eur) && !f.book.prices.contains_key(&acme_usd));
    let replaced = f.book.prices.get(&applied.prices.1[0]).unwrap();
    assert_eq!((replaced.commodity.clone(), replaced.currency.clone()), (eur(), usd()));
}

#[test]
fn a_second_line_on_the_same_day_meets_the_first() {
    let mut f = fx();
    let text = "2024-03-01,10,ACME,NASDAQ,USD\n2024-03-01,11,ACME,NASDAQ,USD\n";
    let p = f.propose(all_columns(), text, false).unwrap();
    assert_eq!(actions(&p), ["add", "skip"]);
    assert_eq!(p.prices[1].existing, Some(Existing::Index(0)));
    let p = f.propose(all_columns(), text, true).unwrap();
    assert_eq!(actions(&p), ["add", "update"]);
    let applied = f.apply(&p, &[]).unwrap();
    assert_eq!((applied.prices.0.len(), applied.prices.1.len()), (1, 1));
    assert_eq!(f.book.prices.len(), 1, "the second replaced the first");
    assert!(f.book.prices.values().next().unwrap().value.same_repr(Numeric::new(11_000_000, 1_000_000)));
}

#[test]
fn the_caller_chooses_per_price() {
    let mut f = fx();
    let old = f.price(acme(), usd(), day(1).neutral_time(), EDITOR_SOURCE, Numeric::new(9, 1));
    let p = f.propose(all_columns(), "2024-03-01,10,ACME,NASDAQ,USD\n2024-03-02,11,ACME,NASDAQ,USD\n", false).unwrap();
    assert_eq!(actions(&p), ["skip", "add"]);
    let update = |index| Change { index, action: Some(ImportAction::Update), ..Change::default() };
    let skip = Change { index: 1, action: Some(ImportAction::Skip), ..Change::default() };
    assert!(f.apply(&p, &[Change { index: 0, action: Some(ImportAction::Clear), ..Change::default() }]).is_err());
    assert!(f.apply(&p, &[Change { index: 2, ..Change::default() }]).is_err());
    let applied = f.apply(&p, &[update(0), skip]).unwrap();
    assert_eq!((applied.prices.1.len(), applied.skipped.clone()), (1, vec![1]));
    assert!(!f.book.prices.contains_key(&old));
}

#[test]
fn a_price_the_database_would_refuse_is_an_error_and_the_old_one_stays() {
    let mut f = fx();
    let editor = f.price(acme(), usd(), day(1).neutral_time(), EDITOR_SOURCE, Numeric::new(9, 1));
    let book_before = f.book.prices.len();
    // A second same-day price, loaded from a file (bulk, no same-day rules): ranked better than user:price.
    let quote = Price { guid: f.env.new_guid(), commodity: acme(), currency: usd(), time: day(1).neutral_time().plus_seconds(18_000), source: Some(FQ_SOURCE.to_owned()), price_type: None, value: Numeric::new(95, 10) };
    f.book.prices.insert(quote.guid, Arc::new(quote));
    let p = f.propose(all_columns(), "2024-03-01,10,ACME,NASDAQ,USD\n", true).unwrap();
    assert!(p.prices.is_empty());
    assert_eq!(p.errors[0].message, format!("Failed to create price from selected columns: price {} of that day has a source that ranks higher.", f.book.prices.get_index(1).unwrap().0));
    assert!(f.book.prices.contains_key(&editor) && f.book.prices.len() == book_before + 1);
}

#[test]
fn yen_prices_round_at_ten_thousandths_of_a_yen() {
    let f = fx();
    let p = f.propose(json!({ "columns": ["Date", "Amount", "From Symbol", "From Namespace", "Currency To"], "currencyFormat": "comma" }), "2024-03-01,\"123,45675\",ACME,NASDAQ,JPY\n", false).unwrap();
    assert!(p.prices[0].value.same_repr(Numeric::new(1_234_568, 10_000)), "{}", p.prices[0].value);
    assert!(p.prices[0].amount.same_repr(Numeric::new(12_345_675, 100_000)));
}

proptest! {
    #[test]
    fn no_file_panics(text in "[0-9A-Za-z:,.\\-\"\n ]{0,120}") {
        let f = fx();
        let _ = f.propose(all_columns(), &text, false);
        let _ = f.propose(json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "CURRENCY::USD", "columns": ["Amount", "Date"] }), &text, true);
    }
}
