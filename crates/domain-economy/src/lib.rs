//! domain-economy: tipos puros de economia. Sem match engine, sem persistência.

pub mod currency;
pub mod ledger;
pub mod market;
pub mod order;
pub mod price_index;
pub mod transaction;

pub use currency::{Currency, Money};
pub use ledger::{Ledger, LedgerEntry, LedgerKind};
pub use market::{validate_new_order, FeePolicy, MarketError};
pub use order::{MarketOrder, OrderStatus};
pub use price_index::{MarketPriceIndex, Vwap};
pub use transaction::Transaction;
