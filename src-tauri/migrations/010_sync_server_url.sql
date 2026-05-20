-- Phase 9: persist the sync server URL across launches.
--
-- The first-launch setup (and the superadmin first-run wizard) calls
-- `config_set_sync_server_url`, which previously only wrote to an
-- in-memory `RwLock<Option<String>>` in `AppState`. On the next launch
-- the URL was reloaded only from the `IDC_SYNC_SERVER_URL` env var --
-- if that wasn't set, the modal reappeared and the sync engine had no
-- target. Adding `server_url` to the singleton `sync_state` row lets
-- the setter persist through SQLite and the boot path restore it before
-- the engine spawns.
--
-- Idempotent: the migration runner records applied migrations in
-- `_migrations`, so this `ALTER TABLE` runs at most once per database.

ALTER TABLE sync_state ADD COLUMN server_url TEXT NULL;
