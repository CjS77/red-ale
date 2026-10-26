//! Business import: the invoice and bill reader and the customer and vendor reader, each with every separator,
//! against an empty book; and the cell parsers, GnuCash's expression parser and `qof_scan_date`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::business::OwnerKind;
use red_ale_core::ops::business::documents::DocumentKind;
use red_ale_core::{Book, CommodityRef, Date, FixedEnv, Time64, TimeZone};
use red_ale_import::business::date::{DateContext, DateFormat, scan_date};
use red_ale_import::business::expr::{NumberFormat, parse_expression};
use red_ale_import::business::{AccountNames, Context, DocumentOptions, OwnerOptions, Separator, documents, owners};
use red_ale_locale::Locale;

const NOW: i64 = 1_719_835_200;
const SEPARATORS: [Separator; 4] = [Separator::Semicolon, Separator::Comma, Separator::SemicolonQuoted, Separator::CommaQuoted];
const DATE_FORMATS: [DateFormat; 5] = [DateFormat::Us, DateFormat::Uk, DateFormat::Ce, DateFormat::Iso, DateFormat::Locale];

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let book = Book::new_empty(&mut FixedEnv::new(Time64(NOW), 1));
    let locale = Locale::c();
    let zone = TimeZone::utc();
    let Some(today) = Date::from_ymd(2024, 7, 1) else { return };
    let ctx = Context { book: &book, locale: &locale, today, now: Time64(NOW), zone: &zone, file: 0, threshold: None };
    let separator = SEPARATORS[usize::from(first % 4)];
    let date_format = DATE_FORMATS[usize::from((first >> 2) % 5)];
    let kind = if first & 0x20 == 0 { DocumentKind::Bill } else { DocumentKind::Invoice };
    let account_names = if first & 0x40 == 0 { AccountNames::Full } else { AccountNames::Leaf };
    let opts = DocumentOptions { kind, separator, date_format, account_names, update_existing: true, auto_pay: first & 0x80 != 0 };
    let _ = documents::propose(&text, &ctx, &opts);
    let owner = if first & 0x20 == 0 { OwnerKind::Customer } else { OwnerKind::Vendor };
    let _ = owners::propose(&text, &ctx, &OwnerOptions { kind: owner, separator, currency: CommodityRef::currency("USD") });

    let comma = NumberFormat { decimal_point: ',', group_separator: ' ' };
    let _ = parse_expression(&text, NumberFormat::default());
    let _ = parse_expression(&text, comma);
    let french = Locale::by_name("fr_FR").unwrap_or_else(|_| Locale::c());
    DATE_FORMATS.iter().for_each(|format| {
        let _ = scan_date(&text, &DateContext::new(*format, &french, today));
    });
});
