//! Schedule from transaction as `5.17:gnucash/gnome/dialog-sx-from-trans.cpp` builds it.

use super::*;
use crate::ops::commit::{NewSplit, NewTransaction};
use crate::ops::scheduled::fixture::{Fixture, date, usd};
use crate::ops::scheduled::{numeric_of, template_transactions};
use crate::ops::transactions::post;
use crate::{Numeric, Recurrence, TemplateSplit};

/// 1234.50 from Bank to Food on 2026-01-15, with num, notes, memos and an action.
fn posted(f: &mut Fixture) -> Guid {
    let mut t = NewTransaction::new(usd(), date(2026, 1, 15));
    t.description = Some("Groceries".into());
    t.num = Some("42".into());
    t.notes = Some("weekly shop".into());
    t.splits = vec![
        NewSplit {
            memo: Some("food".into()),
            action: Some("Buy".into()),
            ..NewSplit::new(f.food, Numeric::new(123_450, 100))
        },
        NewSplit {
            memo: Some("card".into()),
            ..NewSplit::new(f.bank, Numeric::new(-123_450, 100))
        },
    ];
    post(&mut f.book, &mut f.env, t).unwrap()
}

fn request(transaction: Guid, frequency: Frequency) -> FromTransaction {
    FromTransaction {
        transaction,
        name: None,
        frequency,
        start: None,
        end: End::Never,
        auto_create: None,
        notify: None,
        advance_create_days: None,
        advance_remind_days: None,
    }
}

type Row = (Guid, Option<String>, Option<String>, Option<String>, Option<String>);

fn splits(f: &Fixture, sx: Guid) -> Vec<Row> {
    template_transactions(&f.book, &f.book.scheduled[&sx])[0]
        .splits
        .iter()
        .map(|s| {
            let t = TemplateSplit::of(s).unwrap();
            let text = |x: Option<&str>| x.map(str::to_owned);
            (t.account().unwrap(), s.memo.clone(), s.action.clone(), text(t.debit_formula()), text(t.credit_formula()))
        })
        .collect()
}

/// `sxftd_compute_sx` (`:440-515`) and `sxftd_add_template_trans` (`:206-275`): named by the description, daily from
/// the day after the transaction, instance count 1, one template with the transaction's fields and each split's value
/// as a formula, a positive one as the debit; the numerics set, where GnuCash's path sets none.
#[test]
fn the_dialogs_defaults() {
    let mut f = Fixture::new();
    let t = posted(&mut f);
    let before = f.book.transactions[&t].clone();
    let edited = from_transaction(&mut f.book, &mut f.env, request(t, Frequency::Daily), NumberFormat::default()).unwrap();
    let sx = &f.book.scheduled[&edited.guid];
    assert_eq!((sx.name.as_str(), sx.start, sx.instance_count, sx.last), ("Groceries", date(2026, 1, 16), 1, None));
    assert_eq!(sx.schedule, [Recurrence::new(1, PeriodType::Day, date(2026, 1, 16), WeekendAdjust::None).unwrap()]);
    assert_eq!((sx.end, sx.occurrences, sx.auto_create, sx.auto_create_notify), (None, None, false, false));
    let template = template_transactions(&f.book, sx)[0];
    assert_eq!((template.description.as_deref(), template.num.as_deref(), template.notes()), (Some("Groceries"), Some("42"), Some("weekly shop")));
    assert_eq!(template.currency, usd());
    let text = |s: &str| Some(s.to_owned());
    assert_eq!(splits(&f, edited.guid), [
        (f.food, text("food"), text("Buy"), text("1234.50"), text("")),
        (f.bank, text("card"), None, text(""), text("1234.50")),
    ]);
    let numeric = TemplateSplit::of(&template.splits[0]).unwrap().debit_numeric();
    assert_eq!(numeric, Some(numeric_of("1234.50", NumberFormat::default())));
    assert_eq!(f.book.transactions[&t], before, "the transaction is not changed");
}

/// `sxftd_update_schedule` (`:278-327`): Yearly is month ×12, Bi-Weekly week ×2; the start is the frequency's first
/// occurrence after the transaction's date (`:417-430`).
#[test]
fn frequencies_and_their_first_occurrences() {
    let mut f = Fixture::new();
    let t = posted(&mut f);
    let cases = [
        (Frequency::Weekly, 1, PeriodType::Week, date(2026, 1, 22)),
        (Frequency::BiWeekly, 2, PeriodType::Week, date(2026, 1, 29)),
        (Frequency::Monthly, 1, PeriodType::Month, date(2026, 2, 15)),
        (Frequency::Quarterly, 3, PeriodType::Month, date(2026, 4, 15)),
        (Frequency::Yearly, 12, PeriodType::Month, date(2027, 1, 15)),
    ];
    cases.into_iter().for_each(|(frequency, mult, period_type, start)| {
        let mut r = request(t, frequency);
        r.name = Some(format!("{frequency:?}"));
        let edited = from_transaction(&mut f.book, &mut f.env, r, NumberFormat::default()).unwrap();
        let sx = &f.book.scheduled[&edited.guid];
        assert_eq!(sx.start, start, "{frequency:?}");
        assert_eq!(sx.schedule, [Recurrence::new(mult, period_type, start, WeekendAdjust::None).unwrap()]);
    });
}

/// The formula text is in the Locale's decimal form without grouping; the end and the editor's preferences are the
/// caller's.
#[test]
fn the_locale_the_end_and_the_preferences() {
    let mut f = Fixture::new();
    let t = posted(&mut f);
    let de = NumberFormat {
        decimal_point: ',',
        group_separator: '.',
    };
    let mut r = request(t, Frequency::Monthly);
    r.start = Some(date(2026, 3, 1));
    r.end = End::Occurrences { total: 6, remaining: 6 };
    r.auto_create = Some(true);
    r.advance_create_days = Some(2);
    let edited = from_transaction(&mut f.book, &mut f.env, r, de).unwrap();
    assert_eq!(splits(&f, edited.guid)[0].3.as_deref(), Some("1234,50"));
    let sx = &f.book.scheduled[&edited.guid];
    assert_eq!((sx.start, sx.auto_create, sx.auto_create_notify, sx.advance_create_days), (date(2026, 3, 1), true, true, 2));
    assert_eq!(sx.occurrences.map(|o| (o.total, o.remaining)), Some((6, 6)));
    assert!(matches!(
        from_transaction(&mut f.book, &mut f.env, request(Guid::from_bytes([9; 16]), Frequency::Daily), de),
        Err(ScheduledError::Domain(DomainError::NotFound { kind: "transaction", .. }))
    ));
}
