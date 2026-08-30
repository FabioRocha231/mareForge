-- A1-P0 (MF-034): o estado econômico do servidor (ledger global, escrow,
-- carteiras, navios por personagem) exige colunas que a migração inicial
-- não cobre. Extensões aditivas — nada do schema anterior é reescrito.

-- Ledger global (faucets/sinks/trades) não é por personagem: entradas de
-- sistema têm character_id NULL. seq é o número monotônico do domínio.
ALTER TABLE ledger_entries ALTER COLUMN character_id DROP NOT NULL;
ALTER TABLE ledger_entries
    ADD COLUMN IF NOT EXISTS seq BIGINT,
    ADD COLUMN IF NOT EXISTS memo TEXT NOT NULL DEFAULT '';
CREATE UNIQUE INDEX IF NOT EXISTS idx_ledger_seq ON ledger_entries(seq);

-- Escrow e preenchimento parcial de orders (MF-024/§43).
ALTER TABLE market_orders
    ADD COLUMN IF NOT EXISTS order_num INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS filled_quantity INTEGER NOT NULL DEFAULT 0;

-- Onde o item está (PRD §29): PortStorage/ShipCargo/MarketEscrow/Wreck,
-- serializado como JSON do enum ItemLocation do servidor.
ALTER TABLE item_instances
    ADD COLUMN IF NOT EXISTS location JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Carteira global do personagem (§31): ouro não afunda com o navio.
CREATE TABLE IF NOT EXISTS wallets (
    character_id UUID PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    gold BIGINT NOT NULL DEFAULT 0 CHECK (gold >= 0)
);

-- Navios do vertical slice são definidos por enum (ShipKind), não por
-- catálogo de definição em banco.
ALTER TABLE ship_instances
    ADD COLUMN IF NOT EXISTS ship_kind TEXT NOT NULL DEFAULT 'SmallMerchant';
