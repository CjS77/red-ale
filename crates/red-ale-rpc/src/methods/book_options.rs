//! Methods: the Book options red-ale reads, sets and follows: the num field source and the auto read-only threshold
//! (`design/api/book-options.md`), trading accounts (`design/api/trading-accounts.md`), the default budget
//! (`design/api/budgets.md`) and, from v0.6, the business options an invoice prints. The rules live in
//! `red_ale_core::book_options` and `red_ale_core::ops::budgets`.

use red_ale_core::book_options::{self as opts, BusinessOption, MAX_AUTO_READONLY_DAYS};
use red_ale_core::ops::budgets;
use red_ale_core::{Book, Env};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::methods::transactions::today;
use crate::wire::{Day, Id, NoParams};

/// The book options methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "book.options",
            "The book's num field source, auto read-only threshold (with today's threshold date), trading accounts option, default budget, business options and default invoice report.",
            Effect::Read,
            get,
        ),
        method(
            "book.setOptions",
            "Set the num field source, the auto read-only threshold, trading accounts, the default budget, the business options or the default invoice report, as GnuCash's book options dialog does.",
            Effect::Write,
            set,
        ),
    ]
}

/// The book options, as `book.options` and `book.setOptions` return them.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookOptions {
    /// "Use Split Action Field for Number": a split's Number is its action, and the transaction's num is the T-Num.
    pub split_action_for_num: bool,
    /// "Day Threshold for Read-Only Transactions (red line)": 0 when off.
    pub auto_read_only_days: i64,
    /// Today minus the threshold's days: transactions posted before it are read-only. Absent when the threshold is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_threshold: Option<Day>,
    /// "Use Trading Accounts": every commit keeps the trading splits of multi-commodity transactions.
    pub use_trading_accounts: bool,
    /// "Default Budget": the GUID the option holds, or null when it is unset. A GUID naming no budget is returned as
    /// stored; `budget.list` gives the default in effect.
    pub default_budget: Option<Id>,
    /// The Business tab's "Company Name"; `""` when unset (v0.6).
    pub company_name: String,
    /// "Company Address": lines separated by newlines (v0.6).
    pub company_address: String,
    /// "Company Contact Person" (v0.6).
    pub company_contact_person: String,
    /// "Company Phone Number" (v0.6).
    pub company_phone_number: String,
    /// "Company Fax Number" (v0.6).
    pub company_fax_number: String,
    /// "Company Email Address" (v0.6).
    pub company_email_address: String,
    /// "Company Website URL" (v0.6).
    pub company_website_url: String,
    /// "Company ID", such as a tax ID (v0.6).
    pub company_id: String,
    /// The Tax tab's "Tax Number" (v0.6).
    pub tax_number: String,
    /// "Default Invoice Report": the report GnuCash's Print Invoice runs, as GnuCash parses the slot; null when unset
    /// or unreadable. The GUID is returned as stored and may name no report red-ale has (v0.6).
    pub default_invoice_report: Option<InvoiceReport>,
}

/// The Default Invoice Report as `book.options` reads it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvoiceReport {
    /// The report's GUID: a template or a saved configuration, as stored.
    pub guid: String,
    /// The name stored beside it, `""` when none.
    pub name: String,
}

/// The Default Invoice Report as `book.setOptions` writes it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InvoiceReportParam {
    /// The report's GUID: a template or a saved configuration. It is not checked against the reports red-ale has,
    /// since GnuCash may print the Book with reports red-ale lacks.
    pub guid: Id,
    /// The name to store beside it, as GnuCash stores the chosen report's name.
    pub name: String,
}

/// Params of `book.setOptions`; an absent option is left as it is.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetParams {
    /// Use the split action as the Number. Turning it on also sets the Book feature `Number Field Source`.
    #[serde(default)]
    pub split_action_for_num: Option<bool>,
    /// Days after which transactions become read-only: 0 (off) to 3650.
    #[serde(default)]
    #[schemars(range(min = 0, max = MAX_AUTO_READONLY_DAYS))]
    pub auto_read_only_days: Option<i64>,
    /// Balance multi-commodity transactions through trading accounts. No existing transaction changes.
    #[serde(default)]
    pub use_trading_accounts: Option<bool>,
    /// The default budget: a budget of the book. It cannot be unset.
    #[serde(default)]
    pub default_budget: Option<Id>,
    /// "Company Name"; `""` clears it (v0.6).
    #[serde(default)]
    pub company_name: Option<String>,
    /// "Company Address", lines separated by newlines (v0.6).
    #[serde(default)]
    pub company_address: Option<String>,
    /// "Company Contact Person" (v0.6).
    #[serde(default)]
    pub company_contact_person: Option<String>,
    /// "Company Phone Number" (v0.6).
    #[serde(default)]
    pub company_phone_number: Option<String>,
    /// "Company Fax Number" (v0.6).
    #[serde(default)]
    pub company_fax_number: Option<String>,
    /// "Company Email Address" (v0.6).
    #[serde(default)]
    pub company_email_address: Option<String>,
    /// "Company Website URL" (v0.6).
    #[serde(default)]
    pub company_website_url: Option<String>,
    /// "Company ID" (v0.6).
    #[serde(default)]
    pub company_id: Option<String>,
    /// The Tax tab's "Tax Number" (v0.6).
    #[serde(default)]
    pub tax_number: Option<String>,
    /// "Default Invoice Report": `{ guid, name }`, or null to clear it (v0.6).
    #[serde(default, deserialize_with = "present")]
    #[allow(clippy::option_option)]
    pub default_invoice_report: Option<Option<InvoiceReportParam>>,
}

impl SetParams {
    /// The business strings given, each with the option it sets.
    fn business(&self) -> impl Iterator<Item = (BusinessOption, &str)> {
        let given = [
            &self.company_name,
            &self.company_address,
            &self.company_contact_person,
            &self.company_phone_number,
            &self.company_fax_number,
            &self.company_email_address,
            &self.company_website_url,
            &self.company_id,
            &self.tax_number,
        ];
        BusinessOption::ALL
            .into_iter()
            .zip(given)
            .filter_map(|(option, value)| value.as_deref().map(|v| (option, v)))
    }
}

/// Read a present field as `Some(value)`, so that `null` (clear) and absent (keep) differ.
#[allow(clippy::option_option, clippy::unnecessary_wraps)]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

fn describe(book: &Book, env: &mut dyn Env) -> Result<BookOptions, RpcError> {
    let today = today(env)?;
    let business = |option| opts::business_option(book, option).to_owned();
    Ok(BookOptions {
        split_action_for_num: opts::uses_split_action_for_num(book),
        auto_read_only_days: opts::auto_readonly_days(book),
        read_only_threshold: opts::auto_readonly_threshold(book, today).map(Day),
        use_trading_accounts: opts::uses_trading_accounts(book),
        default_budget: budgets::default_budget_option(book).map(Id),
        company_name: business(BusinessOption::CompanyName),
        company_address: business(BusinessOption::CompanyAddress),
        company_contact_person: business(BusinessOption::ContactPerson),
        company_phone_number: business(BusinessOption::PhoneNumber),
        company_fax_number: business(BusinessOption::FaxNumber),
        company_email_address: business(BusinessOption::EmailAddress),
        company_website_url: business(BusinessOption::WebsiteUrl),
        company_id: business(BusinessOption::CompanyId),
        tax_number: business(BusinessOption::TaxNumber),
        default_invoice_report: opts::default_invoice_report(book).map(|(guid, name)| InvoiceReport {
            guid: guid.to_owned(),
            name: name.to_owned(),
        }),
    })
}

fn get(ctx: &mut Context, _: NoParams) -> Result<BookOptions, RpcError> {
    ctx.query(|book, env| describe(book, env))
}

fn set(ctx: &mut Context, p: SetParams) -> Result<BookOptions, RpcError> {
    ctx.mutate_rpc(|book, env| {
        if let Some(on) = p.split_action_for_num {
            opts::set_split_action_for_num(book, on);
        }
        if let Some(days) = p.auto_read_only_days {
            opts::set_auto_readonly_days(book, days)?;
        }
        if let Some(on) = p.use_trading_accounts {
            opts::set_trading_accounts(book, on);
        }
        if let Some(budget) = p.default_budget {
            budgets::set_default_budget(book, budget.0)?;
        }
        p.business()
            .for_each(|(option, value)| opts::set_business_option(book, option, value));
        if let Some(report) = &p.default_invoice_report {
            opts::set_default_invoice_report(book, report.as_ref().map(|r| (r.guid.0, r.name.as_str())));
        }
        describe(book, env)
    })
}
