-- MV-067: Renome do capitão (progresso por jogar; nunca comprado).
ALTER TABLE characters ADD COLUMN renown BIGINT NOT NULL DEFAULT 0;
