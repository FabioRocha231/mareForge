-- Nome de capitão para login (marvyr-auth). Nulo nas contas legadas/anônimas
-- do servidor de jogo (`char-{uuid}@local.dev`), que não fazem login.
ALTER TABLE accounts ADD COLUMN username TEXT;
CREATE UNIQUE INDEX idx_accounts_username_lower ON accounts (lower(username));
