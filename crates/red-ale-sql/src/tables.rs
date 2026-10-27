//! The tables red-ale reads and the columns it selects from each, in GnuCash 5.17's column order
//! (`design/notes/sql-backend.md` §2.2). Column names stay in this crate, as element names stay in `red-ale-xml`.

use crate::cell::Table;

/// `books`.
pub const BOOKS: Table = Table {
    name: "books",
    columns: &["guid", "root_account_guid", "root_template_guid"],
};

/// `commodities`.
pub const COMMODITIES: Table = Table {
    name: "commodities",
    columns: &["guid", "namespace", "mnemonic", "fullname", "cusip", "fraction", "quote_flag", "quote_source", "quote_tz"],
};

/// `accounts`.
pub const ACCOUNTS: Table = Table {
    name: "accounts",
    columns: &[
        "guid",
        "name",
        "account_type",
        "commodity_guid",
        "commodity_scu",
        "non_std_scu",
        "parent_guid",
        "code",
        "description",
        "hidden",
        "placeholder",
    ],
};

/// `budgets`.
pub const BUDGETS: Table = Table {
    name: "budgets",
    columns: &["guid", "name", "description", "num_periods"],
};

/// `budget_amounts`.
pub const BUDGET_AMOUNTS: Table = Table {
    name: "budget_amounts",
    columns: &["id", "budget_guid", "account_guid", "period_num", "amount_num", "amount_denom"],
};

/// `prices`.
pub const PRICES: Table = Table {
    name: "prices",
    columns: &["guid", "commodity_guid", "currency_guid", "date", "source", "type", "value_num", "value_denom"],
};

/// `transactions`.
pub const TRANSACTIONS: Table = Table {
    name: "transactions",
    columns: &["guid", "currency_guid", "num", "post_date", "enter_date", "description"],
};

/// `splits`.
pub const SPLITS: Table = Table {
    name: "splits",
    columns: &[
        "guid",
        "tx_guid",
        "account_guid",
        "memo",
        "action",
        "reconcile_state",
        "reconcile_date",
        "value_num",
        "value_denom",
        "quantity_num",
        "quantity_denom",
        "lot_guid",
    ],
};

/// `slots`.
pub const SLOTS: Table = Table {
    name: "slots",
    columns: &[
        "id",
        "obj_guid",
        "name",
        "slot_type",
        "int64_val",
        "string_val",
        "double_val",
        "timespec_val",
        "guid_val",
        "numeric_val_num",
        "numeric_val_denom",
        "gdate_val",
    ],
};

/// `recurrences`.
pub const RECURRENCES: Table = Table {
    name: "recurrences",
    columns: &[
        "id",
        "obj_guid",
        "recurrence_mult",
        "recurrence_period_type",
        "recurrence_period_start",
        "recurrence_weekend_adjust",
    ],
};

/// `schedxactions`.
pub const SCHEDXACTIONS: Table = Table {
    name: "schedxactions",
    columns: &[
        "guid",
        "name",
        "enabled",
        "start_date",
        "end_date",
        "last_occur",
        "num_occur",
        "rem_occur",
        "auto_create",
        "auto_notify",
        "adv_creation",
        "adv_notify",
        "instance_count",
        "template_act_guid",
    ],
};

/// `lots`.
pub const LOTS: Table = Table {
    name: "lots",
    columns: &["guid", "account_guid", "is_closed"],
};

/// `billterms`.
pub const BILLTERMS: Table = Table {
    name: "billterms",
    columns: &[
        "guid",
        "name",
        "description",
        "refcount",
        "invisible",
        "parent",
        "type",
        "duedays",
        "discountdays",
        "discount_num",
        "discount_denom",
        "cutoff",
    ],
};

/// The address columns of an owner, after their prefix (`5.17:libgnucash/backend/sql/gnc-address-sql.cpp:51-68`).
pub const ADDRESS: [&str; 8] = ["name", "addr1", "addr2", "addr3", "addr4", "phone", "fax", "email"];

/// `customers`.
pub const CUSTOMERS: Table = Table {
    name: "customers",
    columns: &[
        "guid",
        "name",
        "id",
        "notes",
        "active",
        "discount_num",
        "discount_denom",
        "credit_num",
        "credit_denom",
        "currency",
        "tax_override",
        "addr_name",
        "addr_addr1",
        "addr_addr2",
        "addr_addr3",
        "addr_addr4",
        "addr_phone",
        "addr_fax",
        "addr_email",
        "shipaddr_name",
        "shipaddr_addr1",
        "shipaddr_addr2",
        "shipaddr_addr3",
        "shipaddr_addr4",
        "shipaddr_phone",
        "shipaddr_fax",
        "shipaddr_email",
        "terms",
        "tax_included",
        "taxtable",
    ],
};

/// `employees`.
pub const EMPLOYEES: Table = Table {
    name: "employees",
    columns: &[
        "guid",
        "username",
        "id",
        "language",
        "acl",
        "active",
        "currency",
        "ccard_guid",
        "workday_num",
        "workday_denom",
        "rate_num",
        "rate_denom",
        "addr_name",
        "addr_addr1",
        "addr_addr2",
        "addr_addr3",
        "addr_addr4",
        "addr_phone",
        "addr_fax",
        "addr_email",
    ],
};

/// `entries`.
pub const ENTRIES: Table = Table {
    name: "entries",
    columns: &[
        "guid",
        "date",
        "date_entered",
        "description",
        "action",
        "notes",
        "quantity_num",
        "quantity_denom",
        "i_acct",
        "i_price_num",
        "i_price_denom",
        "i_discount_num",
        "i_discount_denom",
        "invoice",
        "i_disc_type",
        "i_disc_how",
        "i_taxable",
        "i_taxincluded",
        "i_taxtable",
        "b_acct",
        "b_price_num",
        "b_price_denom",
        "bill",
        "b_taxable",
        "b_taxincluded",
        "b_taxtable",
        "b_paytype",
        "billable",
        "billto_type",
        "billto_guid",
        "order_guid",
    ],
};

/// `invoices`.
pub const INVOICES: Table = Table {
    name: "invoices",
    columns: &[
        "guid",
        "id",
        "date_opened",
        "date_posted",
        "notes",
        "active",
        "currency",
        "owner_type",
        "owner_guid",
        "terms",
        "billing_id",
        "post_txn",
        "post_lot",
        "post_acc",
        "billto_type",
        "billto_guid",
        "charge_amt_num",
        "charge_amt_denom",
    ],
};

/// `jobs`.
pub const JOBS: Table = Table {
    name: "jobs",
    columns: &["guid", "id", "name", "reference", "active", "owner_type", "owner_guid"],
};

/// `orders`.
pub const ORDERS: Table = Table {
    name: "orders",
    columns: &["guid", "id", "notes", "reference", "active", "date_opened", "date_closed", "owner_type", "owner_guid"],
};

/// `taxtables`.
pub const TAXTABLES: Table = Table {
    name: "taxtables",
    columns: &["guid", "name", "refcount", "invisible", "parent"],
};

/// `taxtable_entries`.
pub const TAXTABLE_ENTRIES: Table = Table {
    name: "taxtable_entries",
    columns: &["id", "taxtable", "account", "amount_num", "amount_denom", "type"],
};

/// `vendors`.
pub const VENDORS: Table = Table {
    name: "vendors",
    columns: &[
        "guid",
        "name",
        "id",
        "notes",
        "currency",
        "active",
        "tax_override",
        "addr_name",
        "addr_addr1",
        "addr_addr2",
        "addr_addr3",
        "addr_addr4",
        "addr_phone",
        "addr_fax",
        "addr_email",
        "terms",
        "tax_inc",
        "tax_table",
    ],
};
