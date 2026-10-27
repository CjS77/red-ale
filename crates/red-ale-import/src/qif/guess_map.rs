//! The QIF account map: what QIF accounts, categories and payees mapped to last time, the securities chosen, and the
//! guesses for names with no mapping.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-guess-map.scm` ("guess (or load from prefs) mappings
//! from QIF cats/accts to gnc"), Bill Gribble <grib@billgribble.com> 20 Feb 2000. GPL-2.0-or-later; this translation
//! is GPL-3.0-or-later.
//!
//! GnuCash keeps the map in a file of Scheme data in its user data directory (`qif-accounts-map`, `:114`) and writes
//! it when an import completes (`:304-344`). red-ale keeps it in its Configuration under `import.qifAccountMap`, as
//! [`SavedMap`]'s JSON (`design/api/configuration.md`). GnuCash also records the account separator and rewrites the
//! names when it changed (`:208-230`); red-ale's separator is always `:`, so nothing is recorded.

use std::collections::BTreeMap;

use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::{AccountType, Book, CommodityRef};
use serde::{Deserialize, Serialize};

use super::dialog_utils::{default_equity_account, default_unspec_acct};
use super::objects::QifMapEntry;

/// A map from QIF names to their entries: one of GnuCash's hash tables (`acct-hash`, `cat-hash`, `memo-hash`).
pub type MapHash = BTreeMap<String, QifMapEntry>;

/// An existing account as `extract-all-account-info` lists it: `(shortname fullname Account*)`.
/// `qif-guess-map.scm:104-108`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GncAcctInfo {
    /// The account's name.
    pub name: String,
    /// Its full name.
    pub full_name: String,
    /// Its type.
    pub account_type: AccountType,
}

/// Every account of the Book, in `gnc-account-get-descendants-sorted` order. `qif-guess-map.scm:183-186`.
pub fn account_info(book: &Book) -> Vec<GncAcctInfo> {
    descendants_sorted(book, book.root)
        .into_iter()
        .map(|a| GncAcctInfo {
            name: a.name.clone(),
            full_name: book.full_name(a.guid).unwrap_or_default(),
            account_type: a.account_type,
        })
        .collect()
}

/// The QIF account map as the Configuration keeps it: `import.qifAccountMap`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedMap {
    /// QIF account names.
    pub accounts: BTreeMap<String, SavedEntry>,
    /// QIF categories.
    pub categories: BTreeMap<String, SavedEntry>,
    /// QIF payees and memos.
    pub payees: BTreeMap<String, SavedEntry>,
    /// QIF security names and the commodity each was mapped to.
    pub securities: BTreeMap<String, SavedSecurity>,
}

/// One saved `<qif-map-entry>`: the fields `qif-import:write-map` writes that `read-map` gives back meaning to.
/// `qif-guess-map.scm:199-230`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEntry {
    /// The GnuCash account's full name.
    pub account: String,
    /// The account types the name may map to, as GnuCash names them (`BANK`).
    #[serde(default)]
    pub types: Vec<String>,
    /// The description, from the QIF account record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A saved security: the QIF name's commodity, by namespace and mnemonic. `qif-guess-map.scm:241-290`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSecurity {
    /// The commodity's namespace.
    pub namespace: String,
    /// Its mnemonic.
    pub mnemonic: String,
}

/// The maps `qif-import:load-map-prefs` returns. `qif-guess-map.scm:99-188`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedMaps {
    /// QIF account name → entry.
    pub accounts: MapHash,
    /// QIF category → entry.
    pub categories: MapHash,
    /// QIF payee or memo → entry.
    pub payees: MapHash,
    /// QIF security name → existing commodity (`qif-import:read-securities`).
    pub securities: BTreeMap<String, CommodityRef>,
    /// Every saved security mapping, `(name namespace mnemonic)`.
    pub security_prefs: Vec<(String, String, String)>,
}

impl SavedMap {
    /// `qif-import:load-map-prefs`: the entries with `display?` cleared (`read-map`, `:208-230`), and the saved
    /// securities whose commodity the Book has (`read-securities`, `:241-258`).
    pub fn load(&self, book: &Book) -> LoadedMaps {
        let securities = self
            .securities
            .iter()
            .map(|(name, s)| (name.clone(), CommodityRef::new(s.namespace.clone(), s.mnemonic.clone())))
            .filter(|(_, id)| red_ale_core::ops::commodities::lookup(book, id).is_some())
            .collect();
        LoadedMaps {
            accounts: read_map(&self.accounts),
            categories: read_map(&self.categories),
            payees: read_map(&self.payees),
            securities,
            security_prefs: self
                .securities
                .iter()
                .map(|(n, s)| (n.clone(), s.namespace.clone(), s.mnemonic.clone()))
                .collect(),
        }
    }

    /// `qif-import:save-map-prefs`, merged over what the Configuration holds: every entry of the three maps, and the
    /// security hash's commodities followed by the saved securities it does not name (`write-securities`,
    /// `:268-290`). `qif-guess-map.scm:304-344`.
    #[must_use]
    pub fn merged(mut self, maps: [&MapHash; 3], security_hash: &BTreeMap<String, CommodityRef>) -> Self {
        let [accounts, categories, payees] = maps;
        self.accounts.extend(write_map(accounts));
        self.categories.extend(write_map(categories));
        self.payees.extend(write_map(payees));
        self.securities.extend(security_hash.iter().map(|(name, id)| {
            (
                name.clone(),
                SavedSecurity {
                    namespace: id.namespace.clone(),
                    mnemonic: id.mnemonic.clone(),
                },
            )
        }));
        self
    }
}

impl SavedMap {
    /// The entries of `newer` over these: what saving a map over the one the Configuration holds leaves.
    pub fn extend(&mut self, newer: SavedMap) {
        self.accounts.extend(newer.accounts);
        self.categories.extend(newer.categories);
        self.payees.extend(newer.payees);
        self.securities.extend(newer.securities);
    }
}

/// `qif-import:read-map`: each saved entry, keyed by its QIF name. `qif-guess-map.scm:208-230`.
fn read_map(saved: &BTreeMap<String, SavedEntry>) -> MapHash {
    saved
        .iter()
        .map(|(key, e)| {
            let entry = QifMapEntry {
                qif_name: Some(key.clone()),
                allowed_types: e.types.iter().filter_map(|t| t.parse().ok()).collect(),
                description: e.description.clone(),
                gnc_name: Some(e.account.clone()),
                new_acct: false,
                display: false,
            };
            (key.clone(), entry)
        })
        .collect()
}

/// `qif-import:write-map`: every entry with a GnuCash name. `qif-guess-map.scm:199-206`.
fn write_map(map: &MapHash) -> impl Iterator<Item = (String, SavedEntry)> + '_ {
    map.iter().filter_map(|(key, e)| {
        let account = e.gnc_name.clone()?;
        let types = e.allowed_types.iter().map(|t| t.as_str().to_owned()).collect();
        Some((
            key.clone(),
            SavedEntry {
                account,
                types,
                description: e.description.clone(),
            },
        ))
    })
}

/// `qif-import:guess-acct`: an existing account of an allowed type with a similar name, else a new account named
/// after the QIF name. `qif-guess-map.scm:362-392`.
pub fn guess_acct(acct_name: &str, allowed_types: &[AccountType], info: &[GncAcctInfo]) -> QifMapEntry {
    let mut entry = QifMapEntry {
        qif_name: Some(acct_name.to_owned()),
        allowed_types: allowed_types.to_vec(),
        ..QifMapEntry::default()
    };
    let ((name, types), new_acct) = match find_similar_acct(acct_name, allowed_types, info) {
        Some(found) => (found, false),
        None => (find_new_acct(acct_name, allowed_types, info), true),
    };
    entry.gnc_name = Some(name);
    entry.allowed_types = types;
    entry.new_acct = new_acct;
    entry
}

/// `qif-import:find-similar-acct`: the first account, in [`account_info`]'s order, of an allowed type whose name
/// could be the QIF name, as its full name and its type. `qif-guess-map.scm:401-436`.
pub fn find_similar_acct(qif_acct_name: &str, allowed_types: &[AccountType], info: &[GncAcctInfo]) -> Option<(String, Vec<AccountType>)> {
    info.iter()
        .filter(|a| allowed_types.contains(&a.account_type))
        .find(|a| possibly_matching_name(qif_acct_name, a))
        .map(|a| (a.full_name.clone(), vec![a.account_type]))
}

/// `qif-import:possibly-matching-name?`: the QIF name, ignoring case, is the account's name or full name, or is in
/// its full name. `qif-guess-map.scm:446-463`.
pub fn possibly_matching_name(qif_acct_name: &str, gnc_acct: &GncAcctInfo) -> bool {
    let qif = qif_acct_name.to_lowercase();
    let full = gnc_acct.full_name.to_lowercase();
    qif == gnc_acct.name.to_lowercase() || qif == full || (!qif_acct_name.is_empty() && full.contains(&qif))
}

/// `qif-import:find-new-acct`: the name and types of a new account for a QIF name. `qif-guess-map.scm:473-480`.
pub fn find_new_acct(qif_acct: &str, allowed_types: &[AccountType], info: &[GncAcctInfo]) -> (String, Vec<AccountType>) {
    let equity = default_equity_account();
    if qif_acct == equity {
        find_similar_acct(&equity, &[AccountType::Equity], info).unwrap_or((equity, vec![AccountType::Equity]))
    } else if qif_acct.is_empty() {
        (default_unspec_acct().to_owned(), allowed_types.to_vec())
    } else {
        (qif_acct.to_owned(), allowed_types.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, full: &str, t: AccountType) -> GncAcctInfo {
        GncAcctInfo {
            name: name.into(),
            full_name: full.into(),
            account_type: t,
        }
    }

    #[test]
    fn similar_names_ignore_case_and_may_be_part_of_the_full_name() {
        let accounts = [
            info("Expenses", "Expenses", AccountType::Expense),
            info("Tax", "Expenses:Tax", AccountType::Expense),
            info("ABC Bank", "Assets:ABC Bank", AccountType::Bank),
        ];
        let guess = guess_acct("abc bank", &[AccountType::Bank, AccountType::Cash], &accounts);
        assert_eq!(
            (guess.gnc_name.as_deref(), guess.allowed_types.as_slice(), guess.new_acct),
            (Some("Assets:ABC Bank"), &[AccountType::Bank][..], false)
        );
        // `(string-contains "expenses:tax" "tax")`: the QIF name inside the full name.
        assert_eq!(
            guess_acct("TAX", &[AccountType::Expense], &accounts).gnc_name.as_deref(),
            Some("Expenses:Tax")
        );
        // The first in the Book's order wins: "Expenses" contains "e" before "Expenses:Tax" does.
        assert_eq!(
            guess_acct("e", &[AccountType::Expense], &accounts).gnc_name.as_deref(),
            Some("Expenses")
        );
        // A type that is not allowed never matches.
        let new = guess_acct("ABC Bank", &[AccountType::Income], &accounts);
        assert_eq!((new.gnc_name.as_deref(), new.new_acct), (Some("ABC Bank"), true));
    }

    #[test]
    fn new_names_for_empty_and_equity() {
        assert_eq!(
            find_new_acct("", &[AccountType::Expense], &[]),
            ("Unspecified".to_owned(), vec![AccountType::Expense])
        );
        let equity = info("Retained Earnings", "Equity:Retained Earnings", AccountType::Equity);
        assert_eq!(
            find_new_acct("Equity:Retained Earnings", &[AccountType::Bank], &[equity]).1,
            [AccountType::Equity]
        );
        assert_eq!(
            find_new_acct("Equity:Retained Earnings", &[AccountType::Bank], &[]),
            ("Equity:Retained Earnings".into(), vec![AccountType::Equity])
        );
    }

    #[test]
    fn saved_maps_round_trip() {
        let mut accounts = MapHash::new();
        let entry = QifMapEntry {
            qif_name: Some("Checking".into()),
            allowed_types: vec![AccountType::Bank],
            gnc_name: Some("Assets:Checking".into()),
            display: true,
            ..QifMapEntry::default()
        };
        accounts.insert("Checking".into(), entry);
        let securities = BTreeMap::from([("Acme".to_owned(), CommodityRef::new("NYSE", "ACM"))]);
        let saved = SavedMap::default().merged([&accounts, &MapHash::new(), &MapHash::new()], &securities);
        let json = serde_json::to_value(&saved).unwrap();
        assert_eq!(
            json["accounts"]["Checking"],
            serde_json::json!({ "account": "Assets:Checking", "types": ["BANK"] })
        );
        assert_eq!(
            json["securities"]["Acme"],
            serde_json::json!({ "namespace": "NYSE", "mnemonic": "ACM" })
        );
        let back: SavedMap = serde_json::from_value(json).unwrap();
        let loaded = back.load(&Book::new_empty(&mut red_ale_core::FixedEnv::new(red_ale_core::Time64(0), 1)));
        assert_eq!(loaded.accounts["Checking"].gnc_name.as_deref(), Some("Assets:Checking"));
        assert!(!loaded.accounts["Checking"].display);
        assert!(loaded.securities.is_empty(), "the Book has no NYSE::ACM");
        assert_eq!(loaded.security_prefs, [("Acme".to_owned(), "NYSE".to_owned(), "ACM".to_owned())]);
    }
}
