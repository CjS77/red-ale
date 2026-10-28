//! Creating, changing and deleting scheduled transactions, each test citing the 5.17 lines it follows: the editor
//! (`5.17:gnucash/gnome/dialog-sx-editor.c`), `xaccSchedXactionInit` and `xaccSchedXactionSetTemplateTrans`
//! (`5.17:libgnucash/engine/SchedXaction.cpp`), and the template register (`5.17:gnucash/register/ledger-core/split-register-model-save.c`).

use std::sync::Arc;

use super::*;
use crate::formula::{FormulaError, Variables};
use crate::ops::scheduled::fixture::{Fixture, date, usd};
use crate::ops::scheduled::{AccountProblem, create_instance, numeric_of, template_transactions};
use crate::scheduled::{SPLIT_FRAME, TemplateSplit, Templates};
use crate::transaction::slot_paths;
use crate::{AccountType, Numeric, SlotValue};

fn monthly() -> Vec<RecurrenceSpec> {
    vec![RecurrenceSpec {
        mult: 1,
        period_type: PeriodType::Month,
        start: None,
        weekend_adjust: WeekendAdjust::None,
    }]
}

fn split(account: Guid, debit: &str, credit: &str) -> SplitSpec {
    SplitSpec {
        account,
        debit_formula: Some(debit.into()),
        credit_formula: Some(credit.into()),
        ..SplitSpec::default()
    }
}

fn rent(f: &Fixture, amount: &str) -> Fields {
    Fields {
        name: Some("Rent".into()),
        start: Some(date(2026, 1, 1)),
        schedule: Some(monthly()),
        templates: Some(vec![TemplateSpec {
            description: Some("Rent".into()),
            num: Some("CHK".into()),
            notes: Some("monthly".into()),
            splits: vec![split(f.food, amount, ""), split(f.bank, "", amount)],
            ..TemplateSpec::default()
        }]),
        ..Fields::default()
    }
}

fn make(f: &mut Fixture, fields: Fields) -> Result<Edited, ScheduledError> {
    create(&mut f.book, &mut f.env, fields, NumberFormat::default())
}

fn make_rent(f: &mut Fixture, amount: &str) -> Result<Edited, ScheduledError> {
    let fields = rent(f, amount);
    make(f, fields)
}

fn change(f: &mut Fixture, guid: Guid, fields: Fields) -> Result<Edited, ScheduledError> {
    update(&mut f.book, &mut f.env, guid, fields, NumberFormat::default())
}

/// A change to the rent's fields.
type Change = fn(&mut Fields);

/// A template split's Frame: account, debit formula and numeric, credit formula and numeric.
type FrameRow = (Option<Guid>, Option<String>, Option<Numeric>, Option<String>, Option<Numeric>);

fn frames(f: &Fixture, guid: Guid) -> Vec<FrameRow> {
    let sx = &f.book.scheduled[&guid];
    template_transactions(&f.book, sx)
        .iter()
        .flat_map(|t| t.splits.iter())
        .map(|s| {
            let t = TemplateSplit::of(s).unwrap();
            let text = |f: Option<&str>| f.map(str::to_owned);
            (
                t.account(),
                text(t.debit_formula()),
                t.debit_numeric(),
                text(t.credit_formula()),
                t.credit_numeric(),
            )
        })
        .collect()
}

fn reason(e: &ScheduledError) -> &'static str {
    match e {
        ScheduledError::Invalid { reason, .. } => reason,
        other => panic!("not a refusal: {other:?}"),
    }
}

/// `gnc_schedxaction_init` (`SchedXaction.cpp:71-88`) with the editor's instance count 1 (`dialog-sx-editor.c:1546-1549`):
/// enabled, no auto-create, nothing in advance, never run, no end; last in book order.
#[test]
fn a_new_scheduled_transaction_has_the_editors_defaults() {
    let mut f = Fixture::new();
    let edited = make_rent(&mut f, "750").unwrap();
    assert!(edited.warnings.is_empty(), "{:?}", edited.warnings);
    let sx = &f.book.scheduled[&edited.guid];
    assert_eq!(
        (sx.name.as_str(), sx.enabled, sx.auto_create, sx.auto_create_notify),
        ("Rent", true, false, false)
    );
    assert_eq!((sx.advance_create_days, sx.advance_remind_days, sx.instance_count), (0, 0, 1));
    assert_eq!((sx.last, sx.end, sx.occurrences), (None, None, None));
    assert!(sx.deferred.is_empty());
    assert_eq!(
        sx.schedule,
        vec![Recurrence::normalised(1, PeriodType::Month, date(2026, 1, 1), WeekendAdjust::None)]
    );
    assert_eq!(f.book.scheduled.keys().last(), Some(&edited.guid));
}

/// The template account `xaccSchedXactionInit` makes (`SchedXaction.cpp:393-406`), under a template root made as
/// `sxtg_book_begin` makes it (`5.17:libgnucash/engine/SX-book.cpp:113-125`): ROOT, `Template Root`, no commodity.
#[test]
fn the_template_account_is_named_by_the_guid_under_a_new_template_root() {
    let mut f = Fixture::new();
    f.book.templates = Templates::default();
    let edited = make_rent(&mut f, "750").unwrap();
    let root = f.book.templates.root.unwrap();
    let r = &f.book.templates.accounts[&root];
    assert_eq!(
        (r.name.as_str(), r.account_type, r.commodity.as_ref(), r.parent),
        ("Template Root", AccountType::Root, None, None)
    );
    let account = f.book.scheduled[&edited.guid].template_account.unwrap();
    let a = &f.book.templates.accounts[&account];
    assert_eq!(a.name, edited.guid.to_string());
    assert_eq!(
        (a.account_type, a.parent, a.commodity_scu, a.non_standard_scu),
        (AccountType::Bank, Some(root), Some(1), false)
    );
    assert_eq!(a.commodity.as_ref().map(ToString::to_string).as_deref(), Some("template::template"));
    assert!(f.book.account(account).is_none(), "template accounts are not in the account tree");
}

/// `xaccSchedXactionSetTemplateTrans` (`SchedXaction.cpp:1007-1029`): description, num, notes, currency, posted
/// today; each split in the template account with value 0 in the currency's fraction and amount 0
/// (`split-register-model-save.c:731-733`), its numerics as `save_cell` sets them (`:687-713`).
#[test]
fn template_transactions_are_written_as_the_register_writes_them() {
    let mut f = Fixture::new();
    let mut fields = rent(&f, "1,234.50");
    if let Some(t) = fields.templates.as_mut() {
        t[0].splits[0].debit_formula = Some("rent * 2".into());
        t[0].splits[1].credit_formula = Some("rent * 2".into());
        t[0].splits.push(SplitSpec {
            memo: Some("marker".into()),
            action: Some("Pay".into()),
            ..split(f.bank, "", "")
        });
        t[0].splits.push(split(f.food, "(5)", ""));
        t[0].splits.push(split(f.bank, "", "-5"));
    }
    let edited = make(&mut f, fields).unwrap();
    let sx = &f.book.scheduled[&edited.guid];
    let t = template_transactions(&f.book, sx)[0];
    assert_eq!(
        (t.description.as_deref(), t.num.as_deref(), t.notes(), &t.currency),
        (Some("Rent"), Some("CHK"), Some("monthly"), &usd())
    );
    assert_eq!(t.slots.get(slot_paths::DATE_POSTED), Some(&SlotValue::Date(date(2026, 1, 1))));
    assert_eq!(t.date_entered, f.env.now);
    assert!(
        t.splits
            .iter()
            .all(|s| s.value.same_repr(Numeric::new(0, 100)) && s.amount.same_repr(Numeric::new(0, 1)))
    );
    assert!(t.splits.iter().all(|s| s.account == sx.template_account));
    assert_eq!(
        (t.splits[2].memo.as_deref(), t.splits[2].action.as_deref()),
        (Some("marker"), Some("Pay"))
    );
    let zero = Some(Numeric::new(0, 1));
    let text = |s: &str| Some(s.to_owned());
    assert_eq!(
        frames(&f, edited.guid),
        vec![
            (Some(f.food), text("rent * 2"), zero, text(""), zero),
            (Some(f.bank), text(""), zero, text("rent * 2"), zero),
            (Some(f.bank), text(""), zero, text(""), zero),
            (Some(f.food), text("(5)"), Some(Numeric::new(-5, 1)), text(""), zero),
            (Some(f.bank), text(""), zero, text("-5"), Some(Numeric::new(-5, 1))),
        ]
    );
    let frame = t.splits[0].slots.get_key(SPLIT_FRAME).and_then(SlotValue::as_frame).unwrap();
    let keys: Vec<&str> = frame.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["account", "credit-formula", "credit-numeric", "debit-formula", "debit-numeric"],
        "no `shares`"
    );
}

/// A constant formula's numeric is its value, reduced as the parser leaves it (`gnc-exp-parser.c:546`): `1,234.50`
/// under the default format is 2469/2, which GnuCash's open-time scrub would also store
/// (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:113-174`).
#[test]
fn a_constant_formulas_numeric_is_its_value() {
    let mut f = Fixture::new();
    let edited = make_rent(&mut f, "1,234.50").unwrap();
    let value = Numeric::new(2469, 2);
    assert_eq!(frames(&f, edited.guid)[0].2, Some(value));
    assert_eq!(frames(&f, edited.guid)[1].4, Some(value));
    assert!(value.same_repr(numeric_of("1,234.50", NumberFormat::default())));
}

/// `recurrenceSet` normalises what red-ale stores (`5.17:libgnucash/engine/Recurrence.cpp:87-139`): multiplier 0
/// becomes 1, an end-of-month start moves to its month's end, a weekend adjustment on a weekly recurrence is dropped;
/// a recurrence with no start starts on the start date (`dialog-sx-editor.c:1009-1023`).
#[test]
fn recurrences_are_normalised_and_start_on_the_start_date() {
    let mut f = Fixture::new();
    let mut fields = rent(&f, "750");
    fields.start = Some(date(2026, 1, 10));
    fields.schedule = Some(vec![
        RecurrenceSpec {
            mult: 0,
            period_type: PeriodType::EndOfMonth,
            start: None,
            weekend_adjust: WeekendAdjust::Back,
        },
        RecurrenceSpec {
            mult: 2,
            period_type: PeriodType::Week,
            start: Some(date(2026, 1, 3)),
            weekend_adjust: WeekendAdjust::Forward,
        },
    ]);
    let edited = make(&mut f, fields).unwrap();
    assert_eq!(
        f.book.scheduled[&edited.guid].schedule,
        vec![
            Recurrence::new(1, PeriodType::EndOfMonth, date(2026, 1, 31), WeekendAdjust::Back).unwrap(),
            Recurrence::new(2, PeriodType::Week, date(2026, 1, 3), WeekendAdjust::None).unwrap(),
        ]
    );
}

/// The editor refuses an empty name (`dialog-sx-editor.c:504-515`), an occurrence count of 0 or fewer remaining than
/// … more remaining than in all (`:547-590`), notify without auto-create (`:976-985`, `:1031-1043`), and the engine
/// an end before the start (`SchedXaction.cpp:625-648`).
#[test]
fn the_editors_refusals() {
    let mut f = Fixture::new();
    let with = |change: fn(&mut Fields), f: &Fixture| {
        let mut fields = rent(f, "750");
        change(&mut fields);
        fields
    };
    let cases: [(Change, &str); 9] = [
        (|x| x.name = Some(String::new()), "name"),
        (|x| x.name = None, "name"),
        (|x| x.schedule = Some(Vec::new()), "schedule"),
        (|x| x.end = Some(End::Occurrences { total: 0, remaining: 0 }), "end"),
        (|x| x.end = Some(End::Occurrences { total: 2, remaining: 3 }), "end"),
        (|x| x.end = Some(End::Date(date(2025, 12, 31))), "end"),
        (|x| x.notify = Some(true), "notify"),
        (|x| x.advance_remind_days = Some(-1), "advance-days"),
        (
            |x| x.templates.as_mut().unwrap()[0].currency = Some(crate::CommodityRef::new("NASDAQ", "ACME")),
            "currency",
        ),
    ];
    cases.into_iter().for_each(|(c, expected)| {
        let fields = with(c, &f);
        let e = make(&mut f, fields).unwrap_err();
        assert_eq!(reason(&e), expected, "{e}");
    });
}

/// A template split must go to an account red-ale's commit would take a split in: GnuCash's editor checks that the
/// account exists (`dialog-sx-editor.c:742-754`); red-ale also refuses a placeholder and the root.
#[test]
fn template_split_accounts_are_checked() {
    let mut f = Fixture::new();
    Arc::make_mut(f.book.accounts.get_mut(&f.food).unwrap()).set_placeholder(true);
    let problem = |f: &mut Fixture, account: Guid| match make(
        f,
        Fields {
            templates: Some(vec![TemplateSpec {
                currency: Some(usd()),
                splits: vec![split(account, "1", ""), split(f.bank, "", "1")],
                ..TemplateSpec::default()
            }]),
            ..rent(f, "1")
        },
    ) {
        Err(ScheduledError::Account { problem, .. }) => problem,
        other => panic!("{other:?}"),
    };
    let (food, root, nowhere) = (f.food, f.book.root, Guid::from_bytes([0xee; 16]));
    assert_eq!(problem(&mut f, food), AccountProblem::Placeholder(food));
    assert_eq!(problem(&mut f, root), AccountProblem::Root(root));
    assert_eq!(problem(&mut f, nowhere), AccountProblem::Missing(Some(nowhere)));
}

/// "Split with memo %s has an unparsable Credit Formula." (`dialog-sx-editor.c:766-790`); a call red-ale can't
/// compute, and a constant formula that fails, are refused with it.
#[test]
fn formulas_must_parse_and_be_computable() {
    let mut f = Fixture::new();
    let reasons = [
        "1 +",
        "nosuch(rent)",
        "cpd_pmt(rate : 12 : 2 : 360 : 100000 : 0 : 0)",
        "pmt(rent : 360)",
        "pmt(0 : 12 : 1200 : 0 : 0)",
    ]
    .map(|text| {
        let mut fields = rent(&f, "750");
        fields.templates.as_mut().unwrap()[0].splits[1].credit_formula = Some(text.into());
        match make(&mut f, fields) {
            Err(ScheduledError::Formula(failure)) => failure.error.reason(),
            other => panic!("{text}: {other:?}"),
        }
    });
    assert_eq!(reasons, ["syntax", "unknown-function", "inexact", "arguments", "division-by-zero"]);
}

/// With no variable and one commodity, the editor's sums are certain (`dialog-sx-editor.c:803-904`): an unbalanced
/// template is refused. With a variable, or two commodities, it can't tell, and red-ale accepts.
#[test]
fn a_template_that_provably_does_not_balance_is_refused() {
    let mut f = Fixture::new();
    let mut fields = rent(&f, "750");
    fields.templates.as_mut().unwrap()[0].splits[1].credit_formula = Some("700".into());
    match make(&mut f, fields).unwrap_err() {
        ScheduledError::Domain(DomainError::Unbalanced { imbalance, currency }) => {
            assert_eq!((imbalance.as_str(), currency.as_str()), ("50/1", "USD"));
        }
        other => panic!("{other:?}"),
    }
    let mut fields = rent(&f, "750");
    fields.templates.as_mut().unwrap()[0].splits[1].credit_formula = Some("rent".into());
    assert!(make(&mut f, fields).is_ok());
    let mut fields = rent(&f, "750");
    fields.templates.as_mut().unwrap()[0].splits[1] = split(f.euro, "", "700");
    assert!(make(&mut f, fields).is_ok());
}

/// `gnc_sxed_check_autocreate` (`dialog-sx-editor.c:617-648`): no auto-create with variables other than `i`, with
/// two commodities, or with no template split. Notify defaults to on with it.
#[test]
fn auto_create_is_refused_where_the_editor_refuses_it() {
    let mut f = Fixture::new();
    let auto = |mut fields: Fields| {
        fields.auto_create = Some(true);
        fields
    };
    let fields = auto(rent(&f, "750 + i - i"));
    let ok = make(&mut f, fields).unwrap();
    assert!(f.book.scheduled[&ok.guid].auto_create_notify);
    let mut variable = rent(&f, "rent");
    variable.templates.as_mut().unwrap()[0].splits[1].credit_formula = Some("rent".into());
    let mut euro = rent(&f, "750");
    euro.templates.as_mut().unwrap()[0].splits[1] = split(f.euro, "", "750");
    let empty = Fields {
        templates: None,
        ..rent(&f, "750")
    };
    [variable, euro, empty].into_iter().for_each(|fields| {
        let e = make(&mut f, auto(fields)).unwrap_err();
        assert_eq!(reason(&e), "auto-create", "{e}");
    });
}

/// The editor's questions become warnings: a name already used (`dialog-sx-editor.c:536-545`), asked only when the
/// name is new or changed; a schedule with no occurrence by its end date (`:594-611`), not asked with a count.
#[test]
fn the_editors_questions_are_warnings() {
    let mut f = Fixture::new();
    let first = make_rent(&mut f, "750").unwrap();
    let second = make_rent(&mut f, "750").unwrap();
    assert_eq!(second.warnings, [Warning::DuplicateName("Rent".into())]);
    let renamed = change(
        &mut f,
        first.guid,
        Fields {
            enabled: Some(false),
            ..Fields::default()
        },
    )
    .unwrap();
    assert!(renamed.warnings.is_empty(), "the name did not change");
    let mut once = rent(&f, "750");
    once.name = Some("Once".into());
    once.start = Some(date(2026, 1, 15));
    once.schedule = Some(vec![RecurrenceSpec {
        mult: 0,
        period_type: PeriodType::Once,
        start: Some(date(2026, 1, 10)),
        weekend_adjust: WeekendAdjust::None,
    }]);
    assert_eq!(make(&mut f, once.clone()).unwrap().warnings, [Warning::NeverRuns]);
    once.name = Some("Once, counted".into());
    once.end = Some(End::Occurrences { total: 1, remaining: 1 });
    assert!(make(&mut f, once).unwrap().warnings.is_empty());
    let mut ends = rent(&f, "750");
    ends.name = Some("Ends".into());
    ends.end = Some(End::Date(date(2026, 1, 1)));
    assert!(
        make(&mut f, ends.clone()).unwrap().warnings.is_empty(),
        "the start is an occurrence"
    );
    ends.name = Some("Ends early".into());
    ends.start = Some(date(2026, 1, 2));
    ends.schedule.as_mut().unwrap()[0].start = Some(date(2026, 1, 1));
    ends.end = Some(End::Date(date(2026, 1, 31)));
    assert_eq!(make(&mut f, ends).unwrap().warnings, [Warning::NeverRuns]);
}

/// `gnc_sxed_save_sx` (`dialog-sx-editor.c:928-964`) sets one end and clears the other; only Since Last Run moves the
/// temporal state.
#[test]
fn an_update_keeps_one_end_and_the_temporal_state() {
    let mut f = Fixture::new();
    let g = make_rent(&mut f, "750").unwrap().guid;
    Arc::make_mut(f.book.scheduled.get_mut(&g).unwrap()).last = Some(date(2026, 2, 1));
    let end = |f: &mut Fixture, end| {
        change(
            f,
            g,
            Fields {
                end: Some(end),
                ..Fields::default()
            },
        )
        .unwrap();
        let sx = &f.book.scheduled[&g];
        (sx.end, sx.occurrences, sx.last, sx.instance_count)
    };
    let last = Some(date(2026, 2, 1));
    let occurrences = Some(Occurrences { total: 12, remaining: 10 });
    assert_eq!(
        end(&mut f, End::Date(date(2026, 12, 31))),
        (Some(date(2026, 12, 31)), None, last, 1)
    );
    assert_eq!(
        end(&mut f, End::Occurrences { total: 12, remaining: 10 }),
        (None, occurrences, last, 1)
    );
    assert_eq!(end(&mut f, End::Never), (None, None, last, 1));
}

/// Replacing the template transactions keeps the GUIDs given, in their places, with a kept formula's numeric as read;
/// the others are new and those left out are deleted (`xaccSchedXactionSetTemplateTrans` deletes them,
/// `SchedXaction.cpp:999-1029`).
#[test]
fn an_update_replaces_the_templates_keeping_the_guids_given() {
    let mut f = Fixture::new();
    let mut fields = rent(&f, "750");
    let second = fields.templates.as_ref().unwrap()[0].clone();
    fields.templates.as_mut().unwrap().push(second);
    let g = make(&mut f, fields).unwrap().guid;
    let before: Vec<Guid> = f.book.templates.transactions.keys().copied().collect();
    let kept = before[1];
    let kept_split = f.book.templates.transactions[&kept].splits[0].guid;
    let stale = Numeric::new(1, 1);
    let t = Arc::make_mut(f.book.templates.transactions.get_mut(&kept).unwrap());
    let SlotValue::Frame(frame) = t.splits[0].slots.get_key(SPLIT_FRAME).cloned().unwrap() else {
        panic!()
    };
    let mut frame = frame;
    frame.insert_key("debit-numeric", SlotValue::Numeric(stale));
    frame.insert_key("shares", SlotValue::String("(x + y)/42".into()));
    t.splits[0].slots.insert_key(SPLIT_FRAME, SlotValue::Frame(frame));
    let replaced = TemplateSpec {
        guid: Some(kept),
        description: Some("Rent, kept".into()),
        splits: vec![
            SplitSpec {
                guid: Some(kept_split),
                ..split(f.food, "750", "")
            },
            split(f.bank, "", "750"),
        ],
        ..TemplateSpec::default()
    };
    let new = TemplateSpec {
        description: Some("New".into()),
        splits: vec![split(f.food, "5", ""), split(f.bank, "", "5")],
        ..TemplateSpec::default()
    };
    change(
        &mut f,
        g,
        Fields {
            templates: Some(vec![new, replaced]),
            ..Fields::default()
        },
    )
    .unwrap();
    let after: Vec<Guid> = f.book.templates.transactions.keys().copied().collect();
    assert_eq!(
        (after.len(), after[0]),
        (2, kept),
        "the first is deleted, the kept one stays in its place"
    );
    let t = &f.book.templates.transactions[&kept];
    assert_eq!(
        (t.description.as_deref(), t.num.as_deref(), t.notes()),
        (Some("Rent, kept"), None, None)
    );
    assert_eq!(t.splits[0].guid, kept_split);
    let template = TemplateSplit::of(&t.splits[0]).unwrap();
    assert_eq!((template.debit_numeric(), template.shares()), (Some(stale), Some("(x + y)/42")));
    let e = change(
        &mut f,
        g,
        Fields {
            templates: Some(vec![TemplateSpec {
                guid: Some(before[0]),
                ..TemplateSpec::default()
            }]),
            ..Fields::default()
        },
    )
    .unwrap_err();
    assert_eq!(reason(&e), "template-guid");
}

/// `xaccSchedXactionFree` (`SchedXaction.cpp:462-488`) destroys the template transactions and the template account;
/// the transactions it created stay.
#[test]
fn delete_takes_the_template_account_and_transactions() {
    let mut f = Fixture::new();
    let gone = make_rent(&mut f, "750").unwrap().guid;
    let stays = make_rent(&mut f, "5").unwrap().guid;
    let i = Variables::from([("i".to_owned(), Numeric::from_int(1))]);
    create_instance(&mut f.book, &mut f.env, gone, date(2026, 2, 1), &i, NumberFormat::default()).unwrap();
    let account = f.book.scheduled[&gone].template_account.unwrap();
    delete(&mut f.book, gone).unwrap();
    assert!(!f.book.scheduled.contains_key(&gone));
    assert!(!f.book.templates.accounts.contains_key(&account));
    assert_eq!(f.book.templates.transactions.len(), 1);
    assert_eq!(template_transactions(&f.book, &f.book.scheduled[&stays]).len(), 1);
    assert_eq!(f.book.transactions.len(), 1, "the created transaction stays");
    assert!(matches!(delete(&mut f.book, gone), Err(DomainError::NotFound { .. })));
}

/// What red-ale writes, Since Last Run creates from: the rent's instance is 750 from Bank to Food
/// (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1310-1422`).
#[test]
fn since_last_run_creates_from_a_new_template() {
    let mut f = Fixture::new();
    let g = make_rent(&mut f, "750").unwrap().guid;
    let i = Variables::from([("i".to_owned(), Numeric::from_int(1))]);
    let created = create_instance(&mut f.book, &mut f.env, g, date(2026, 1, 1), &i, NumberFormat::default()).unwrap();
    let t = &f.book.transactions[&created[0].transaction];
    let values: Vec<(Option<Guid>, Numeric)> = t.splits.iter().map(|s| (s.account, s.value)).collect();
    assert_eq!(
        values,
        [(Some(f.food), Numeric::new(75000, 100)), (Some(f.bank), Numeric::new(-75000, 100))]
    );
    assert_eq!(t.description.as_deref(), Some("Rent"));
}

#[test]
fn a_formula_error_names_the_split() {
    let mut f = Fixture::new();
    let mut fields = rent(&f, "750");
    fields.templates.as_mut().unwrap()[0].splits[0].debit_formula = Some("1 2".into());
    let Err(ScheduledError::Formula(failure)) = make(&mut f, fields) else {
        panic!()
    };
    assert_eq!((failure.side, failure.formula.as_str()), (super::super::Side::Debit, "1 2"));
    assert!(matches!(failure.error, FormulaError::Unexpected(_)));
}
