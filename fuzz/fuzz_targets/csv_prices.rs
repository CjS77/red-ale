//! CSV price import: tokenize, `PriceImport::parse`, the file's errors and the proposal, with two of GnuCash's
//! layouts, against a book holding a security.

#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use red_ale_core::{Book, Commodity, CommodityRef, Date, FixedEnv, Time64, TimeZone};
use red_ale_import::csv::prices::props::PriceContext;
use red_ale_import::csv::prices::{Defaults, PriceImport, propose};
use red_ale_import::csv::{DateContext, MonetaryLocale, PriceLayout, tokenize};
use serde_json::json;

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let mut book = Book::new_empty(&mut FixedEnv::new(Time64(1_711_000_000), 1));
    let acme = CommodityRef::new("NASDAQ", "ACME");
    book.commodities.insert(acme.clone(), Arc::new(Commodity::new(acme, Some("Acme Corp".to_owned()), 10_000)));
    let layout = if first & 1 == 0 {
        json!({ "skipStartLines": 1, "columns": ["Date", "From Symbol", "From Namespace", "Currency To", "Amount"] })
    } else {
        json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "CURRENCY::USD", "columns": ["Amount", "Date"], "currencyFormat": "comma" })
    };
    let Ok(layout) = serde_json::from_value::<PriceLayout>(layout) else { return };
    let layout = layout.sanitized();
    let Ok(defaults) = Defaults::resolve(&book, &layout) else { return };
    let Ok(rows) = tokenize(&text, &layout.csv) else { return };
    let Some(today) = Date::from_ymd(2024, 3, 26) else { return };
    let ctx = PriceContext {
        date_format: layout.csv.date_format,
        currency_format: layout.csv.currency_format,
        dates: DateContext { today, locale_date: None },
        monetary: MonetaryLocale::default(),
    };
    let import = PriceImport::parse(&book, rows, &layout, &defaults, &ctx);
    if import.file_errors(&defaults).is_empty() {
        let _ = propose(&book, &TimeZone::utc(), &import, first & 2 != 0, 0);
    }
});
