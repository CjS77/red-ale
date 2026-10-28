# GnuCash features

Every GnuCash feature that red-ale aims to match, against the parity target in [`GNUCASH_REV`](GNUCASH_REV) (5.17).

| Column     | Meaning                                                                                                                |
|------------|------------------------------------------------------------------------------------------------------------------------|
| Status     | ✅ ported; blank: not yet; ❌ won't do                                                                                 |
| Introduced | First stable GnuCash release with the feature. "≤ 1.4" means it was already in 1.4.0, the oldest tagged stable release |
| Ported     | The red-ale version that first ships the feature                                                                       |

**Won't do (❌)** features will likely never be ported, usually because the feature is GUI-specific, or is too niche.

A feature added in a development series is listed under the stable release that first shipped it.

## 1. Books and accounts

### Account structure

| Status | Name                             | Short description                                                                                | Introduced | Ported |
|:------:|----------------------------------|--------------------------------------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Account hierarchy                | Accounts nest under a single root to any depth                                                   |   ≤ 1.4    |  0.1   |
|   ✅   | Standard account types           | Bank, Cash, Asset, Credit Card, Liability, Stock, Mutual Fund, Currency, Income, Expense, Equity |   ≤ 1.4    |  0.1   |
|   ✅   | A/Receivable and A/Payable types | Account types that carry business documents and payments                                         |    1.8     |  0.1   |
|   ✅   | Trading account type             | Account type that balances multi-commodity transactions                                          |    2.4     |  0.2   |
|   ✅   | Placeholder accounts             | Accounts that group others and accept no transactions                                            |   1.6.7    |  0.1   |
|   ✅   | Hidden accounts                  | Accounts kept but hidden from everyday views                                                     |    2.0     |  0.1   |
|   ✅   | Account codes                    | A user code per account, used for sorting and numbering                                          |   ≤ 1.4    |  0.1   |
|   ✅   | Account description and notes    | Free-text description and notes per account                                                      |   ≤ 1.4    |  0.1   |
|   ✅   | Account colours                  | A display colour per account                                                                     |    2.4     |  0.2   |
|   ✅   | Tax-related flag                 | Marks accounts that feed tax reports                                                             |    1.6     |  0.1   |
|   ✅   | US tax (TXF) codes               | Per-account TXF form and line codes for US tax export                                            |    1.6     |  0.1   |
|   ✅   | Balance limits                   | Upper and lower balance limits that warn when crossed                                            |    5.0     |  0.2   |
|   ✅   | Opening balance account          | A dedicated equity account for opening balances (`equity-type` slot)                             |    4.3     |  0.2   |
|   ✅   | Associated accounts              | Remembers the proceeds, fees, dividend and gains accounts for a security                         |    5.6     |  0.2   |
|   ✅   | Renumber sub-accounts            | Assigns sequential codes to an account's children                                                |    2.0     |  0.1   |
|   ✅   | Cascade account properties       | Copies colour, placeholder or hidden settings to sub-accounts                                    |    4.0     |  0.2   |
|   ✅   | Delete with move                 | Deletes an account after moving its sub-accounts and splits elsewhere                            |    1.8     |  0.1   |

### Book settings

| Status | Name                                                      | Short description                                                            | Introduced | Ported |
|:------:|-----------------------------------------------------------|------------------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Accounting period                                         | Fiscal year start and end, used by reports and date ranges                   |    2.0     |  0.1   |
|   ✅   | Accounting period turned into local 00:00:00 and 23:59:59 | Period bounds at local day start and end; v0.1 used UTC                      |    2.0     |  0.2   |
|   ✅   | Number counters and formats                               | Next numbers and formats for invoices, bills, customers and others           |   2.4.1    |  0.2   |
|   ✅   | Auto read-only threshold                                  | Transactions older than N days become read-only                              |    2.6     |  0.2   |
|   ✅   | Num field source                                          | Use the split action field as the transaction number                         |    2.6     |  0.2   |
|   ✅   | Use trading accounts                                      | Book option to balance multi-commodity transactions through trading accounts |    2.4     |  0.2   |
|   ✅   | Close book                                                | Zeroes income and expense into equity at a closing date                      |   2.2.4    |  0.2   |

## 2. Transactions

### Entry

| Status | Name                                                       | Short description                                                             | Introduced | Ported |
|:------:|------------------------------------------------------------|-------------------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Double-entry transactions                                  | Transactions made of splits whose values sum to zero                          |   ≤ 1.4    |  0.1   |
|   ✅   | Multi-currency transactions                                | A transaction currency, with each split's quantity in its account's commodity |    1.6     |  0.1   |
|   ✅   | Exchange rates on transfer                                 | Enter a rate or amount when a transfer crosses currencies                     |    1.8     |  0.1   |
|   ✅   | Transaction notes                                          | Free-text notes per transaction                                               |    1.6     |  0.1   |
|   ✅   | Date entered                                               | Records when a transaction was entered, separate from the posted date         |   ≤ 1.4    |  0.1   |
|   ✅   | Split action and memo                                      | Per-split action and memo fields                                              |   ≤ 1.4    |  0.1   |
|   ✅   | Document links                                             | Links a transaction to a file or URL                                          |    2.6     |  0.2   |
|   ✅   | Void and unvoid                                            | Voids a transaction, keeping it with a reason and timestamp                   |    1.8     |  0.1   |
|   ✅   | Reverse transaction                                        | Creates the opposite of an existing transaction                               |    2.0     |  0.1   |
|   ✅   | Find transactions                                          | Search splits by any combination of fields                                    |   ≤ 1.4    |  0.1   |
|   ✅   | "Today" and the find dialog's day boundaries in local time | "Today" and date searches use local days; v0.1 used UTC                       |   ≤ 1.4    |  0.2   |

### Reconciliation

| Status | Name                          | Short description                                          | Introduced | Ported |
|:------:|-------------------------------|------------------------------------------------------------|:----------:|:------:|
|   ✅   | Reconcile states              | New, cleared, reconciled, frozen and void states per split |   ≤ 1.4    |  0.1   |
|   ✅   | Reconcile against a statement | Matches splits to a statement's ending balance and date    |   ≤ 1.4    |  0.1   |
|   ✅   | Last reconcile date           | Remembers each account's last reconciliation               |    1.6     |  0.1   |
|   ✅   | Auto-clear                    | Finds the splits that make up a given cleared balance      |    2.4     |  0.2   |

### Data integrity

| Status | Name            | Short description                                                | Introduced | Ported |
|:------:|-----------------|------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Imbalance scrub | Balances a transaction through an `Imbalance-<currency>` account |    1.6     |  0.1   |
|   ✅   | Orphan scrub    | Moves splits with no account to an `Orphan-<currency>` account   |    1.6     |  0.1   |
|   ✅   | Check & Repair  | Runs the scrubbers on an account, a subtree or the whole book    |   1.6.3    |  0.1   |

## 3. Commodities, prices and investments

### Commodities

| Status | Name                   | Short description                                          | Introduced | Ported |
|:------:|------------------------|------------------------------------------------------------|:----------:|:------:|
|   ✅   | Commodity table        | Currencies and securities grouped by namespace             |    1.6     |  0.1   |
|   ✅   | ISO 4217 currencies    | Built-in table of ISO currencies and their fractions       |    1.6     |  0.1   |
|   ✅   | Security editor        | Create, edit and delete securities and namespaces          |    1.6     |  0.1   |
|   ✅   | User currency symbols  | Overrides a currency's display symbol                      |    2.6     |  0.2   |
|   ✅   | Euro legacy currencies | Fixed conversion rates for currencies replaced by the euro |   ≤ 1.4    |  0.1   |

### Prices

| Status | Name                                  | Short description                                      | Introduced | Ported |
|:------:|---------------------------------------|--------------------------------------------------------|:----------:|:------:|
|   ✅   | Price database                        | Dated prices of one commodity in another               |    1.6     |  0.1   |
|   ✅   | Same-day prices decided in local time | Which prices share a day is a local day; v0.1 used UTC |    1.6     |  0.2   |
|   ✅   | Price editor                          | Add, edit and delete prices                            |    1.6     |  0.1   |
|   ✅   | Remove old prices                     | Thins out prices before a date, by namespace           |    2.0     |  0.1   |
|   ❌   | Online quotes                         | Fetches prices through Finance::Quote                  |   ≤ 1.4    |        |
|   ✅   | Online quotes (red-ale)               | Fetches prices and currency rates from Quote services, stored as GnuCash stores fetched quotes ([`api/quotes.md`](api/quotes.md)) |   ≤ 1.4    |  0.3   |
|   ✅   | Quote source and time zone            | Per-security quote source and time zone                |    1.6     |  0.1   |

*Note:* red-ale fetches quotes itself, in Rust, rather than through Finance::Quote; the Book keeps Finance::Quote's
source names and the `Finance::Quote` price source, so GnuCash reads the prices as fetched quotes. Tests:
`crates/red-ale-testkit/tests/quotes.rs`, `oracle_quotes.rs`.

### Investments

| Status | Name                        | Short description                                                    | Introduced | Ported |
|:------:|-----------------------------|----------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Lots                        | Groups the splits that buy and sell the same holding                 |    1.8     |  0.1   |
|   ✅   | Lot viewer                  | Inspect and edit lots for an account                                 |    2.0     |  0.1   |
|   ✅   | Capital gains               | Computes realised gains per lot and records them as splits           |    2.0     |  0.1   |
|   ✅   | Stock split assistant       | Records a stock split and its cash in lieu                           |    1.6     |  0.1   |
|   ❌   | Stock transaction assistant | Guided buy, sell, dividend, return of capital and split transactions |    5.0     |        |

## 4. Scheduled transactions

### Scheduled transactions

| Status | Name                        | Short description                                                         | Introduced | Ported |
|:------:|-----------------------------|---------------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Scheduled transactions      | Template transactions that recur on a schedule                            |    1.8     |  0.4   |
|   ✅   | Recurrence rules            | Daily, weekly, semi-monthly, monthly, yearly and end-of-month schedules   |    2.0     |  0.4   |
|   ✅   | Formulas and variables      | Template amounts as formulas, with variables filled in at creation        |    1.8     |  0.4   |
|   ✅   | Since Last Run              | Creates or reminds about the instances due since the book was last opened |    1.8     |  0.4   |
|   ✅   | Schedule from transaction   | Makes a scheduled transaction from an existing one                        |    1.8     |  0.4   |
|   ✅   | Mortgage and loan assistant | Builds the scheduled repayments for a loan                                |    1.8     |  0.4   |

## 5. Budgets

### Budgets

| Status | Name                  | Short description                                                  | Introduced | Ported |
|:------:|-----------------------|--------------------------------------------------------------------|:----------:|:------:|
|        | Budgets               | Several budgets per book, each with amounts per account and period |    2.0     |        |
|        | Estimate from history | Fills budget amounts from past transactions                        |    2.0     |        |
|        | Budget notes          | A note per account and period                                      |    4.0     |        |
|        | Natural signs         | Stores budget amounts with their natural signs                     |    3.8     |        |

## 6. Business

### Contacts

| Status | Name      | Short description                                               | Introduced | Ported |
|:------:|-----------|-----------------------------------------------------------------|:----------:|:------:|
|   ✅   | Customers | Customers with addresses, terms, tax settings and credit limits |    1.8     |  0.1   |
|   ✅   | Vendors   | Vendors with addresses, terms and tax settings                  |    1.8     |  0.1   |
|   ✅   | Employees | Employees with rates, for expense vouchers                      |    1.8     |  0.1   |
|   ✅   | Jobs      | Groups invoices or bills for one customer or vendor             |    1.8     |  0.1   |

### Documents

| Status | Name               | Short description                                                | Introduced | Ported |
|:------:|--------------------|------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Invoices and bills | Documents with entries, quantities, prices, discounts and taxes  |    1.8     |  0.1   |
|   ✅   | Expense vouchers   | Employee expense claims                                          |   1.8.2    |  0.1   |
|   ✅   | Credit notes       | Negative invoices and bills                                      |    2.6     |  0.2   |
|   ✅   | Orders             | Groups invoice entries into an order                             |    1.8     |  0.1   |
|   ✅   | Tax tables         | Named tax rates, as percentages or fixed amounts                 |    1.8     |  0.1   |
|   ✅   | Billing terms      | Due dates and early-payment discounts                            |    1.8     |  0.1   |
|   ✅   | Post to accounts   | Posts a document to A/R or A/P, creating its transaction and lot |    1.8     |  0.1   |
|   ✅   | Unpost             | Reverses a posting so the document can be edited                 |    1.8     |  0.1   |
|   ✅   | Duplicate document | Copies an invoice or bill                                        |   2.4.1    |  0.2   |
|   ✅   | Document links     | Links an invoice or bill to a file or URL                        |    4.2     |  0.2   |

### Payments

| Status | Name                              | Short description                                                | Introduced | Ported |
|:------:|-----------------------------------|------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Process payment                   | Records a payment against an owner's open documents              |    1.8     |  0.1   |
|   ✅   | Payment matching and pre-payments | Links payments to documents through lots, including pre-payments |    2.6     |  0.2   |
|   ✅   | Payment from transaction          | Turns an existing transaction into a business payment            |    2.6     |  0.2   |
|   ✅   | Due reminders                     | Lists bills and invoices that are due soon                       |    2.0     |  0.1   |

### Business import

| Status | Name                         | Short description                             | Introduced | Ported |
|:------:|------------------------------|-----------------------------------------------|:----------:|:------:|
|   ✅   | Import invoices and bills    | Creates invoices and bills from a CSV file    |   2.4.1    |  0.3   |
|   ✅   | Import customers and vendors | Creates customers and vendors from a CSV file |    2.6     |  0.3   |

## 7. Import and export

### File import

| Status | Name                   | Short description                                        | Introduced | Ported |
|:------:|------------------------|----------------------------------------------------------|:----------:|:------:|
|   ✅   | QIF import             | Imports Quicken Interchange Format files                 |   ≤ 1.4    |  0.3   |
|   ❌   | OFX/QFX import         | Imports OFX and QFX statements through LibOFX            |    1.8     |        |
|   ✅   | CSV transaction import | Imports transactions from CSV, with saved column layouts |    2.6     |  0.3   |
|   ✅   | CSV account import     | Imports an account tree from CSV                         |    2.6     |  0.3   |
|   ✅   | CSV price import       | Imports prices from CSV                                  |    3.0     |  0.3   |
|   ❌   | AqBanking file import  | MT940, MT942 and DTAUS statements through AqBanking      |   1.8.9    |        |
|   ❌   | CAMT import            | ISO 20022 CAMT statements through AqBanking              |    5.0     |        |

*Note:* OFX/QFX, MT940, MT942, DTAUS and CAMT are parsed by LibOFX and AqBanking, C libraries outside GnuCash. red-ale
links no C library and has no GnuCash parsing code to translate for them, so they are not ported ([`milestone3.md`](milestone3.md)).

### Matching

| Status | Name                      | Short description                                                     | Introduced | Ported |
|:------:|---------------------------|-----------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Import matcher            | Detects duplicates and matches imported transactions to existing ones |    1.8     |  0.3   |
|   ✅   | Bayesian account matching | Learns which account an imported transaction belongs to               |   1.8.3    |  0.3   |
|   ✅   | Import map editor         | View and delete learned import mappings                               |    3.0     |  0.3   |

### Online banking

| Status | Name                        | Short description                                 | Introduced | Ported |
|:------:|-----------------------------|---------------------------------------------------|:----------:|:------:|
|        | Online banking (FinTS/HBCI) | Fetches balances and statements through AqBanking |    1.8     |        |
|        | SEPA transfers              | Sends SEPA credit transfers through AqBanking     |    2.6     |        |
|        | SEPA direct debits          | Sends SEPA direct debits through AqBanking        |    2.6     |        |

### Export

| Status | Name            | Short description                                          | Introduced | Ported |
|:------:|-----------------|------------------------------------------------------------|:----------:|:------:|
|   ✅   | CSV export      | Exports transactions or the account tree to CSV            |    2.6     |  0.3   |
|   ✅   | Export accounts | Writes the account tree to a new book with no transactions |    1.8     |  0.3   |

## 8. Reports

GnuCash's reports are written in Guile Scheme. red-ale will never run Guile. Instead, each report is translated
from its GnuCash Scheme file into Rhai, red-ale's report scripting, which arrives in v0.2. A translation keeps the
report's options, logic and figures (see [`milestone2.md`](milestone2.md)).

### Report system

| Status | Name                        | Short description                              | Introduced | Ported |
|:------:|-----------------------------|------------------------------------------------|:----------:|:------:|
|   ✅   | Report framework            | Reports with options, run over the book's data |   ≤ 1.4    |  0.2   |
|   ✅   | Style sheets                | Named styles that control report layout        |    1.6     |  0.2   |
|   ✅   | Saved report configurations | Stores a report with its options for reuse     |   1.8.10   |  0.2   |
|        | Multicolumn reports         | Combines several reports on one page           |    1.6     |        |
|   ✅   | HTML export                 | Saves a report as HTML                         |    1.6     |  0.2   |
|        | PDF export                  | Saves a report as PDF                          |    2.6     |        |
|        | Interactive charts          | Charts drawn with Chart.js                     |    4.0     |        |

### Financial statements

| Status | Name                              | Short description                                  | Introduced | Ported |
|:------:|-----------------------------------|----------------------------------------------------|:----------:|:------:|
|   ✅   | Account Summary                   | Balances of a chosen set of accounts               |    1.6     |  0.2   |
|   ✅   | Balance Sheet                     | Assets, liabilities and equity at a date           |    1.6     |  0.2   |
|   ✅   | Income Statement                  | Income and expense over a period (profit and loss) |    1.6     |  0.2   |
|        | Multicolumn Balance Sheet and P&L | Balance sheet and P&L across several periods       |    3.6     |        |
|        | Trial Balance                     | Debit and credit balances of every account         |    2.0     |        |
|        | Cash Flow                         | Money into and out of chosen accounts              |    1.8     |        |
|        | Equity Statement                  | Changes in equity over a period                    |    2.0     |        |
|        | General Journal                   | Every transaction in date order                    |    2.0     |        |
|        | General Ledger                    | Every split, grouped by account                    |    2.0     |        |
|        | Transaction Report                | Filtered, sorted and subtotalled list of splits    |   1.6.1    |        |
|        | Reconciliation Report             | Reconciled and outstanding splits per account      |    3.6     |        |
|        | Register report                   | Prints an account register                         |    1.6     |        |

### Analysis and charts

| Status | Name                          | Short description                                             | Introduced | Ported |
|:------:|-------------------------------|---------------------------------------------------------------|:----------:|:------:|
|   ✅   | Average Balance               | Average, highest and lowest balance per period                |   ≤ 1.4    |  0.2   |
|        | Balance Forecast              | Projected balances including scheduled transactions           |    3.5     |        |
|   ✅   | Net worth and income charts   | Net worth and income/expense over time, as bar or line charts |    1.6     |  0.2   |
|   ✅   | Account bar charts            | Account balances or flows over time                           |    1.6     |  0.2   |
|   ✅   | Account pie charts            | Share of balances or flows by account                         |    1.6     |  0.2   |
|   ✅   | Price chart                   | A commodity's price over time                                 |    1.6     |  0.2   |
|        | Income/expense vs day of week | Totals by day of the week                                     |   1.8.3    |        |
|        | Cash flow chart               | Cash in and out over time                                     |    3.0     |        |
|        | Transaction Breakdown         | Splits of each transaction spread across account columns      |    5.7     |        |
|        | Future scheduled transactions | Upcoming scheduled transaction instances                      |   2.4.1    |        |

### Investment reports

| Status | Name                 | Short description                              | Introduced | Ported |
|:------:|----------------------|------------------------------------------------|:----------:|:------:|
|        | Investment Portfolio | Holdings and values at a date                  |    1.6     |        |
|        | Advanced Portfolio   | Holdings with basis, gains, income and returns |    1.8     |        |
|        | Investment Lots      | Lots with basis, gains and annual growth rate  |    5.0     |        |
|        | IFRS cost basis      | Weighted-average cost basis under IFRS         |    4.6     |        |

### Tax reports

| Status | Name                     | Short description                                      | Introduced | Ported |
|:------:|--------------------------|--------------------------------------------------------|:----------:|:------:|
|        | US income tax (TXF)      | US tax report and TXF export for tax software          |   1.4.9    |        |
|        | German income tax        | Tax report for German returns                          |   1.8.11   |        |
|        | Income and GST Statement | Sales and purchases with GST or VAT collected and paid |    3.0     |        |

### Budget reports

| Status | Name                    | Short description                             | Introduced | Ported |
|:------:|-------------------------|-----------------------------------------------|:----------:|:------:|
|   ✅   | Budget Report           | Budget against actual, per account and period |    2.0     |  0.4   |
|   ✅   | Budget Balance Sheet    | Balance sheet projected from a budget         |    2.4     |  0.4   |
|        | Budget Income Statement | Income statement from a budget                |    2.4     |        |
|        | Budget Flow             | Budgeted against actual flow for one period   |    2.4     |        |
|        | Budget Chart            | Budget against actual as a chart              |    2.4     |        |

### Business reports

| Status | Name                                  | Short description                                      | Introduced | Ported |
|:------:|---------------------------------------|--------------------------------------------------------|:----------:|:------:|
|        | Customer, vendor and employee reports | Documents and payments for one owner                   |    1.8     |        |
|        | Owner report (new)                    | Rewritten owner report with running balances and links |    3.8     |        |
|        | Receivable and payable aging          | Amounts owed by age bracket                            |    1.8     |        |
|        | Customer Summary                      | Sales, costs and profit per customer                   |   2.4.1    |        |
|        | Job Report                            | Documents and payments for one job                     |    2.4     |        |
|        | Printable invoice                     | Standard invoice layout                                |    1.8     |        |
|        | Fancy invoice                         | Invoice layout with company details and styling        |   1.8.5    |        |
|        | Easy invoice                          | Simplified invoice layout                              |    2.0     |        |
|        | Tax invoice                           | Configurable invoice layout showing tax                |    2.4     |        |
|        | Receipt                               | Receipt layout for point-of-sale printers              |    3.0     |        |

### Samples

| Status | Name               | Short description                | Introduced | Ported |
|:------:|--------------------|----------------------------------|:----------:|:------:|
|   ❌   | Hello World report | Sample report for report authors |   ≤ 1.4    |        |

## 9. Files and storage

### Formats

| Status | Name                | Short description                               | Introduced | Ported |
|:------:|---------------------|-------------------------------------------------|:----------:|:------:|
|   ✅   | XML file format     | The v2 XML book format                          |    1.6     |  0.1   |
|   ✅   | Compressed XML      | Gzip-compressed XML books                       |    1.8     |  0.1   |
|        | SQLite books        | Books stored in SQLite through libdbi           |    2.4     |        |
|        | MySQL/MariaDB books | Books stored in MySQL or MariaDB through libdbi |    2.4     |        |
|        | PostgreSQL books    | Books stored in PostgreSQL through libdbi       |    2.4     |        |
|        | ISO dates in SQLite | ISO 8601 timestamps in SQLite books             |   2.6.20   |        |

### Safety

| Status | Name                  | Short description                                                   | Introduced | Ported |
|:------:|-----------------------|---------------------------------------------------------------------|:----------:|:------:|
|   ✅   | Book locking          | `.LCK` file or `gnclock` table stops two programs writing one book  |   ≤ 1.4    |  0.1   |
|   ✅   | Book features         | Lists the features a book uses, so older versions refuse to open it |    2.6     |  0.1   |
|   ✅   | Transaction journal   | Writes every change to a `.log` file beside the book                |   ≤ 1.4    |  0.1   |
|   ✅   | Replay journal        | Re-applies a `.log` file to recover lost changes                    |   1.8.5    |  0.1   |
|   ✅   | Backups and retention | Timestamped backups on save, deleted after a set time               |    1.8     |  0.1   |
|   ❌   | Autosave              | Saves the book on a timer                                           |    2.2     |        |
|   ✅   | Open read-only        | Opens a book without taking the lock                                |    2.6     |  0.1   |

**Notes:**

* Autosave won't be ported. Every operation is atomic, so a timer adds nothing a client can't do with `book.save`;
  changes made since the last save are in the journal, and `book.replayJournal` restores them.

### Other

| Status | Name              | Short description                                       | Introduced | Ported |
|:------:|-------------------|---------------------------------------------------------|:----------:|:------:|
|   ❌   | Book currency     | Never implemented upstream; GnuCash only keeps the flag |    3.0     |        |
|   ❌   | Window state file | `.gcm` file of open tabs, sort and filter settings      |    2.6     |        |

## 10. Automation and tools

### Command line

| Status | Name                          | Short description                               | Introduced | Ported |
|:------:|-------------------------------|-------------------------------------------------|:----------:|:------:|
|   ❌   | gnucash-cli                   | Command-line tool that runs without the GUI     |    4.0     |        |
|   ❌   | Quotes from the command line  | Fetches and stores price quotes without the GUI |    1.6     |        |
|   ❌   | Reports from the command line | Runs a saved report and writes the output       |    4.0     |        |

**Notes:**

* `red-ale` has its own CLI interface via the red-ale binary, which lets you do anything the JSON-RPC server does.
* Currency and price quotes can be automatically updated via JSON-RPC as of v0.3, fetched by red-ale's own Rust code
  rather than Finance::Quote, so no Perl is needed (see [`open-questions.md`](open-questions.md))
* `red-ale` will support its own reporting format.

### Scripting

| Status | Name             | Short description                                                                                    | Introduced | Ported |
|:------:|------------------|------------------------------------------------------------------------------------------------------|:----------:|:------:|
|   ❌   | Guile scripting  | Scheme access to the engine and reports. Never supported; reports use red-ale's own scripting (v0.2) |   ≤ 1.4    |        |
|   ❌   | Python bindings  | Python access to the engine; red-ale offers JSON-RPC                                                 |    2.4     |        |
|   ❌   | REST API example | Sample Flask server built on the Python bindings                                                     |    2.6     |        |

### Setup and tools

| Status | Name                            | Short description                                             | Introduced | Ported |
|:------:|---------------------------------|---------------------------------------------------------------|:----------:|:------:|
|   ❌   | New account hierarchy assistant | Creates a starting account tree from templates                |    1.8     |        |
|   ❌   | Account templates               | Localised starting account trees                              |    1.6     |        |
|   ❌   | Financial calculator            | Loan and savings calculations: payment, rate, periods, values |    1.6     |        |
|   ❌   | Check printing                  | Prints cheques from transactions                              |   ≤ 1.4    |        |

**Notes:**

* The new account hierarchy assistant is a GUI walk-through; `account.create` builds any tree.
* Account templates are better done as agent skills, which build a tree through `account.create`.
* The financial calculator won't be ported. The financial functions that scheduled-transaction formulas call are
  still translated in v0.4 ([`milestone4.md`](milestone4.md)).

## 11. User interface

### Desktop interface

| Status | Name              | Short description                                                       | Introduced | Ported |
|:------:|-------------------|-------------------------------------------------------------------------|:----------:|:------:|
|   ❌   | Account registers | GTK ledger views (basic, auto-split, journal) for entering transactions |   ≤ 1.4    |        |
|   ❌   | Autocompletion    | Completes descriptions and accounts while typing                        |   ≤ 1.4    |        |
|   ❌   | Tip of the Day    | Start-up tips dialog                                                    |    1.6     |        |
