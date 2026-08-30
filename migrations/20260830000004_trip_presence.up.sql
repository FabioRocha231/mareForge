-- A1-P6 (MF-049): o restore pós-grace precisa saber se o navio estava no
-- mar ou atracado para definir `trip_started_at` corretamente. Coluna
-- aditiva; default `AtSea` mantém compatibilidade com linhas existentes.

ALTER TABLE ship_instances
    ADD COLUMN IF NOT EXISTS presence TEXT NOT NULL DEFAULT 'AtSea';
