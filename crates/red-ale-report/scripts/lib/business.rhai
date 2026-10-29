// lib/business: an owner's name, address and ID as a business document prints them, and the Book's company details.
//
// Translated from 5.17:bindings/guile/business-core.scm (lines 35-105), cited below as BC, and from
// `gnc:company-info` in 5.17:bindings/guile/options.scm (lines 409-411), cited as OPT. business-core.scm's header:
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
// options.scm carries the same notice under "Scheme code for supporting options".
//
// This translation is part of red-ale and is distributed under the GNU General Public License, version 3 or (at your
// option) any later version (GPL-3.0-or-later); see LICENSE.

/// The owner whose details a document shows: a job's customer or vendor, any other owner itself (`gncJobGetOwner`
/// in BC:47-49, :65, :103-104). A job with no owner gives `()`.
fn end_owner(owner) {
    if owner.kind == "job" { owner.owner } else { owner }
}

/// `gnc:owner-get-name-dep` (BC:63-67): a job's owner's name, else `gncOwnerGetName`.
fn owner_get_name_dep(owner) {
    let end = end_owner(owner);
    if type_of(end) == "()" { "" } else { end.name }
}

/// `gnc:owner-get-address-dep` (BC:69-82, `gnc:owner-get-address` at BC:35-50): the address's name and four lines,
/// those that are not empty, joined by newlines.
fn owner_get_address_dep(owner) {
    let end = end_owner(owner);
    let addr = if type_of(end) == "()" { () } else { end.address };
    if type_of(addr) == "()" {
        return "";
    }
    let text = "";
    for line in [addr.name, addr.addr1, addr.addr2, addr.addr3, addr.addr4].filter(|l| l != "") {
        text += if text == "" { line } else { "\n" + line };
    }
    text
}

/// `gnc:owner-get-owner-id` (BC:91-105): a job's owner's ID, else the owner's.
fn owner_get_owner_id(owner) {
    let end = end_owner(owner);
    if type_of(end) == "()" { "" } else { end.id }
}

/// `gnc:company-info` (OPT:409-411): a string of the Book's Business options, `key` being its name ("Company Name");
/// `""` when the Book has none.
fn company_info(book, key) {
    book_string(book, "Business/" + key)
}

/// `gnc:book-get-option-value` of a string option at `path` below `options/`; `""` when the Book has none.
fn book_string(book, path) {
    let value = book.option(path);
    if type_of(value) == "string" { value } else { "" }
}
