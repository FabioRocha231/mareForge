-- accounts
CREATE TABLE accounts (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    password_changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- characters
CREATE TABLE characters (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    region_id UUID NOT NULL,
    last_port_region_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(account_id, name)
);
CREATE INDEX idx_characters_account ON characters(account_id);

-- ship_instances
CREATE TABLE ship_instances (
    id UUID PRIMARY KEY,
    character_id UUID NOT NULL REFERENCES characters(id) ON DELETE RESTRICT,
    definition_id UUID NOT NULL,
    equipped_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    current_hp INTEGER NOT NULL,
    current_region_id UUID NOT NULL,
    position_x DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    position_y DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    heading DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ship_instances_character ON ship_instances(character_id);

-- item_instances
CREATE TABLE item_instances (
    id UUID PRIMARY KEY,
    owner_character_id UUID NOT NULL REFERENCES characters(id) ON DELETE RESTRICT,
    definition_id UUID NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    durability SMALLINT,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_item_instances_owner ON item_instances(owner_character_id);
CREATE INDEX idx_item_instances_definition ON item_instances(definition_id);

-- ledger_entries (append-only)
CREATE TABLE ledger_entries (
    id UUID PRIMARY KEY,
    transaction_id UUID NOT NULL,
    character_id UUID NOT NULL REFERENCES characters(id) ON DELETE RESTRICT,
    delta_money BIGINT NOT NULL DEFAULT 0,
    delta_item_id UUID,
    delta_quantity INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ledger_character_time ON ledger_entries(character_id, occurred_at);

-- market_orders
CREATE TABLE market_orders (
    id UUID PRIMARY KEY,
    seller_character_id UUID NOT NULL REFERENCES characters(id) ON DELETE RESTRICT,
    item_definition_id UUID NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price BIGINT NOT NULL CHECK (unit_price > 0),
    region_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'partial', 'filled', 'cancelled', 'expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_market_orders_region_status ON market_orders(region_id, status);

-- recipes
CREATE TABLE recipes (
    id UUID PRIMARY KEY,
    output_definition_id UUID NOT NULL,
    output_quantity INTEGER NOT NULL CHECK (output_quantity > 0),
    ingredients JSONB NOT NULL DEFAULT '[]'::jsonb,
    required_station TEXT,
    craft_time_secs INTEGER NOT NULL CHECK (craft_time_secs >= 0)
);
