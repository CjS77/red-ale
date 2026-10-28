use std::sync::Arc;

use super::*;
use crate::ops::accounts::{AccountEdit, create_account};
use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
use crate::transaction::slot_paths;
use crate::{AccountType, CommodityRef, DENOM_AUTO, DenomMode, FixedEnv, How, Price, Round, Time64, TimeZone};

struct Fx {
    book: Book,
    env: FixedEnv,
}

fn day(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

fn monthly(start: Date) -> Recurrence {
    Recurrence::new(1, PeriodType::Month, start, WeekendAdjust::None).unwrap()
}

impl Fx {
    /// A book whose clock reads 2026-10-27 12:00 UTC.
    fn new() -> Self {
        let mut env = FixedEnv::new(Time64(1_793_102_400), 0);
        let book = Book::new_empty(&mut env);
        Self { book, env }
    }

    fn add_in(&mut self, parent: Option<Guid>, name: &str, t: AccountType, currency: &str) -> Guid {
        let edit = AccountEdit {
            name: Some(name.into()),
            account_type: Some(t),
            commodity: Some(CommodityRef::currency(currency)),
            ..AccountEdit::default()
        };
        let parent = parent.unwrap_or(self.book.root);
        create_account(&mut self.book, &mut self.env, parent, &edit).unwrap()
    }

    fn add(&mut self, name: &str, t: AccountType) -> Guid {
        self.add_in(None, name, t, "USD")
    }

    /// Post `cents` from `b` to `a` at an exact instant.
    fn post_at(&mut self, currency: &str, a: Guid, b: Guid, cents: i64, at: Time64) -> Guid {
        let mut t = NewTransaction::new(CommodityRef::currency(currency), day(2024, 1, 1));
        t.splits = vec![NewSplit::new(a, n(cents, 100)), NewSplit::new(b, n(-cents, 100))];
        let g = add_transaction(&mut self.book, &mut self.env, t).unwrap();
        Arc::make_mut(self.book.transactions.get_mut(&g).unwrap()).date_posted = at;
        g
    }

    fn price(&mut self, k: u8, d: Date, value: Numeric) {
        let p = Price {
            guid: Guid::from_bytes([k; 16]),
            commodity: CommodityRef::currency("EUR"),
            currency: CommodityRef::currency("USD"),
            time: d.neutral_time(),
            source: None,
            price_type: None,
            value,
        };
        self.book.prices.insert(p.guid, Arc::new(p));
    }

    fn create(&mut self, fields: BudgetFields) -> Guid {
        create(&mut self.book, &mut self.env, fields).unwrap()
    }

    fn budget(&self, g: Guid) -> &Budget {
        self.book.budget(g).unwrap()
    }

    fn set(&mut self, b: Guid, account: Guid, period: u32, amount: Option<Numeric>) -> DomainResult<(usize, usize)> {
        set_amounts(&mut self.book, b, &[(Cell { account, period }, amount)])
    }
}

fn utc(y: i32, m: u32, d: u32, secs: i64) -> Time64 {
    Time64(day(y, m, d).start_utc().0 + secs)
}

/// A Budget inserted as a file written before GnuCash 3.8 would hold it: no feature.
fn legacy(f: &mut Fx, amounts: &[(Guid, u32, Numeric)]) -> Guid {
    let mut slots = Frame::new();
    amounts
        .iter()
        .for_each(|(a, p, v)| slots.set_in(&[&a.to_string(), &p.to_string()], SlotValue::Numeric(*v)));
    let b = Budget {
        guid: f.env.new_guid(),
        name: "Old".into(),
        description: String::new(),
        num_periods: 12,
        recurrence: monthly(day(2019, 1, 1)),
        slots,
    };
    let g = b.guid;
    f.book.budgets.insert(g, Arc::new(b));
    g
}

#[test]
fn create_takes_gnucash_defaults_and_sets_the_feature() {
    // gnc_budget_init (gnc-budget.cpp:103-119); New Budget sets the feature (gnc-plugin-budget.c:181-186).
    let mut f = Fx::new();
    assert!(!uses_natural_signs(&f.book));
    let g = f.create(BudgetFields::default());
    let b = f.budget(g);
    assert_eq!((b.name.as_str(), b.description.as_str(), b.num_periods), ("Unnamed Budget", "", 12));
    assert_eq!(
        b.recurrence,
        monthly(day(2026, 10, 1)),
        "monthly from the first of the current month"
    );
    assert!(uses_natural_signs(&f.book));
    assert_eq!(
        f.book.slots.get_in(&[FEATURES, NATURAL_SIGNS_FEATURE]).and_then(SlotValue::as_str),
        Some("Store budget amounts unreversed (i.e. natural) signs (requires at least Gnucash 3.8)")
    );
    let second = f.create(BudgetFields {
        name: Some("Two".into()),
        ..BudgetFields::default()
    });
    assert_eq!(
        f.book.budgets.keys().copied().collect::<Vec<_>>(),
        [g, second],
        "new budgets go last"
    );
}

#[test]
fn create_and_update_refuse_what_the_dialog_cannot_write() {
    let mut f = Fx::new();
    let refused = |fields: BudgetFields, f: &mut Fx| create(&mut f.book, &mut f.env, fields).unwrap_err();
    let once = Recurrence::new(0, PeriodType::Once, day(2024, 1, 1), WeekendAdjust::None).unwrap();
    let back = Recurrence::new(1, PeriodType::Month, day(2024, 1, 1), WeekendAdjust::Back).unwrap();
    [Some(0), Some(61)].into_iter().for_each(|num_periods| {
        let fields = BudgetFields {
            num_periods,
            ..BudgetFields::default()
        };
        assert!(matches!(refused(fields, &mut f), DomainError::Invalid(_)));
    });
    [once, back].into_iter().for_each(|r| {
        let fields = BudgetFields {
            recurrence: Some(r),
            ..BudgetFields::default()
        };
        assert!(matches!(refused(fields, &mut f), DomainError::Invalid(_)));
    });
    assert!(f.book.budgets.is_empty() && !uses_natural_signs(&f.book), "nothing was created");
    // recurrenceSet moves an end-of-month start to the month's end (Recurrence.cpp:106-110).
    let g = f.create(BudgetFields {
        num_periods: Some(60),
        recurrence: Some(Recurrence::new(1, PeriodType::EndOfMonth, day(2024, 2, 10), WeekendAdjust::None).unwrap()),
        ..BudgetFields::default()
    });
    assert_eq!(f.budget(g).recurrence.start(), day(2024, 2, 29));
}

#[test]
fn amounts_are_stored_as_given_and_unset_is_not_zero() {
    // gnc_budget_set_account_period_value and _unset_ (gnc-budget.cpp:516-576).
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let g = f.create(BudgetFields::default());
    let edits = [
        (Cell { account: food, period: 0 }, Some(n(1234, 100))),
        (Cell { account: food, period: 1 }, Some(n(0, 1))),
        (Cell { account: food, period: 2 }, Some(n(0, 100))),
        (Cell { account: food, period: 3 }, None),
    ];
    assert_eq!(
        set_amounts(&mut f.book, g, &edits),
        Ok((3, 0)),
        "unsetting an unset cell does nothing"
    );
    let b = f.budget(g);
    assert!(b.amount(food, 0).unwrap().same_repr(n(1234, 100)), "not reduced");
    assert!(b.amount(food, 1).unwrap().same_repr(n(0, 1)), "an explicit zero is set");
    assert!(b.amount(food, 2).unwrap().same_repr(n(0, 100)));
    assert_eq!(b.amount(food, 3), None);
    assert_eq!(f.set(g, food, 1, None), Ok((0, 1)));
    assert_eq!(f.budget(g).amount(food, 1), None);
    let listed: Vec<(u32, Numeric)> = amounts(&f.book, f.budget(g)).iter().map(|(c, v)| (c.period, *v)).collect();
    assert_eq!(listed, [(0, n(1234, 100)), (2, n(0, 100))]);
}

#[test]
fn bad_cells_refuse_the_whole_call() {
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let g = f.create(BudgetFields::default());
    let root = f.book.root;
    let cell = |account, period| (Cell { account, period }, Some(n(1, 1)));
    let refusals = [
        vec![cell(food, 0), cell(food, 12)],
        vec![cell(food, 0), cell(food, 0)],
        vec![cell(food, 0), cell(root, 0)],
    ];
    refusals.iter().for_each(|edits| {
        assert!(
            matches!(set_amounts(&mut f.book, g, edits), Err(DomainError::Invalid(_))),
            "{edits:?}"
        );
    });
    let unknown = [cell(Guid::from_bytes([9; 16]), 0)];
    assert!(matches!(
        set_amounts(&mut f.book, g, &unknown),
        Err(DomainError::NotFound { kind: "account", .. })
    ));
    assert!(matches!(
        set_amounts(&mut f.book, Guid::from_bytes([8; 16]), &[cell(food, 0)]),
        Err(DomainError::NotFound { kind: "budget", .. })
    ));
    assert_eq!(f.budget(g).amount(food, 0), None, "nothing applied");
}

#[test]
fn notes_set_and_delete_and_empty_text_deletes() {
    // gnc_budget_set_account_period_note (gnc-budget.cpp:599-636); empty text is NULL (gnc-plugin-page-budget.cpp:1295-1296).
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let g = f.create(BudgetFields::default());
    let c = |period| Cell { account: food, period };
    let edits = [(c(0), Some("January & <more>".to_owned())), (c(1), Some("x".into())), (c(2), None)];
    assert_eq!(set_notes(&mut f.book, g, &edits), Ok((2, 0)));
    assert_eq!(set_notes(&mut f.book, g, &[(c(1), Some(String::new()))]), Ok((0, 1)));
    let b = f.budget(g);
    assert_eq!(b.note(food, 0), Some("January & <more>"));
    assert_eq!(b.note(food, 1), None);
    assert_eq!(notes(&f.book, b).len(), 1);
}

#[test]
fn lowering_the_count_hides_and_raising_it_shows_again() {
    // gnc_budget_set_num_periods keeps the Slots (gnc-budget.cpp:472-480; engine-budgets.md §1.3).
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let g = f.create(BudgetFields::default());
    f.set(g, food, 11, Some(n(5, 1))).unwrap();
    set_notes(&mut f.book, g, &[(Cell { account: food, period: 10 }, Some("n".into()))]).unwrap();
    let lower = BudgetFields {
        num_periods: Some(6),
        ..BudgetFields::default()
    };
    update(&mut f.book, g, lower).unwrap();
    let b = f.budget(g);
    assert!(amounts(&f.book, b).is_empty() && notes(&f.book, b).is_empty());
    let kept = kept_slots(&f.book, b);
    assert_eq!((kept.hidden_amounts, kept.hidden_notes, kept.orphaned_amounts), (1, 1, 0));
    let raise = BudgetFields {
        num_periods: Some(12),
        name: Some("Renamed".into()),
        ..BudgetFields::default()
    };
    update(&mut f.book, g, raise).unwrap();
    assert_eq!(f.budget(g).amount(food, 11), Some(n(5, 1)));
    assert_eq!(f.budget(g).name, "Renamed");
}

#[test]
fn orphaned_and_unknown_slots_are_counted_or_ignored_and_kept() {
    let mut f = Fx::new();
    let g = f.create(BudgetFields::default());
    let gone = Guid::from_bytes([7; 16]).to_string();
    let b = Arc::make_mut(f.book.budgets.get_mut(&g).unwrap());
    b.slots.set_in(&[&gone, "0"], SlotValue::Numeric(n(42, 1)));
    b.slots.set_in(&[NOTES, &gone, "0"], SlotValue::String("gone".into()));
    b.slots.set_in(&["unknown", "0"], SlotValue::Integer(1));
    let kept = kept_slots(&f.book, f.budget(g));
    assert_eq!((kept.orphaned_amounts, kept.orphaned_notes, kept.hidden_amounts), (1, 1, 0));
}

#[test]
fn copy_is_gnucash_clone_without_notes() {
    // gnc_budget_clone (gnc-budget.cpp:354-383), "Copy of %s" (gnc-plugin-budget.c:261).
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let g = f.create(BudgetFields {
        name: Some("Plan".into()),
        description: Some("d".into()),
        ..BudgetFields::default()
    });
    f.set(g, food, 3, Some(n(1234, 100))).unwrap();
    f.set(g, food, 11, Some(n(9, 1))).unwrap();
    set_notes(&mut f.book, g, &[(Cell { account: food, period: 3 }, Some("note".into()))]).unwrap();
    let lower = BudgetFields {
        num_periods: Some(10),
        ..BudgetFields::default()
    };
    update(&mut f.book, g, lower).unwrap();
    let c = copy(&mut f.book, &mut f.env, g).unwrap();
    let (src, cp) = (f.budget(g), f.budget(c));
    assert_eq!(
        (cp.name.as_str(), cp.description.as_str(), cp.num_periods),
        ("Copy of Plan", "d", 10)
    );
    assert_eq!(cp.recurrence, src.recurrence);
    assert!(cp.amount(food, 3).unwrap().same_repr(n(1234, 100)));
    assert!(notes(&f.book, cp).is_empty(), "notes are not copied");
    assert_eq!(kept_slots(&f.book, cp), KeptSlots::default(), "nor hidden amounts");
    assert_eq!(f.book.budgets.keys().last(), Some(&c));
}

#[test]
fn deleting_the_last_budget_unsets_the_feature() {
    // gnc-plugin-page-budget.cpp:944-948.
    let mut f = Fx::new();
    let a = f.create(BudgetFields::default());
    let b = f.create(BudgetFields::default());
    set_default_budget(&mut f.book, b).unwrap();
    assert_eq!(delete(&mut f.book, b), Ok(false));
    assert!(uses_natural_signs(&f.book));
    assert_eq!(default_budget_option(&f.book), Some(b), "the option is left stale");
    assert_eq!(default_budget(&f.book), Some((a, DefaultFrom::First)));
    assert_eq!(delete(&mut f.book, a), Ok(true));
    assert!(!uses_natural_signs(&f.book));
    assert_eq!(default_budget(&f.book), None);
    assert!(matches!(delete(&mut f.book, a), Err(DomainError::NotFound { .. })));
}

#[test]
fn the_default_budget_option_is_a_guid_slot() {
    let mut f = Fx::new();
    let a = f.create(BudgetFields::default());
    let b = f.create(BudgetFields::default());
    assert_eq!(default_budget(&f.book), Some((a, DefaultFrom::First)));
    set_default_budget(&mut f.book, b).unwrap();
    assert_eq!(f.book.slots.get("options/Budgeting/Default Budget"), Some(&SlotValue::Guid(b)));
    assert_eq!(default_budget(&f.book), Some((b, DefaultFrom::Option)));
    assert!(matches!(
        set_default_budget(&mut f.book, Guid::from_bytes([3; 16])),
        Err(DomainError::NotFound { .. })
    ));
}

#[test]
fn a_book_that_needs_the_scrub_refuses_edits_but_not_delete_or_scrub() {
    let mut f = Fx::new();
    let food = f.add("Food", AccountType::Expense);
    let old = legacy(&mut f, &[(food, 0, n(100, 1))]);
    assert!(needs_sign_scrub(&f.book));
    let refused = |r: DomainResult<()>| assert_eq!(r, Err(DomainError::NaturalSigns));
    refused(create(&mut f.book, &mut f.env, BudgetFields::default()).map(drop));
    refused(update(&mut f.book, old, BudgetFields::default()));
    refused(copy(&mut f.book, &mut f.env, old).map(drop));
    refused(f.set(old, food, 1, Some(n(1, 1))).map(drop));
    refused(set_notes(&mut f.book, old, &[(Cell { account: food, period: 0 }, None)]).map(drop));
    let e = Estimate {
        accounts: vec![food],
        start: day(2024, 1, 1),
        significant_digits: 1,
        use_average: false,
    };
    refused(estimate(&mut f.book, old, &e, &TimeZone::utc()).map(drop));
    let mut copy_book = f.book.clone();
    assert_eq!(delete(&mut copy_book, old), Ok(false), "no feature to unset");
    let report = scrub_signs(&mut f.book);
    assert!(report.feature_set && !needs_sign_scrub(&f.book));
    assert!(f.set(old, food, 1, Some(n(1, 1))).is_ok());
}

/// The four rows of exp2 (engine-budgets.md §7.2), from `ScrubBudget.c:105-167`.
#[test]
fn the_scrub_guesses_three_policies() {
    let mut f = Fx::new();
    let bank = f.add("Bank", AccountType::Bank);
    let card = f.add("Card", AccountType::Credit);
    let equity = f.add("Equity", AccountType::Equity);
    let salary = f.add("Salary", AccountType::Income);
    let food = f.add("Food", AccountType::Expense);
    let base = f.book.clone();
    let mut run = |amounts: &[(Guid, u32, Numeric)]| {
        f.book = base.clone();
        let g = legacy(&mut f, amounts);
        let report = scrub_signs(&mut f.book);
        assert!(report.feature_set && uses_natural_signs(&f.book));
        let b = f.book.budget(g).unwrap().clone();
        (report.budgets[0], b)
    };
    let (s, b) = run(&[
        (food, 0, n(-100, 1)),
        (salary, 0, n(500, 1)),
        (card, 0, n(50, 1)),
        (bank, 0, n(10, 1)),
    ]);
    assert_eq!((s.policy, s.flipped), (SignPolicy::IncomeExpense, 2));
    assert_eq!(
        [b.amount(food, 0), b.amount(salary, 0), b.amount(card, 0)],
        [Some(n(100, 1)), Some(n(-500, 1)), Some(n(50, 1))]
    );
    let (s, b) = run(&[(food, 0, n(100, 1)), (salary, 0, n(-500, 1)), (card, 0, n(50, 1))]);
    assert_eq!((s.policy, s.flipped, b.amount(card, 0)), (SignPolicy::None, 0, Some(n(50, 1))));
    let (s, b) = run(&[
        (food, 0, n(100, 1)),
        (salary, 0, n(500, 1)),
        (card, 0, n(50, 1)),
        (equity, 1, n(7, 1)),
        (bank, 2, n(-3, 1)),
    ]);
    assert_eq!((s.policy, s.flipped), (SignPolicy::CreditAccounts, 3));
    assert_eq!(
        [
            b.amount(food, 0),
            b.amount(salary, 0),
            b.amount(card, 0),
            b.amount(equity, 1),
            b.amount(bank, 2)
        ],
        [Some(n(100, 1)), Some(n(-500, 1)), Some(n(-50, 1)), Some(n(-7, 1)), Some(n(-3, 1))]
    );
    let (s, b) = run(&[(food, 0, n(0, 1)), (salary, 0, n(0, 1))]);
    assert_eq!(
        (s.policy, b.amount(salary, 0), b.amount(food, 1)),
        (SignPolicy::CreditAccounts, Some(n(0, 1)), None)
    );
    // An overflowing sum compares as 0 (gnc-numeric.cpp:837-842), so the negative expenses go unseen.
    let (s, _) = run(&[(food, 0, n(-i64::MAX, 1)), (food, 1, n(-i64::MAX, 1)), (salary, 0, n(5, 1))]);
    assert_eq!(s.policy, SignPolicy::CreditAccounts);
}

#[test]
fn the_scrub_without_budgets_unsets_the_feature_and_with_it_does_nothing() {
    let mut f = Fx::new();
    let g = f.create(BudgetFields::default());
    assert_eq!(scrub_signs(&mut f.book), ScrubReport::default(), "the feature is set: nothing");
    f.book.budgets.shift_remove(&g);
    let report = scrub_signs(&mut f.book);
    assert!(report.feature_unset && !uses_natural_signs(&f.book));
    assert_eq!(scrub_signs(&mut f.book), ScrubReport::default());
}

#[test]
fn period_dates_clamp_to_the_month_end() {
    // engine-budgets.md §1.4, exp4; utest-Budget.c's leap February.
    let dates = period_dates(&monthly(day(2024, 1, 31)), 3).unwrap();
    assert_eq!(
        dates,
        [
            (day(2024, 1, 31), day(2024, 2, 28)),
            (day(2024, 2, 29), day(2024, 3, 30)),
            (day(2024, 3, 31), day(2024, 4, 29))
        ]
    );
    let year = period_dates(&monthly(day(2012, 1, 1)), 12).unwrap();
    assert_eq!(year[1], (day(2012, 2, 1), day(2012, 2, 29)));
    assert_eq!(year[11], (day(2012, 12, 1), day(2012, 12, 31)));
}

/// exp4's book (engine-budgets.md §7.4): Exp in USD with a EUR child, a split at a period's last second, a closing
/// transaction, and EUR priced 1.10 then 1.30.
fn exp4() -> (Fx, Guid, Guid) {
    let mut f = Fx::new();
    let exp = f.add("Exp", AccountType::Expense);
    let eur_child = f.add_in(Some(exp), "EurChild", AccountType::Expense, "EUR");
    let bank = f.add("Bank", AccountType::Bank);
    let eur_bank = f.add_in(None, "EurBank", AccountType::Bank, "EUR");
    let equity = f.add("Equity", AccountType::Equity);
    f.post_at("USD", exp, bank, 1000, utc(2024, 1, 15, 39_540));
    f.post_at("USD", exp, bank, 100, utc(2024, 1, 31, 86_399));
    f.post_at("USD", exp, bank, 200, utc(2024, 2, 1, 0));
    let closing = f.post_at("USD", exp, equity, -500, utc(2024, 1, 31, 39_540));
    Arc::make_mut(f.book.transactions.get_mut(&closing).unwrap())
        .slots
        .set(slot_paths::BOOK_CLOSING, SlotValue::Integer(1));
    f.post_at("EUR", eur_child, eur_bank, 333, utc(2024, 1, 20, 39_540));
    f.price(1, day(2024, 1, 10), n(110, 100));
    f.price(2, day(2024, 2, 15), n(130, 100));
    let g = f.create(BudgetFields {
        num_periods: Some(6),
        recurrence: Some(monthly(day(2024, 1, 1))),
        ..BudgetFields::default()
    });
    (f, exp, g)
}

#[test]
fn actuals_follow_the_engine_with_strict_bounds_and_converted_children() {
    // gnc_budget_get_account_period_actual_value (gnc-budget.cpp:661-669; Account.cpp:3887-3923).
    let (f, exp, g) = exp4();
    let zone = TimeZone::utc();
    let b = f.budget(g);
    let actuals: Vec<Numeric> = (0..3).map(|p| actual(&f.book, b, exp, p, &zone).unwrap()).collect();
    assert!(
        actuals[0].same_repr(n(1366, 100)),
        "10.00 + EUR 3.33 at 1.10; the 23:59:59 split is in no period"
    );
    assert!(actuals[1].same_repr(n(200, 100)), "{:?}", actuals[1]);
    assert!(actuals[2].is_zero());
}

#[test]
fn estimate_reads_balances_converted_at_each_bound() {
    // estimate_budget_helper (gnc-plugin-page-budget.cpp:955-1005); exp4's figures.
    let (mut f, exp, g) = exp4();
    let zone = TimeZone::utc();
    let mut e = Estimate {
        accounts: vec![exp],
        start: day(2024, 1, 1),
        significant_digits: 6,
        use_average: false,
    };
    let cells = estimate(&mut f.book, g, &e, &zone).unwrap();
    let raw: Vec<Numeric> = cells.iter().map(|(_, v)| *v).collect();
    assert!(raw[0].same_repr(n(136_600, 10_000)), "{:?}", raw[0]);
    assert!(
        raw[1].same_repr(n(267_000, 100_000)),
        "the child's revaluation counts: {:?}",
        raw[1]
    );
    assert!(raw[2].same_repr(n(0, 1)), "a zero change is a set zero");
    assert_eq!(f.budget(g).amount(exp, 5), Some(n(0, 1)));
    e.significant_digits = 1;
    let rounded: Vec<Numeric> = estimate(&mut f.book, g, &e, &zone).unwrap().iter().map(|(_, v)| *v).collect();
    assert!(rounded[0].same_repr(n(14, 1)) && rounded[1].same_repr(n(3, 1)), "{rounded:?}");
    e.use_average = true;
    let avg = estimate(&mut f.book, g, &e, &zone).unwrap();
    assert!(avg.iter().all(|(_, v)| v.same_repr(n(3, 1))), "17.33 / 6 to one digit");
    e.significant_digits = 3;
    let avg = estimate(&mut f.book, g, &e, &zone).unwrap();
    assert!(avg[0].1.same_repr(n(289, 100)) && avg.len() == 6);
    let root = f.book.root;
    e.accounts = vec![root];
    assert!(matches!(estimate(&mut f.book, g, &e, &zone), Err(DomainError::Invalid(_))));
    e.accounts = vec![];
    assert!(matches!(estimate(&mut f.book, g, &e, &zone), Err(DomainError::Invalid(_))));
    e.accounts = vec![exp];
    e.significant_digits = 13;
    assert!(matches!(estimate(&mut f.book, g, &e, &zone), Err(DomainError::Invalid(_))));
}

#[test]
fn significant_digits_round_as_gnucash_does() {
    // GncNumeric::sigfigs_denom (gnc-numeric.cpp:468-486); the table of engine-budgets.md §3.4.
    let sig = |x: Numeric, d: u8| x.convert(DENOM_AUTO, How::new(DenomMode::SigFigs(d), Round::HalfUp)).unwrap();
    let table = [
        (n(123_456, 100), [n(1235, 1), n(1235, 1), n(1235, 1), n(123_456, 100)]),
        (n(56_789, 10_000), [n(6, 1), n(57, 10), n(568, 100), n(567_890, 100_000)]),
        (n(567, 1000), [n(6, 10), n(57, 100), n(567, 1000), n(567_000, 1_000_000)]),
        (n(567, 10_000), [n(6, 100), n(57, 1000), n(567, 10_000), n(567_000, 10_000_000)]),
        (n(999, 100), [n(10, 1), n(100, 10), n(999, 100), n(999_000, 100_000)]),
        (n(1, 1), [n(10, 10), n(100, 100), n(1000, 1000), n(1_000_000, 1_000_000)]),
        (n(98_765, 1), [n(98_765, 1), n(98_765, 1), n(98_765, 1), n(987_650, 10)]),
    ];
    table.iter().for_each(|(x, row)| {
        [1, 2, 3, 6].iter().zip(row).for_each(|(d, want)| {
            let got = sig(*x, *d);
            assert!(got.same_repr(*want), "{x:?} to {d}: {got:?}, want {want:?}");
        });
    });
    assert!(sig(n(-123_456, 100), 1).same_repr(n(-1235, 1)));
}
