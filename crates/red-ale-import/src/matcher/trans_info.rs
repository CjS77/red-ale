//! One imported transaction and what the matcher decided for it (`GNCImportTransInfo`,
//! `5.17:gnucash/import-export/import-backend.cpp:82-358`, `:1092-1200`).

use red_ale_core::{Book, DENOM_AUTO, DomainError, DomainResult, Guid, How, Numeric, Round, Transaction};
use red_ale_locale::Locale;

use super::{ImportAction, ImportSettings, MatchInfo, tokens::transaction_tokens};
use crate::import_map::{CATEGORY_DESC, bayes::find_account_bayes, find_account};

/// What the importer knows about the balancing split (`GNCImportLastSplitInfo`,
/// `5.17:gnucash/import-export/import-backend.h:49-58`): the parts GnuCash 5.17 reads. Its action, memo, reconcile
/// state and date are stored and never read (`import-backend.cpp:299-312`), so a balancing split gets none of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LastSplitInfo {
    /// The price of the destination commodity in the transaction currency (value = amount × price).
    pub price: Option<Numeric>,
    /// The balancing split's amount, as the file gives it.
    pub amount: Option<Numeric>,
    /// The balancing split's account, as the file names it.
    pub account: Option<Guid>,
}

/// Where the destination account came from: the `from` of `design/api/import.md`'s `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DestSource {
    /// The file named it ([`LastSplitInfo::account`]).
    File,
    /// The Bayesian Import map.
    Bayes,
    /// The plain Import map's `desc` category.
    Map,
    /// The caller chose it ([`TransInfo::set_destination`] with `manually`).
    Caller,
}

/// An imported transaction with its candidates, Import action and destination.
#[derive(Debug, Clone, PartialEq)]
pub struct TransInfo {
    /// Its position in the order the importer built the transactions.
    pub index: usize,
    /// The imported transaction, not in the Book. Split 0 is the imported split: the one in the account the file is
    /// a statement of.
    pub trans: Transaction,
    /// Its tokens for the Bayesian Import map, taken once when it is created (`match_tokens`).
    pub tokens: Vec<String>,
    /// Candidate matches, best first.
    pub matches: Vec<MatchInfo>,
    /// The split of the selected match.
    pub selected_match: Option<Guid>,
    /// True when the caller chose the match.
    pub match_selected_manually: bool,
    /// The Import action.
    pub action: ImportAction,
    /// The action before the last change (`previous_action`).
    pub previous_action: ImportAction,
    /// The balancing split's account on *add*.
    pub dest_account: Option<Guid>,
    /// Where that account came from.
    pub dest_source: Option<DestSource>,
    /// The balancing split's value: minus the transaction's imbalance (`lsplit_value`).
    pub dest_value: Numeric,
    /// Its amount (`lsplit_amount`); zero when unknown, and the value is then used at a rate of 1.
    pub dest_amount: Numeric,
    /// True when the amount was given rather than computed.
    pub dest_amount_selected_manually: bool,
    /// The destination commodity's price in the transaction currency (`lsplit_price`).
    pub price: Option<Numeric>,
}

impl TransInfo {
    /// `gnc_import_TransInfo_new` with `set_last_split_info` (`import-backend.cpp:1127-1147`, `:291-314`): the
    /// destination is looked up in the imported split's account's Import map, then the file's account, when it names
    /// one, replaces it without the amount being recomputed, as in GnuCash.
    pub fn new(
        book: &Book,
        settings: &ImportSettings,
        index: usize,
        trans: Transaction,
        last: Option<&LastSplitInfo>,
        locale: &Locale,
    ) -> DomainResult<Self> {
        if trans.splits.is_empty() {
            return Err(DomainError::invalid(format!("imported transaction {index} has no split")));
        }
        let tokens = transaction_tokens(&trans, locale);
        let mut info = Self {
            index,
            trans,
            tokens,
            matches: Vec::new(),
            selected_match: None,
            match_selected_manually: false,
            action: ImportAction::Add,
            previous_action: ImportAction::Add,
            dest_account: None,
            dest_source: None,
            dest_value: Numeric::ZERO,
            dest_amount: Numeric::ZERO,
            dest_amount_selected_manually: false,
            price: None,
        };
        let (found, source) = info.find_destination(book, settings.use_bayes);
        info.set_destination(book, found, false);
        info.dest_source = found.map(|_| source);
        if let Some(last) = last {
            info.set_last_split_info(last);
        }
        Ok(info)
    }

    /// The imported split's account.
    pub fn account(&self) -> Option<Guid> {
        self.trans.splits.first().and_then(|s| s.account)
    }

    /// `matchmap_find_destination` (`import-backend.cpp:501-524`): from the Bayesian map with the tokens, or the
    /// plain map's `desc` category with the whole description.
    fn find_destination(&self, book: &Book, use_bayes: bool) -> (Option<Guid>, DestSource) {
        let Some(acc) = self.account() else {
            return (None, DestSource::Map);
        };
        if use_bayes {
            (find_account_bayes(book, acc, &self.tokens), DestSource::Bayes)
        } else {
            (
                find_account(book, acc, Some(CATEGORY_DESC), self.trans.description.as_deref().unwrap_or("")),
                DestSource::Map,
            )
        }
    }

    /// `gnc_import_TransInfo_set_last_split_info` (`import-backend.cpp:291-314`).
    fn set_last_split_info(&mut self, last: &LastSplitInfo) {
        self.price = last.price;
        if let Some(amount) = last.amount.filter(|a| a.is_valid()) {
            self.dest_amount = amount;
            self.dest_amount_selected_manually = true;
        }
        if let Some(account) = last.account {
            self.dest_account = Some(account);
            self.dest_source = Some(DestSource::File);
        }
    }

    /// Set the destination account (`gnc_import_TransInfo_set_destacc`, `import-backend.cpp:217-230`) and recompute
    /// the balancing split's amount. GnuCash learns a destination picked by hand at once; red-ale learns it when the
    /// batch is applied ([`super::process`]).
    pub fn set_destination(&mut self, book: &Book, account: Option<Guid>, manually: bool) {
        self.dest_account = account;
        if manually || account.is_none() {
            self.dest_source = account.map(|_| DestSource::Caller);
        }
        self.calculate_dest_amount(book);
    }

    /// True when the caller chose the destination: such a destination is learned at apply.
    pub fn dest_selected_manually(&self) -> bool {
        self.dest_source == Some(DestSource::Caller)
    }

    /// Set the price and recompute the amount (`gnc_import_TransInfo_set_price`, `import-backend.cpp:256-270`).
    pub fn set_price(&mut self, book: &Book, price: Numeric) {
        self.price = Some(price);
        if price.is_valid() {
            self.dest_amount_selected_manually = false;
            self.calculate_dest_amount(book);
        }
    }

    /// Give the balancing split's amount, as GnuCash's exchange rate dialog does (`import-main-matcher.cpp:1145-1152`).
    pub fn set_dest_amount(&mut self, book: &Book, amount: Numeric) {
        self.dest_amount = amount;
        self.dest_amount_selected_manually = true;
        self.calculate_dest_amount(book);
    }

    /// `trans_info_calculate_dest_amount` (`import-backend.cpp:1092-1124`): the value is minus the imbalance; the
    /// amount is the value when the destination holds the transaction currency, the given amount if there is one,
    /// else value ÷ price rounded half up, else zero. A zero price, which GnuCash would carry as an error value, leaves
    /// the amount zero.
    fn calculate_dest_amount(&mut self, book: &Book) {
        self.dest_value = imbalance(&self.trans).map_or(Numeric::ZERO, Numeric::neg);
        if !self.dest_amount_selected_manually {
            self.dest_amount = Numeric::ZERO;
        }
        let Some(dest) = self.dest_account.and_then(|a| book.account(a)) else {
            return;
        };
        if self.dest_value.is_zero() {
            return;
        }
        if dest.commodity.as_ref() == Some(&self.trans.currency) {
            self.dest_amount = self.dest_value;
            return;
        }
        if self.dest_amount_selected_manually && self.dest_amount.is_valid() {
            return;
        }
        if let Some(price) = self.price.filter(|p| p.is_valid()) {
            let by_price = price
                .invert()
                .and_then(|inverse| self.dest_value.mul(inverse, DENOM_AUTO, How::round(Round::HalfUp)));
            self.dest_amount = by_price.unwrap_or(Numeric::ZERO);
        }
    }

    /// Set the action, keeping the previous one (`gnc_import_TransInfo_set_action`, `import-backend.cpp:199-209`).
    pub fn set_action(&mut self, action: ImportAction) {
        if action != self.action {
            self.previous_action = self.action;
            self.action = action;
        }
    }

    /// Select one of the candidates by its split (`gnc_import_TransInfo_set_selected_match_info`). Returns false,
    /// changing nothing, when the split is not a candidate.
    pub fn select_match(&mut self, split: Guid, manually: bool) -> bool {
        if !self.matches.iter().any(|m| m.split == split) {
            return false;
        }
        self.selected_match = Some(split);
        self.match_selected_manually = manually;
        true
    }

    /// The selected candidate.
    pub fn selected(&self) -> Option<&MatchInfo> {
        self.selected_match.and_then(|s| self.matches.iter().find(|m| m.split == s))
    }

    /// `gnc_import_TransInfo_init_matches` (`import-backend.cpp:1162-1200`): sort the candidates best first (a stable
    /// sort), select the best, and choose the action from its score.
    pub fn init_matches(&mut self, settings: &ImportSettings) {
        self.matches.sort_by_key(|m| std::cmp::Reverse(m.probability));
        self.selected_match = self.matches.first().map(|m| m.split);
        self.match_selected_manually = false;
        self.action = match self.matches.first() {
            Some(best) if best.probability >= settings.auto_clear_threshold => {
                if settings.enable_update && best.update_proposed {
                    ImportAction::Update
                } else {
                    ImportAction::Clear
                }
            }
            Some(best) if best.probability > settings.auto_add_threshold => {
                if settings.enable_skip {
                    ImportAction::Skip
                } else if settings.enable_update {
                    ImportAction::Update
                } else {
                    ImportAction::Add
                }
            }
            _ => ImportAction::Add,
        };
        self.previous_action = self.action;
    }

    /// `gnc_import_TransInfo_remove_top_match` (`import-backend.cpp:133-145`): drop the best candidate and select the
    /// next. The action is **not** recomputed, as in GnuCash, unless no candidate is left: then it becomes *add*.
    pub fn remove_top_match(&mut self) {
        if !self.matches.is_empty() {
            self.matches.remove(0);
        }
        self.selected_match = self.matches.first().map(|m| m.split);
        if self.selected_match.is_none() {
            self.set_action(ImportAction::Add);
        }
    }
}

/// `xaccTransGetImbalanceValue` (`5.17:libgnucash/engine/Transaction.cpp:990-1003`): the exact sum of the values.
pub(crate) fn imbalance(t: &Transaction) -> DomainResult<Numeric> {
    Ok(t.splits.iter().try_fold(Numeric::ZERO, |acc, s| acc.add_exact(s.value))?)
}
