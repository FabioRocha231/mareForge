-- MV-066: cosméticos (velas e bandeiras). Só aparência — nunca stats.
-- Concedidos pela administração (`marvyr-db-migrate grant-cosmetic`).
CREATE TABLE character_cosmetics (
    character_id UUID NOT NULL REFERENCES characters(id) ON DELETE RESTRICT,
    cosmetic_id TEXT NOT NULL,
    granted_by TEXT NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (character_id, cosmetic_id)
);
-- O que o capitão está usando (NULL = aparência padrão do casco).
ALTER TABLE characters ADD COLUMN sail_cosmetic TEXT, ADD COLUMN flag_cosmetic TEXT;
