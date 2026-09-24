DROP INDEX IF EXISTS idx_accounts_username_lower;
ALTER TABLE accounts DROP COLUMN IF EXISTS username;
