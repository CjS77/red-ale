// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation; either version 2 of
// the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, contact:
//
// Free Software Foundation           Voice:  +1-617-542-5942
// 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
// Boston, MA  02110-1301,  USA       gnu@gnu.org
//
// This translation is part of red-ale and is distributed under the GNU General Public License, version 3 or (at your
// option) any later version (GPL-3.0-or-later); see LICENSE.
//
// Translated from 5.17:gnucash/report/reports/standard/ifrs-cost-basis.scm, cited below by line.
//
// The IFRS weighted-average cost basis report: one row per transaction of the Stock Account, classified by the signs
// of its splits in the chosen accounts, with the cumulative average cost basis, the basis of each sale, its gains and
// the running profits. Every figure is exact; the only rounding is the exchange rate, which the price database
// rounds to the report currency's SCU. GnuCash's #f ("none") is `()` here, and an empty cell.

import "lib/dates" as dates;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:52, :527-533).
fn templates() {
    [#{ guid: "15d5b744176c4625a703720338725291", name: "IFRS weighted-average cost basis report",
        menuPath: ["_Experimental"], params: #{} }]
}

/// `options-generator` (:79-120). Each account option defaults to the first account of its types in display order,
/// as GnuCash's account-sel options do when given none.
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", " ");
    opts_util::add_currency(o, "General", "Report's currency", "a");
    o.account_sel("General", "Stock Account", "b", "Stock Account", (), #{ types: ["STOCK", "MUTUAL"] });
    o.account_sel("General", "Proceeds Account", "c", "Proceeds Account", (), #{ types: ["ASSET", "BANK"] });
    o.account_sel("General", "Dividend Account", "c", "Dividend Account", (), #{ types: ["INCOME"] });
    o.account_sel("General", "Cap Gains Account", "d", "Cap Gains Account", (), #{ types: ["INCOME"] });
    o.account_sel("General", "Fees Account", "c5", "Fees Account", (), #{ types: ["EXPENSE"] });
    o.string("General", "Action field filter for fees", "d5",
             "This string will be used to compare with the split action field to detect capitalized fees on stock activity",
             "Fee");
    o.boolean("General", "Format monetary cells", "e", "Check this option to show cells with currency", true);
    o.boolean("General", "Alternative row-style for shorts", "f",
              "Check this option to use alternate style for shorts. Disable to use alternate style every other row", true);
    o.boolean("General", "Capitalise purchase commissions", "g",
              "Check this option to capitalise purchase commissions in cumulative average cost and gain/loss after commission",
              true);
}

/// The three paragraphs of `disclaimer` (:33-50), each on one line as the browser shows it.
private fn disclaimer() {
    [
        "This report is designed for cost basis accumulation and capital gain/loss reporting using the weighted average "
            + "cost basis method, which is most consistent with typical accounting frameworks (US GAAP, IFRS, etc.).  "
            + "This report allows for for the choice to capitalize (most consistent with typical accounting frameworks) "
            + "vs expense (used by some taxing jurisdictions) commissions paid on purchase.",
        "This report is not appropriate for FIFO, LIFO, or specific-identification methods for cost basis accumulation "
            + "and capital gain/loss reporting.  This report may not be appropriate for tax purposes, if the taxing "
            + "jurisdiction requires a method other than the weighted average cost basis method.",
        "This report is not designed with options reporting in mind.  If your activity involves options and/or futures "
            + "that are purchased, written, and/or exercised, there is no guarantee that this report will accurately "
            + "portray this options activity.",
    ].map(|p| #{ kind: "text", text: p })
}

// The arithmetic of :122-149, where `()` is none: sums skip it; products, negations and quotients with it, and
// division by zero, are none.

private fn is_none(x) {
    type_of(x) == "()"
}

/// `M+` (:122-126).
private fn m_add(a, b) {
    if is_none(a) { b } else if is_none(b) { a } else { a + b }
}

/// `M+` of several (:126).
private fn m_sum(xs) {
    xs.reduce(|acc, x| m_add(acc, x), ())
}

/// `M-` of one (:142).
private fn m_neg(a) {
    if is_none(a) { () } else { -a }
}

/// `M-` of two (:143): `a` plus the negation of `b`.
private fn m_sub(a, b) {
    m_add(a, m_neg(b))
}

/// `M*` (:133-137).
private fn m_mul(a, b) {
    if is_none(a) || is_none(b) { () } else { a * b }
}

/// `M/` of two (:145-149): `a` times the inverse of `b`, none when `b` is none or zero.
private fn m_div(a, b) {
    if is_none(b) || b.is_zero() { () } else { m_mul(a, rational(1) / b) }
}

/// `M-abs` (:128-131).
private fn m_abs(a) {
    if is_none(a) { () } else { a.abs() }
}

/// The sign tests of :164-169, by the names the type tables give them.
private fn sign_ok(test, x) {
    let n = is_none(x);
    switch test {
        "Dr>0" => !n && x > 0,
        "Cr>0" => !n && x < 0,
        "Dr>=0" => n || x >= 0,
        "Cr>=0" => n || x <= 0,
        "DrCr" => true,
        "...." => n || x == 0,
        _ => throw `not a sign test: ${test}`,
    }
}

// The transaction types (:174-211): seven sign tests, on the stock amount, stock value, proceeds, capitalised fees,
// expensed fees, dividends and capital gains, then purchase?, sale? and the narrative.

/// `open-types` (:174-179).
private fn open_types() {
    [
        ["Dr>0", "Dr>0", "Cr>0", "Dr>=0", "....", "....", "....", true, false, "Open Long"],
        ["Cr>0", "Cr>0", "Dr>0", "Dr>=0", "....", "....", "....", true, false, "Open Short"],
    ]
}

/// `long-types` (:181-196).
private fn long_types() {
    [
        ["Dr>0", "Dr>0", "Cr>0", "Dr>=0", "....", "....", "....", true, false, "Buy"],
        ["Cr>0", "Cr>0", "Dr>=0", "....", "Dr>=0", "....", "DrCr", false, true, "Sell"],
        ["....", "....", "Dr>0", "....", "Dr>=0", "Cr>0", "....", false, false, "Dividend"],
        ["Dr>0", "Dr>0", "Dr>0", "Dr>=0", "....", "Cr>0", "....", true, false, "Dividend reinvestment (w/ remainder)"],
        ["Dr>0", "Dr>0", "Cr>=0", "Dr>=0", "....", "Cr>0", "....", true, false, "Dividend reinvestment (w/o remainder)"],
        ["....", "Cr>0", "Dr>0", "Dr>=0", "....", "....", "....", true, false, "Return of capital"],
        ["....", "Cr>0", "....", "Dr>=0", "....", "Dr>0", "....", true, false, "Return of capital (reclassification)"],
        ["....", "Dr>0", "....", "....", "Dr>=0", "Cr>0", "....", true, false, "Notional distribution (dividend)"],
        ["....", "Dr>0", "....", "....", "Dr>=0", "....", "Cr>0", true, false, "Notional distribution (capital gain)"],
        ["Dr>0", "....", "Cr>=0", "Dr>=0", "....", "....", "....", false, false, "Stock split"],
        ["Cr>0", "....", "Cr>=0", "Dr>=0", "....", "....", "....", false, false, "Reverse split"],
        ["Cr>0", "Cr>0", "Dr>0", "....", "Dr>=0", "....", "DrCr", false, true, "Reverse split w/ cash in lieu for fractionals"],
    ]
}

/// `short-types` (:198-211).
private fn short_types() {
    [
        ["Cr>0", "Cr>0", "Dr>0", "Dr>=0", "....", "....", "....", true, false, "Short Sell"],
        ["Dr>0", "Dr>0", "Cr>=0", "....", "Dr>=0", "....", "DrCr", false, true, "Cover Buy"],
        ["....", "....", "Cr>0", "....", "Dr>=0", "Dr>0", "....", false, false, "Compensatory dividend"],
        ["....", "Dr>0", "Cr>0", "Dr>=0", "....", "....", "....", true, false, "Compensatory return of capital"],
        ["....", "Dr>0", "....", "Dr>=0", "....", "Cr>0", "....", true, false, "Compensatory return of capital (reclassification)"],
        ["....", "Cr>0", "....", "....", "Dr>=0", "Dr>0", "....", true, false, "Compensatory notional distribution (dividend)"],
        ["....", "Cr>0", "....", "....", "Dr>=0", "....", "Dr>0", true, false, "Compensatory notional distribution (capital gain)"],
        ["Cr>0", "....", "Cr>=0", "Dr>=0", "....", "....", "....", false, false, "Stock split"],
        ["Dr>0", "....", "Cr>=0", "Dr>=0", "....", "....", "....", false, false, "Reverse split"],
        ["Dr>0", "Dr>0", "Cr>0", "....", "Dr>=0", "....", "DrCr", false, true, "Reverse split w/ cash in lieu for fractionals"],
    ]
}

/// `txn-identify` (:219-234): `[purchase?, sale?, narrative]` of the first type whose seven tests all pass, from the
/// short, open or long types by the sign of the units held before the transaction (`cmp`, :213-216).
private fn txn_identify(info, cumul_units) {
    let types = if cumul_units < 0 { short_types() } else if cumul_units == 0 { open_types() } else { long_types() };
    let figures = [info.stock_amt, info.stock_val, info.proceeds_val, info.fees_cap_val, info.fees_exp_val,
                   info.dividend_val, info.capgains_val];
    for t in types {
        if figures.all(|x, i| sign_ok(t[i], x)) {
            return [t[7], t[8], t[9]];
        }
    }
    [false, false, "Unknown"]
}

/// Whether `split` is in `acct`, an option's account or `()` (`from-acct?`, :238-240).
private fn from_acct(acct, split) {
    !is_none(acct) && split.account.guid == acct.guid
}

/// `txn->info` (:236-290): each split of `trans` adds to the first figure it matches, in this order: the Proceeds,
/// Cap Gains, Fees and Dividend accounts' amounts; a Stock Account split whose action (or num, `gnc-get-action-num`)
/// is the fee filter, its value as a capitalised fee; any other Stock Account split, its amount and value.
private fn txn_info(trans, env) {
    let info = #{ stock_amt: (), stock_val: (), proceeds_val: (), fees_cap_val: (), fees_exp_val: (), dividend_val: (),
                  capgains_val: () };
    for split in trans.splits() {
        if from_acct(env.proceeds_acct, split) {
            info.proceeds_val = m_add(info.proceeds_val, split.amount);
        } else if from_acct(env.capgains_acct, split) {
            info.capgains_val = m_add(info.capgains_val, split.amount);
        } else if from_acct(env.fees_acct, split) {
            info.fees_exp_val = m_add(info.fees_exp_val, split.amount);
        } else if from_acct(env.dividend_acct, split) {
            info.dividend_val = m_add(info.dividend_val, split.amount);
        } else if from_acct(env.stock_acct, split) && split.actionNum == env.cap_fee_action {
            info.fees_cap_val = m_add(info.fees_cap_val, split.value);
        } else if from_acct(env.stock_acct, split) {
            info.stock_amt = m_add(info.stock_amt, split.amount);
            info.stock_val = m_add(info.stock_val, split.value);
        }
    }
    info
}

/// `get-fx` (:317-320): 10,000,000 units of `currency` converted at the price nearest local noon of `posted`
/// (`time64CanonicalDayTime`, :427-428), rounded to the report currency's SCU, over 10,000,000.
private fn get_fx(env, posted) {
    let large = 10000000;
    let when = dates::time64_canonical_day_time(env.ctx, posted);
    env.book.price_convert(monetary(env.currency, large), env.report_currency, when, "nearest").amount / large
}

/// Whether the position is short after the transaction (:417-418). With no units held after it and no stock amount
/// in it, GnuCash compares `(< 0 #f)`, a Guile error that stops the report; red-ale stops it too, naming the
/// transaction.
private fn shorting(new_units, trans_units, trans, ctx) {
    if new_units < 0 { return true; }
    if new_units != 0 { return false; }
    if is_none(trans_units) {
        throw `IFRS cost basis: the transaction "${trans.description}" of ${ctx.print_date(trans.datePosted)} ` +
              `(${trans.guid}) has no stock amount, as every Stock Account split in it matches the fee filter, ` +
              `and no units are held after it; GnuCash fails on it too`;
    }
    trans_units > 0
}

/// The figures of one transaction (:393-462), from the running `state`: the cells' figures and the next state.
private fn transaction_figures(env, state, split) {
    let trans = split.transaction;
    let info = txn_info(trans, env);
    let trans_units = info.stock_amt;
    let fees_value = m_add(info.fees_cap_val, info.fees_exp_val);
    let trans_value = m_add(info.stock_val, info.fees_cap_val);
    let cumul_units = state.units;
    let new_units = m_add(cumul_units, trans_units);
    let identified = if new_units < 0 && 0 < cumul_units {
        [false, false, "ERROR: long→short"]
    } else if cumul_units < 0 && 0 < new_units {
        [false, false, "ERROR: short→long"]
    } else {
        txn_identify(info, cumul_units)
    };
    let purchase = identified[0];
    let sale = identified[1];
    let f = #{ split: split, trans: trans, trans_units: trans_units, new_units: new_units, note: identified[2],
               shorting: shorting(new_units, trans_units, trans, env.ctx), capgains_val: info.capgains_val };
    f.purchase_cost = if purchase { fees_value } else { () };
    f.purchase_val = if purchase { m_sub(trans_value, f.purchase_cost) } else { () };
    f.cash_dividends = m_neg(info.dividend_val);
    f.proceeds_cost = if sale { fees_value } else { () };
    f.proceeds_value = if sale { m_add(info.proceeds_val, f.proceeds_cost) } else { () };

    // now convert to report-currency (:426-433)
    f.fx = get_fx(env, trans.datePosted);
    f.conv_purchase_val = m_mul(f.fx, f.purchase_val);
    f.conv_purchase_cost = m_mul(f.fx, f.purchase_cost);
    f.conv_dividends = m_mul(f.fx, f.cash_dividends);
    f.conv_proceeds_value = m_mul(f.fx, f.proceeds_value);
    f.conv_proceeds_cost = m_mul(f.fx, f.proceeds_cost);

    // now perform AVERAGE-COST-BASIS calculations (:435-462)
    f.per_unit = m_abs(m_div(state.basis, cumul_units));
    f.basis_of_sale = if is_none(f.proceeds_value) { () } else { m_mul(f.per_unit, trans_units) };
    let capitalised = if env.cap_purch_costs { f.conv_purchase_cost } else { () };
    f.basis = m_sum([state.basis, f.conv_purchase_val, capitalised, f.basis_of_sale]);
    f.net_proceeds = m_sub(f.conv_proceeds_value, f.conv_proceeds_cost);
    let expensed = if env.cap_purch_costs { () } else { f.conv_purchase_cost };
    f.gain_post = m_sum([f.net_proceeds, f.basis_of_sale, expensed]);
    f.gain_pre = m_add(f.conv_proceeds_value, f.basis_of_sale);
    f.variance = m_add(f.gain_pre, info.capgains_val);
    f.gross = m_add(state.gross, f.gain_pre);
    f.net = m_add(state.net, f.gain_post);
    f.tot = m_sum([state.tot, f.gain_post, f.conv_dividends]);
    f
}

/// A figure's cell (`to-cell` of `to-commodity`, `to-orig-currency` or `to-report-currency`, :322-358): a
/// monetary of `commodity`, or with "Format monetary cells" off the bare number; none is an empty cell.
private fn figure(env, commodity, amt) {
    if is_none(amt) {
        ()
    } else if env.format_cells {
        #{ monetary: monetary(commodity, amt) }
    } else {
        #{ number: amt }
    }
}

/// One row (:479-510): shaded as `alternate-row` while short with "Alternative row-style for shorts", else every
/// other written row. The trans-units cell links to the split; with no stock amount it is empty, where GnuCash links
/// the text of #f.
private fn table_row(env, f, odd_row) {
    let alternate = if env.short_alternate_format { f.shorting } else { !odd_row };
    let units = figure(env, env.commodity, f.trans_units);
    let rc = env.report_currency;
    let oc = env.currency;
    #{ alternate: alternate, cells: [
        #{ date: f.trans.datePosted },
        #{ text: f.trans.description },
        if is_none(units) { () } else { html_utils::html_split_anchor(f.split, units) },
        figure(env, env.commodity, f.new_units),
        #{ text: f.note },
        #{ text: oc.mnemonic },
        #{ price: f.fx, currency: rc },
        figure(env, oc, f.purchase_val),
        figure(env, oc, f.purchase_cost),
        figure(env, oc, f.cash_dividends),
        figure(env, oc, f.proceeds_value),
        figure(env, oc, f.proceeds_cost),
        figure(env, rc, f.conv_purchase_val),
        figure(env, rc, f.conv_purchase_cost),
        figure(env, rc, f.conv_dividends),
        figure(env, rc, f.conv_proceeds_value),
        figure(env, rc, f.conv_proceeds_cost),
        figure(env, rc, f.basis),
        figure(env, rc, f.per_unit),
        figure(env, rc, m_neg(f.basis_of_sale)),
        figure(env, rc, f.net_proceeds),
        figure(env, rc, f.gain_post),
        figure(env, rc, f.gain_pre),
        figure(env, rc, f.capgains_val),
        figure(env, rc, f.variance),
        figure(env, rc, f.gross),
        figure(env, rc, f.net),
        figure(env, rc, f.tot),
    ] }
}

/// The column headers as written (:369-379): the date, the texts, then figures.
private fn columns() {
    let labels = ["date", "description", "trans-units", "cumul-units", "note", "curr", "fx", "purchase-val",
                  "purchase-cost", "cash-dividends", "proceeds-val", "proceeds-cost", "conv-purchase-val",
                  "conv-purchase-cost", "conv-dividends", "conv-proceeds-val", "conv-proceeds-cost",
                  "cumulative-average-cost-basis", "average-cost-basis/unit-for-sale", "average-cost-basis-of-sale",
                  "net-proceeds", "gain-post-commission", "gain-pre-commission (calc)",
                  "gain-pre-commission (register)", "gain-pre-commission variance", "cumul-gross-profit",
                  "cumul-net-profit", "cumul-tot-return"];
    labels.map(|label, i| #{ label: label, role: switch i { 0 => "date", 1 | 4 | 5 => "text", _ => "amount" } })
}

/// The table (:337-343, :381-518): every transaction of the Stock Account, void ones too, in split order, by its
/// first split there (`xaccQueryGetSplitsUniqueTrans`). Each one moves the running figures; only those posted
/// strictly between the two dates' starts are written (:467-469).
private fn cost_basis_table(env) {
    let splits = env.book.split_query(#{ accounts: [env.stock_acct], reconcile: ["n", "c", "y", "f", "v"], unique: true });
    let state = #{ units: rational(0), basis: rational(0), gross: rational(0), net: rational(0), tot: rational(0) };
    let odd_row = true;
    let rows = [];
    for split in splits {
        let f = transaction_figures(env, state, split);
        state = #{ units: f.new_units, basis: f.basis, gross: f.gross, net: f.net, tot: f.tot };
        let posted = f.trans.datePosted;
        if env.startdate < posted && posted < env.enddate {
            rows.push(table_row(env, f, odd_row));
            odd_row = !odd_row;
        }
    }
    #{ kind: "table", columns: columns(), rows: rows }
}

/// `ifrs-cost-basis-renderer` (:292-523). Both dates are taken at the start of their day (:299-304). Without a Stock
/// Account the report is the options warning (:327-331); the disclaimer always follows.
fn render(params, book, opts, ctx) {
    let get = |name| opts.get("General", name);
    let stock_acct = get.call("Stock Account");
    if is_none(stock_acct) {
        let warning = html_utils::make_generic_options_warning("IFRS weighted-average cost basis report");
        return #{ title: "IFRS weighted average cost basis Report", sections: [#{ blocks: [warning] + disclaimer() }] };
    }
    let currency = stock_acct.currency_or_parent();
    if is_none(currency) {
        throw `IFRS cost basis: the Stock Account ${stock_acct.fullName} and its parents have no currency`;
    }
    let env = #{
        book: book, ctx: ctx, stock_acct: stock_acct, commodity: stock_acct.commodity, currency: currency,
        startdate: dates::time64_start_day_time(ctx, get.call("Start Date")),
        enddate: dates::time64_start_day_time(ctx, get.call("End Date")),
        proceeds_acct: get.call("Proceeds Account"), dividend_acct: get.call("Dividend Account"),
        capgains_acct: get.call("Cap Gains Account"), fees_acct: get.call("Fees Account"),
        report_currency: get.call("Report's currency"), format_cells: get.call("Format monetary cells"),
        short_alternate_format: get.call("Alternative row-style for shorts"),
        cap_purch_costs: get.call("Capitalise purchase commissions"),
        cap_fee_action: get.call("Action field filter for fees"),
    };
    let title = `Average-Cost (Basis) Report: From ${ctx.print_date(env.startdate)} to ${ctx.print_date(env.enddate)}. `
        + `Report-currency ${env.report_currency.mnemonic}`;
    #{ title: title, sections: [#{ blocks: [cost_basis_table(env)] + disclaimer() }] }
}
