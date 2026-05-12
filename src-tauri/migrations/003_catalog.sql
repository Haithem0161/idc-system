-- Phase 3 catalog & reference data.
-- 8 syncable tables + FTS5 virtual table for doctors search.
-- See docs/idc-system/phase-03.md.

-- ---- check_types (PRD §6.1.2) ---------------------------------------------
CREATE TABLE IF NOT EXISTS check_types (
  id                TEXT PRIMARY KEY,
  name_ar           TEXT NOT NULL,
  name_en           TEXT NULL,
  has_subtypes      INTEGER NOT NULL CHECK (has_subtypes IN (0,1)),
  base_price_iqd    INTEGER NULL,
  dye_supported     INTEGER NOT NULL DEFAULT 0 CHECK (dye_supported IN (0,1)),
  report_supported  INTEGER NOT NULL DEFAULT 0 CHECK (report_supported IN (0,1)),
  sort_order        INTEGER NOT NULL DEFAULT 0,
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL,
  CHECK (
    (has_subtypes = 1 AND base_price_iqd IS NULL) OR
    (has_subtypes = 0 AND base_price_iqd IS NOT NULL AND base_price_iqd >= 0)
  )
);
CREATE INDEX IF NOT EXISTS check_types_sort ON check_types(entity_id, sort_order) WHERE deleted_at IS NULL;

-- ---- check_subtypes (PRD §6.1.3) -------------------------------------------
CREATE TABLE IF NOT EXISTS check_subtypes (
  id                TEXT PRIMARY KEY,
  check_type_id     TEXT NOT NULL REFERENCES check_types(id),
  name_ar           TEXT NOT NULL,
  name_en           TEXT NULL,
  price_iqd         INTEGER NOT NULL CHECK (price_iqd >= 0),
  sort_order        INTEGER NOT NULL DEFAULT 0,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS check_subtypes_type ON check_subtypes(check_type_id) WHERE deleted_at IS NULL;

-- ---- doctors (PRD §6.1.4) + FTS5 ------------------------------------------
CREATE TABLE IF NOT EXISTS doctors (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  specialty         TEXT NULL,
  phone             TEXT NULL,
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  notes             TEXT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS doctors_name ON doctors(entity_id, name) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS doctors_active ON doctors(entity_id, is_active) WHERE deleted_at IS NULL;

CREATE VIRTUAL TABLE IF NOT EXISTS doctors_fts USING fts5(name, specialty, content='doctors', content_rowid='rowid');

-- §7.33 triggers filter soft-deleted rows from FTS.
CREATE TRIGGER IF NOT EXISTS doctors_ai AFTER INSERT ON doctors WHEN new.deleted_at IS NULL BEGIN
  INSERT INTO doctors_fts(rowid, name, specialty) VALUES (new.rowid, new.name, COALESCE(new.specialty, ''));
END;
CREATE TRIGGER IF NOT EXISTS doctors_ad AFTER DELETE ON doctors BEGIN
  INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty) VALUES ('delete', old.rowid, old.name, COALESCE(old.specialty, ''));
END;
CREATE TRIGGER IF NOT EXISTS doctors_au AFTER UPDATE ON doctors BEGIN
  INSERT INTO doctors_fts(doctors_fts, rowid, name, specialty) VALUES ('delete', old.rowid, old.name, COALESCE(old.specialty, ''));
  INSERT INTO doctors_fts(rowid, name, specialty)
    SELECT new.rowid, new.name, COALESCE(new.specialty, '')
    WHERE new.deleted_at IS NULL;
END;

-- ---- doctor_check_pricing (PRD §6.1.5) ------------------------------------
CREATE TABLE IF NOT EXISTS doctor_check_pricing (
  id                  TEXT PRIMARY KEY,
  doctor_id           TEXT NOT NULL REFERENCES doctors(id),
  check_type_id       TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id    TEXT NULL REFERENCES check_subtypes(id),
  price_override_iqd  INTEGER NULL,
  cut_kind            TEXT NOT NULL CHECK (cut_kind IN ('pct','fixed')),
  cut_value           INTEGER NOT NULL CHECK (cut_value >= 0),
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  deleted_at          TEXT NULL,
  version             INTEGER NOT NULL DEFAULT 0,
  dirty               INTEGER NOT NULL DEFAULT 1,
  last_synced_at      TEXT NULL,
  origin_device_id    TEXT NULL,
  entity_id           TEXT NOT NULL,
  CHECK (cut_kind != 'pct' OR cut_value <= 100),
  CHECK (price_override_iqd IS NULL OR price_override_iqd >= 0)
);
CREATE UNIQUE INDEX IF NOT EXISTS doctor_check_pricing_unique
  ON doctor_check_pricing(doctor_id, check_type_id, IFNULL(check_subtype_id,''))
  WHERE deleted_at IS NULL;

-- ---- operators (PRD §6.1.6) ------------------------------------------------
CREATE TABLE IF NOT EXISTS operators (
  id                       TEXT PRIMARY KEY,
  name                     TEXT NOT NULL,
  phone                    TEXT NULL,
  base_cut_per_check_iqd   INTEGER NOT NULL CHECK (base_cut_per_check_iqd >= 0),
  is_active                INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  notes                    TEXT NULL,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  deleted_at               TEXT NULL,
  version                  INTEGER NOT NULL DEFAULT 0,
  dirty                    INTEGER NOT NULL DEFAULT 1,
  last_synced_at           TEXT NULL,
  origin_device_id         TEXT NULL,
  entity_id                TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS operators_active ON operators(entity_id, is_active) WHERE deleted_at IS NULL;

-- ---- operator_specialties (PRD §6.1.7) ------------------------------------
CREATE TABLE IF NOT EXISTS operator_specialties (
  id                TEXT PRIMARY KEY,
  operator_id       TEXT NOT NULL REFERENCES operators(id),
  check_type_id     TEXT NOT NULL REFERENCES check_types(id),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  deleted_at        TEXT NULL,
  version           INTEGER NOT NULL DEFAULT 0,
  dirty             INTEGER NOT NULL DEFAULT 1,
  last_synced_at    TEXT NULL,
  origin_device_id  TEXT NULL,
  entity_id         TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS operator_specialties_unique
  ON operator_specialties(operator_id, check_type_id)
  WHERE deleted_at IS NULL;

-- ---- inventory_items (PRD §6.1.12) ----------------------------------------
CREATE TABLE IF NOT EXISTS inventory_items (
  id                    TEXT PRIMARY KEY,
  name_ar               TEXT NOT NULL,
  name_en               TEXT NULL,
  unit                  TEXT NOT NULL CHECK (length(trim(unit)) > 0),
  quantity_on_hand      INTEGER NOT NULL DEFAULT 0,
  low_stock_threshold   INTEGER NOT NULL DEFAULT 0 CHECK (low_stock_threshold >= 0),
  is_active             INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  deleted_at            TEXT NULL,
  version               INTEGER NOT NULL DEFAULT 0,
  dirty                 INTEGER NOT NULL DEFAULT 1,
  last_synced_at        TEXT NULL,
  origin_device_id      TEXT NULL,
  entity_id             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS inventory_items_active
  ON inventory_items(entity_id, is_active) WHERE deleted_at IS NULL;

-- ---- inventory_consumption_map (PRD §6.1.13) ------------------------------
CREATE TABLE IF NOT EXISTS inventory_consumption_map (
  id                  TEXT PRIMARY KEY,
  check_type_id       TEXT NOT NULL REFERENCES check_types(id),
  check_subtype_id    TEXT NULL REFERENCES check_subtypes(id),
  item_id             TEXT NOT NULL REFERENCES inventory_items(id),
  quantity_per_check  INTEGER NOT NULL CHECK (quantity_per_check > 0),
  on_dye_only         INTEGER NOT NULL DEFAULT 0 CHECK (on_dye_only IN (0,1)),
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  deleted_at          TEXT NULL,
  version             INTEGER NOT NULL DEFAULT 0,
  dirty               INTEGER NOT NULL DEFAULT 1,
  last_synced_at      TEXT NULL,
  origin_device_id    TEXT NULL,
  entity_id           TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS inventory_consumption_unique
  ON inventory_consumption_map(check_type_id, IFNULL(check_subtype_id,''), item_id, on_dye_only)
  WHERE deleted_at IS NULL;
