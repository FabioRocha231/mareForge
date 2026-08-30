//! domain-economy: tipos puros de economia. Sem match engine, sem persistência.

pub mod currency;
pub mod ledger;
pub mod market;
pub mod order;
pub mod transaction;

pub use currency::{Currency, Money};
pub use ledger::{Ledger, LedgerEntry, LedgerKind};
pub use market::{validate_new_order, FeePolicy, MarketError};
pub use order::{MarketOrder, OrderStatus};
pub use transaction::Transaction;
