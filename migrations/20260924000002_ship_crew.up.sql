-- MV-061: tripulação embarcada persiste com o navio.
ALTER TABLE ship_instances ADD COLUMN crew INTEGER NOT NULL DEFAULT 4;
