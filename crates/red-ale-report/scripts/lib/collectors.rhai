// lib/collectors: GnuCash's value and commodity collectors and the helpers built on them.
//
// Translated from 5.17:gnucash/report/report-utilities.scm (lines 289-455 and 1018-1022) and
// 5.17:gnucash/report/commodity-utilities.scm (lines 900-919). The headers of those files:
//
// report-utilities.scm -- Reporting utilities
//
// commodity-utilities.scm: Functions for handling different commodities.
// Copyright 2001 Christian Stimming <stimming@tu-harburg.de>
//
// Both carry this notice:
//
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
// Cited lines: RU is report-utilities.scm, CU commodity-utilities.scm.
//
// A commodity collector is an array of [commodity, total] pairs, GnuCash's association list (RU:340-354): a new
// commodity goes on the front and lookups take the first match, so merging walks the other list from its front and
// reverses its order (RU:356-361). The order of a multi-commodity cell depends on it. GnuCash's collector is a closure
// taking an action ('add, 'merge, ...); here each action is a method on the array, which changes the collector held
// in the variable it is called on:
//
//     import "lib/collectors" as collectors;
//     let c = collectors::make_commodity_collector();
//     c.add(usd, rational(5));            // (c 'add usd 5)
//     c.merge(other);                     // (c 'merge other #f)
//     let mons = c.format(|comm, amt| monetary(comm, amt));   // (c 'format gnc:make-gnc-monetary #f)
//
// Arrays are values in Rhai: `let d = c;` copies, where GnuCash's collectors are shared. The methods are visible on
// every array in a script that imports this module. A value collector is its total, a Rational: `v += amount`.

import "lib/exchange" as exchange;

/// `gnc:make-value-collector` (RU:289-296): a value collector is its total, starting at zero. Add to it with `+=`.
fn make_value_collector() {
    rational(0)
}

/// `gnc:make-commodity-collector` (RU:340-416): an empty collector.
fn make_commodity_collector() {
    []
}

/// True for the numbers a value collector adds; it ignores anything else (RU:293).
private fn is_number(amount) {
    let t = type_of(amount);
    t == "Rational" || t == "i64"
}

/// The position of `commodity`'s pair in `list`, -1 when it has none: `assoc`, first match.
private fn assoc_index(list, commodity) {
    list.index_of(|pair| pair[0] == commodity)
}

/// 'add (RU:346-354, :404): add `amount` to `commodity`'s total, putting a new commodity on the front.
fn array.add(commodity, amount) {
    let i = assoc_index(this, commodity);
    if i < 0 {
        this.insert(0, [commodity, rational(0)]);
        i = 0;
    }
    if is_number(amount) {
        this[i][1] = amount + this[i][1];
    }
}

/// 'merge (RU:356-361, :405): add each of `other`'s totals, walking `other` from its front.
fn array.merge(other) {
    for pair in other {
        this.add(pair[0], pair[1]);
    }
}

/// 'minusmerge (RU:363-368, :407): subtract each of `other`'s totals, walking `other` from its front.
fn array.minusmerge(other) {
    for pair in other {
        this.add(pair[0], -pair[1]);
    }
}

/// 'format (RU:372-376, :409): `f(commodity, total)` for each pair, in the collector's order.
fn array.format(f) {
    let out = [];
    for pair in this {
        out.push(f.call(pair[0], pair[1]));
    }
    out
}

/// 'reset (RU:410): forget every commodity.
fn array.reset() {
    this.clear();
}

/// The total of `commodity`, zero when the collector has none.
private fn total_of(list, commodity) {
    let i = assoc_index(list, commodity);
    if i < 0 { rational(0) } else { list[i][1] }
}

/// 'getpair (RU:380-384): `[commodity, total]`, the total 0 when absent and negated when `sign` is true.
fn array.getpair(commodity, sign) {
    let total = total_of(this, commodity);
    [commodity, if sign { -total } else { total }]
}

/// 'getmonetary (RU:387-391): the total as a Monetary, 0 when absent and negated when `sign` is true.
fn array.getmonetary(commodity, sign) {
    let total = total_of(this, commodity);
    monetary(commodity, if sign { -total } else { total })
}

/// 'getmonetary-strict (RU:394-398): as getmonetary, but `()` when the collector has no such commodity.
fn array.getmonetary_strict(commodity, sign) {
    let i = assoc_index(this, commodity);
    if i < 0 {
        return ();
    }
    let total = this[i][1];
    monetary(commodity, if sign { -total } else { total })
}

/// 'remove-zeros (RU:398, :414): drop the commodities whose total is zero.
fn array.remove_zeros() {
    this.retain(|pair| pair[1] != 0);
}

/// `gnc:commodity-collector-get-negated` (RU:418-421): a new collector, minus-merged from `collector`, so in
/// reverse order.
fn commodity_collector_get_negated(collector) {
    let negated = make_commodity_collector();
    negated.minusmerge(collector);
    negated
}

/// `gnc-commodity-collector-allzero?` (RU:424-425): true when every total is zero, and for an empty collector.
fn commodity_collector_allzero(collector) {
    collector.all(|pair| pair[1] == 0)
}

/// `gnc:collector+` (RU:429-432), its arguments as an array: a new collector merging each in turn.
fn collector_plus(collectors) {
    let res = make_commodity_collector();
    for coll in collectors {
        res.merge(coll);
    }
    res
}

/// `gnc:collector-` (RU:436-445), its arguments as an array: the first minus the sum of the rest; one collector
/// alone is negated, as `(gnc:collector- (gnc:make-commodity-collector) coll)`.
fn collector_minus(collectors) {
    if collectors.is_empty() {
        throw "gnc:collector- needs at least 1 collector argument";
    }
    if collectors.len() == 1 {
        return collector_minus([make_commodity_collector(), collectors[0]]);
    }
    let res = make_commodity_collector();
    res.merge(collectors[0]);
    res.minusmerge(collector_plus(collectors.extract(1)));
    res
}

/// `gnc:monetaries-add` (RU:449-455), its arguments as an array: a collector of the Monetaries.
fn monetaries_add(monetaries) {
    let coll = make_commodity_collector();
    for mon in monetaries {
        coll.add(mon.commodity, mon.amount);
    }
    coll
}

/// `gnc:not-all-zeros` (RU:1018-1022): true when a number is not zero, or any element of an array is not all
/// zeros; false for anything else.
fn not_all_zeros(data) {
    if is_number(data) {
        data != 0
    } else if type_of(data) == "array" {
        data.some(|d| not_all_zeros(d))
    } else {
        false
    }
}

/// `gnc:sum-collector-commodity` (CU:900-910): `foreign`'s totals exchanged into `domestic` by `exchange_fn` (an
/// exchange function of lib/exchange, or a closure `|foreign, domestic|`), each rounded to `domestic`'s SCU with
/// banker's rounding (`gnc-numeric-convert` with `GNC-RND-ROUND`), and added. `()` when `foreign` or
/// `exchange_fn` is.
fn sum_collector_commodity(foreign, domestic, exchange_fn) {
    if foreign == () || exchange_fn == () {
        return ();
    }
    let fraction = domestic.fraction;
    let sum = rational(0);
    for pair in foreign {
        let exchanged = exchange::apply_exchange_fn(exchange_fn, monetary(pair[0], pair[1]), domestic);
        sum += exchanged.amount.round_to(fraction, "round");
    }
    monetary(domestic, sum)
}

/// `gnc:uniform-commodity?` (CU:913-919): true when the collector holds nothing, or only `report_commodity`.
fn uniform_commodity(amt, report_commodity) {
    if amt.is_empty() {
        true
    } else if amt.len() == 1 {
        amt[0][0] == report_commodity
    } else {
        false
    }
}
