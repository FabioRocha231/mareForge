-- Reverso da extensão de estado econômico (A1-P0).
DROP TABLE IF EXISTS wallets;
DROP INDEX IF EXISTS idx_ledger_seq;
ALTER TABLE ship_instances DROP COLUMN IF EXISTS ship_kind;
ALTER TABLE item_instances DROP COLUMN IF EXISTS location;
ALTER TABLE market_orders DROP COLUMN IF EXISTS filled_quantity;
ALTER TABLE market_orders DROP COLUMN IF EXISTS order_num;
ALTER TABLE ledger_entries DROP COLUMN IF EXISTS memo;
ALTER TABLE ledger_entries DROP COLUMN IF EXISTS seq;
ALTER TABLE ledger_entries ALTER COLUMN character_id SET NOT NULL;
