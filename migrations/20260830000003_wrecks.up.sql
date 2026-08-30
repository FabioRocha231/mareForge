-- A1-P5 (MF-027 cont.): wrecks no mar precisam sobreviver a restart. O baú
-- econômico já vive em `item_instances` com `location = ItemLocation::Wreck`;
-- esta tabela guarda só os metadados do wreck em si (id, posição, dono
-- exclusivo da janela, instante de spawn). Itens do baú são substituídos
-- juntos via DELETE+INSERT dentro da mesma transação do snapshot.

CREATE TABLE IF NOT EXISTS wrecks (
    wreck_num        INTEGER       PRIMARY KEY,
    wreck_id         UUID          NOT NULL UNIQUE,
    position_x       DOUBLE PRECISION NOT NULL,
    position_y       DOUBLE PRECISION NOT NULL,
    exclusive_looter UUID          NULL,
    spawned_at_secs  DOUBLE PRECISION NOT NULL
);
