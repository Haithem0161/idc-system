-- Dev catalog seed: check types, doctors, operators, inventory items.
--
-- Seeds ONLY the catalog (does NOT touch users / visits / patients / shifts),
-- so it is safe to run on a live dev DB that already has an admin.
--
-- Rows are written dirty=1 with the real tenant + device so the sync engine
-- pushes them to the server on the next authenticated push.
--
-- Usage (app MUST be closed so the WAL isn't locked):
--   DB=~/snap/code/240/.local/share/com.idc.system/idc-local.db
--   sqlite3 "$DB" < tools/dev-seed-catalog.sql
--
-- Idempotent-ish: uses INSERT OR IGNORE on stable ids, so re-running won't
-- duplicate. Tenant/device are templated below -- update if yours differ.

PRAGMA foreign_keys = ON;

-- Tenant + device for the seeded rows (your dev values).
-- ae43dc36-4f81-4a10-a771-c7263397a619  / 019ecc92-5fe4-7fe0-9c0b-c412689106c4

BEGIN;

-- ---- Check types (5) ------------------------------------------------------
INSERT OR IGNORE INTO check_types
  (id, name_ar, name_en, has_subtypes, base_price_iqd, dye_supported, report_supported, sort_order, is_active, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id)
VALUES
  ('seed-ct-0001-0000-0000-000000000001', 'أشعة سينية', 'X-Ray',        0, 15000, 0, 1, 1, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-ct-0002-0000-0000-000000000002', 'سونار',     'Ultrasound',   0, 25000, 0, 1, 2, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-ct-0003-0000-0000-000000000003', 'مفراس',     'CT Scan',      0, 75000, 1, 1, 3, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-ct-0004-0000-0000-000000000004', 'رنين',      'MRI',          0, 120000, 1, 1, 4, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-ct-0005-0000-0000-000000000005', 'تخطيط صدى', 'Echo',         0, 35000, 0, 1, 5, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619');

-- ---- Doctors (5) ----------------------------------------------------------
INSERT OR IGNORE INTO doctors
  (id, name, specialty, phone, is_active, notes, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id)
VALUES
  ('seed-dr-0001-0000-0000-000000000001', 'Dr. Ahmed Hassan',   'Radiology',    '07700000001', 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-dr-0002-0000-0000-000000000002', 'Dr. Mariam Ali',     'Cardiology',   '07700000002', 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-dr-0003-0000-0000-000000000003', 'Dr. Omar Salim',     'Internal Med', '07700000003', 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-dr-0004-0000-0000-000000000004', 'Dr. Layla Kareem',   'Pediatrics',   '07700000004', 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-dr-0005-0000-0000-000000000005', 'Dr. Yusuf Nabil',    'Neurology',    '07700000005', 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619');

-- ---- Operators (4) --------------------------------------------------------
INSERT OR IGNORE INTO operators
  (id, name, phone, base_cut_per_check_iqd, is_active, notes, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id)
VALUES
  ('seed-op-0001-0000-0000-000000000001', 'Hassan Tech',  '07710000001', 2000, 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-op-0002-0000-0000-000000000002', 'Zainab Tech',  '07710000002', 2500, 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-op-0003-0000-0000-000000000003', 'Karim Tech',   '07710000003', 2000, 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-op-0004-0000-0000-000000000004', 'Noor Tech',    '07710000004', 3000, 1, NULL, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619');

-- ---- Inventory items (8) --------------------------------------------------
INSERT OR IGNORE INTO inventory_items
  (id, name_ar, name_en, unit, quantity_on_hand, low_stock_threshold, is_active, created_at, updated_at, deleted_at, version, dirty, last_synced_at, origin_device_id, entity_id)
VALUES
  ('seed-iv-0001-0000-0000-000000000001', 'صبغة وريدية',   'IV Contrast Dye',  'vial',  120, 20, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0002-0000-0000-000000000002', 'فيلم أشعة',     'X-Ray Film',       'sheet', 500, 50, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0003-0000-0000-000000000003', 'جل سونار',      'Ultrasound Gel',   'bottle', 60, 10, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0004-0000-0000-000000000004', 'قفازات',        'Gloves',           'box',    80, 15, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0005-0000-0000-000000000005', 'حقن',           'Syringes',         'unit',  300, 40, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0006-0000-0000-000000000006', 'مناديل كحول',   'Alcohol Wipes',    'pack',   45, 10, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0007-0000-0000-000000000007', 'ورق طباعة',     'Printer Paper',    'ream',   25,  5, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619'),
  ('seed-iv-0008-0000-0000-000000000008', 'كمامات',        'Face Masks',       'box',    18,  8, 1, datetime('now'), datetime('now'), NULL, 1, 1, NULL, '019ecc92-5fe4-7fe0-9c0b-c412689106c4', 'ae43dc36-4f81-4a10-a771-c7263397a619');

COMMIT;
