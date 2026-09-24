-- MV-067: Rosa dos Ventos — ids dos talentos aprendidos (ganhos com Renome).
ALTER TABLE characters ADD COLUMN talents TEXT[] NOT NULL DEFAULT '{}';
