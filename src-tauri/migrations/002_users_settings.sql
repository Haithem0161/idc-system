-- Phase 2 users and settings.
-- audit_log.action closed union (application-enforced).
-- See src-tauri/src/domains/sync/domain/value_objects.

CREATE TABLE IF NOT EXISTS users (
  id                TEXT PRIMARY KEY,
  email             TEXT NOT NULL,
  name              TEXT NOT NULL,
  password_hash     TEXT NOT NULL,
  role              TEXT NOT NULL CHECK (role IN ('superadmin','receptionist','accountant')),
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  last_login_at     TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique
  ON users(entity_id, email) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS settings (
  id                TEXT PRIMARY KEY,
  key               TEXT NOT NULL,
  value             TEXT NOT NULL,
  value_type        TEXT NOT NULL CHECK (value_type IN ('int','decimal','text','bool')),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS settings_key
  ON settings(entity_id, key) WHERE deleted_at IS NULL;

INSERT OR IGNORE INTO settings (id, key, value, value_type, created_at, updated_at, version, dirty, entity_id) VALUES
  ('01000000-0000-7000-8000-000000000001','dye_cost_iqd','10000','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000002','report_cost_iqd','10000','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000003','internal_doctor_pct','30','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000004','idle_lock_minutes','10','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000005','arabic_numerals','false','bool',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000006','clinic_display_name_ar','','text',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000007','clinic_display_name_en','','text',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000008','currency_symbol','د.ع','text',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-000000000009','thermal_width','32','int',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped'),
  ('01000000-0000-7000-8000-00000000000a','thermal_printer_name','','text',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,0,'unscoped');
