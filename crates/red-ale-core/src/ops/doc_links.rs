//! Document links: a link from a transaction or business document to a file or URL.
//!
//! Both keep it in an `assoc_uri` string slot, the name from before GnuCash 4 (`5.17:libgnucash/engine/Transaction.cpp:179`,
//! `5.17:libgnucash/engine/gncInvoice.c:87`). A link with no `scheme://` is a path relative to the *path head*, a user
//! preference the caller passes in. See `design/api/transactions.md`, "Document links".

use std::sync::Arc;

use crate::book_options::check_not_read_only_by_date;
use crate::ops::transactions::check_writable;
use crate::{Book, Date, DomainError, DomainResult, Frame, Guid, SlotValue};

/// The slot holding a transaction's or document's link.
pub const DOC_LINK: &str = "assoc_uri";

/// The link stored in an entity's slots, if any.
pub fn doc_link(slots: &Frame) -> Option<&str> {
    slots.get_key(DOC_LINK).and_then(SlotValue::as_str)
}

/// Store a link, or delete the slot when it is empty, as `xaccTransSetDocLink` and `gncInvoiceSetDocLink` do
/// (`5.17:libgnucash/engine/Transaction.cpp:2027-2031`, `:1845-1853`; `5.17:libgnucash/engine/gncInvoice.c:544-563`).
pub fn set_doc_link_slot(slots: &mut Frame, link: &str) {
    slots.set_opt_str(DOC_LINK, Some(link));
}

/// Set or remove a transaction's link. A read-only transaction, or one read-only by date on `today` (the book's auto
/// read-only threshold), is refused, as the register refuses both (`is_trans_readonly_and_warn`,
/// `5.17:gnucash/gnome/gnc-split-reg.c:1118-1158`, called at `:1255-1256` and `:1325-1326`).
pub fn set_transaction_link(book: &mut Book, transaction: Guid, link: &str, today: Date) -> DomainResult<()> {
    let t = book
        .transaction(transaction)
        .ok_or_else(|| DomainError::not_found("transaction", &transaction))?;
    check_writable(t)?;
    check_not_read_only_by_date(book, t, today)?;
    let t = book
        .transactions
        .get_mut(&transaction)
        .ok_or_else(|| DomainError::not_found("transaction", &transaction))?;
    set_doc_link_slot(&mut Arc::make_mut(t).slots, link);
    Ok(())
}

/// Set or remove a business document's link. Posted documents take links too: GnuCash's Manage Document Link is not
/// limited to unposted documents (`5.17:gnucash/gnome/gnc-plugin-page-invoice.cpp:150-180`).
pub fn set_invoice_link(book: &mut Book, invoice: Guid, link: &str) -> DomainResult<()> {
    let inv = book
        .business
        .invoices
        .get_mut(&invoice)
        .ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    set_doc_link_slot(&mut Arc::make_mut(inv).slots, link);
    Ok(())
}

/// A transaction's link as GnuCash's listing reads it: a link GnuCash before 3.5 stored as `file:` without `//` is the
/// relative path after `file:/` or `file:` (`gnc_doclink_convert_trans_link_uri`,
/// `5.17:gnucash/gnome-utils/dialog-doclink-utils.c:135-161`). Documents never had that form.
pub fn legacy_transaction_link(link: &str) -> &str {
    if link.starts_with("file:") && !link.starts_with("file://") {
        link.strip_prefix("file:/").or_else(|| link.strip_prefix("file:")).unwrap_or(link)
    } else {
        link
    }
}

/// True when a link is a path relative to the path head: it has no `scheme://` (`gnc_uri_get_components`,
/// `5.17:libgnucash/engine/gnc-uri-utils.c:164-172`).
pub fn is_relative(link: &str) -> bool {
    !link.contains("://")
}

/// The path head as GnuCash keeps it: a `file://` URI ending in `/` (`doclink_get_path_head_and_set`,
/// `5.17:gnucash/gnome-utils/dialog-doclink-utils.c:165-203`). An absolute path becomes a `file://` URI; anything
/// else is refused.
pub fn normalise_path_head(head: &str) -> DomainResult<String> {
    let uri = if head.starts_with('/') {
        format!("file://{head}")
    } else {
        head.to_owned()
    };
    if !uri.starts_with("file:///") {
        return Err(DomainError::invalid(format!(
            "the path head must be an absolute path or a file:// URI, not {head}"
        )));
    }
    Ok(if uri.ends_with('/') { uri } else { format!("{uri}/") })
}

/// The URI to open for a link (`gnc_doclink_get_use_uri`, `5.17:gnucash/gnome-utils/dialog-doclink-utils.c:47-121`):
/// a relative link joined to the path head as a `file://` URI, any other link as stored. A relative link with no path
/// head has none.
pub fn resolve(link: &str, path_head: Option<&str>) -> Option<String> {
    if !is_relative(link) {
        return Some(link.to_owned());
    }
    let head = path_head?.strip_prefix("file://")?;
    if link.starts_with('/') {
        return Some(format!("file://{link}"));
    }
    let sep = if head.ends_with('/') { "" } else { "/" };
    Some(format!("file://{head}{sep}{link}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_links_delete_the_slot() {
        let mut f = Frame::new();
        set_doc_link_slot(&mut f, "https://example.com/r/1");
        assert_eq!(doc_link(&f), Some("https://example.com/r/1"));
        set_doc_link_slot(&mut f, "");
        assert!(f.is_empty());
    }

    #[test]
    fn read_only_transactions_keep_their_links() {
        use crate::book_options::set_auto_readonly_days;
        use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
        use crate::{Account, AccountType, CommodityRef, Env, FixedEnv, Numeric, Time64};

        let mut env = FixedEnv::new(Time64(1_719_835_200), 5);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let accounts: Vec<Guid> = ["Bank", "Food"]
            .iter()
            .map(|name| {
                let a = Account::new(env.new_guid(), *name, AccountType::Bank, Some(usd.clone()), Some(book.root));
                let g = a.guid;
                book.accounts.insert(g, Arc::new(a));
                g
            })
            .collect();
        let mut new = NewTransaction::new(usd, Date::from_ymd(2024, 6, 20).unwrap());
        new.splits = vec![
            NewSplit::new(accounts[0], Numeric::new(-5, 1)),
            NewSplit::new(accounts[1], Numeric::new(5, 1)),
        ];
        let t = add_transaction(&mut book, &mut env, new).unwrap();
        let today = Date::from_ymd(2024, 7, 1).unwrap();
        set_transaction_link(&mut book, t, "a.pdf", today).unwrap();
        set_auto_readonly_days(&mut book, 10).unwrap();
        assert!(matches!(
            set_transaction_link(&mut book, t, "", today),
            Err(DomainError::ReadOnlyByDate { .. })
        ));
        assert_eq!(book.transaction(t).and_then(|t| doc_link(&t.slots)), Some("a.pdf"));
        set_auto_readonly_days(&mut book, 0).unwrap();
        let tx = Arc::make_mut(book.transactions.get_mut(&t).unwrap());
        tx.slots.set(
            crate::transaction::slot_paths::READ_ONLY,
            SlotValue::String("Generated from an invoice".into()),
        );
        assert!(matches!(
            set_transaction_link(&mut book, t, "", today),
            Err(DomainError::Conflict(_))
        ));
    }

    #[test]
    fn links_resolve_against_the_path_head() {
        let head = normalise_path_head("/home/me/Documents").unwrap();
        assert_eq!(head, "file:///home/me/Documents/");
        assert_eq!(normalise_path_head("file:///srv/docs/").unwrap(), "file:///srv/docs/");
        assert!(normalise_path_head("docs").is_err());
        assert!(normalise_path_head("https://example.com/").is_err());
        assert_eq!(
            resolve("receipts/42.pdf", Some(&head)).as_deref(),
            Some("file:///home/me/Documents/receipts/42.pdf")
        );
        assert_eq!(resolve("receipts/42.pdf", None), None);
        assert_eq!(resolve("https://example.com/x", None).as_deref(), Some("https://example.com/x"));
        assert_eq!(resolve("file:///tmp/a.pdf", Some(&head)).as_deref(), Some("file:///tmp/a.pdf"));
        assert!(is_relative("a b/c.pdf") && !is_relative("ftp://host/x"));
    }

    #[test]
    fn old_relative_transaction_links_are_read_as_gnucash_reads_them() {
        assert_eq!(legacy_transaction_link("file:receipts/1.pdf"), "receipts/1.pdf");
        assert_eq!(legacy_transaction_link("file:/receipts/1.pdf"), "receipts/1.pdf");
        assert_eq!(legacy_transaction_link("file:///receipts/1.pdf"), "file:///receipts/1.pdf");
        assert_eq!(legacy_transaction_link("receipts/1.pdf"), "receipts/1.pdf");
    }
}
