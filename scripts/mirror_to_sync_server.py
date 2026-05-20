#!/usr/bin/env python3
"""
Mirror the local SQLite seeded data into the sync-server Postgres so the
push queue stops failing on FK violations.

The local DB has a full week of catalog + patients + visits + shifts +
adjustments (seeded via src-tauri/src/bin/seed_weekly.rs); the server only
has the 4 user rows that were mirrored earlier. Any new visit created in
the app references a check_type / patient / operator / doctor the server
has never seen, so the outbox push 500s on FK violations and stalls.

This script copies every table that participates in those FKs, in
dependency order, idempotently (ON CONFLICT DO UPDATE keyed on PK). It
also resets `outbox.attempts` so the engine retries the stuck rows on the
next push cycle.

Usage:
    python3 scripts/mirror_to_sync_server.py
"""

from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
from pathlib import Path

LOCAL_DB = Path.home() / ".local/share/com.idc.system/idc-local.db"
DOCKER_CMD = [
    "docker", "exec", "-i", "idc-sync-db",
    "psql", "-U", "postgres", "-d", "idc_sync",
    "-v", "ON_ERROR_STOP=1",
]


def psql(sql: str) -> tuple[int, str, str]:
    p = subprocess.run(DOCKER_CMD, input=sql, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def quote(value):
    """Render a Python value as a Postgres literal."""
    if value is None:
        return "NULL"
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, str):
        # Single-quote escape per Postgres standard.
        return "'" + value.replace("'", "''") + "'"
    raise TypeError(f"unsupported literal type {type(value)!r}")


def sqlite_bool(v):
    """SQLite stores BOOL as INT 0/1. Coerce."""
    if v is None:
        return None
    return bool(v)


def upsert_rows(
    table: str,
    pk: str,
    rows: list[dict],
    *,
    pre_sql: str = "",
) -> None:
    """Emit a single INSERT ... ON CONFLICT (pk) DO UPDATE ... statement."""
    if not rows:
        print(f"  {table}: 0 rows (skip)")
        return
    columns = list(rows[0].keys())
    values_lines = []
    for r in rows:
        cells = [r[c] for c in columns]
        values_lines.append("(" + ", ".join(cells) + ")")
    update_cols = [c for c in columns if c != pk]
    set_clause = ", ".join(f'"{c}" = EXCLUDED."{c}"' for c in update_cols)
    sql = (
        f"{pre_sql}\n"
        f"INSERT INTO \"{table}\" (\n  "
        + ",\n  ".join(f'"{c}"' for c in columns)
        + "\n) VALUES\n"
        + ",\n".join(values_lines)
        + f"\nON CONFLICT (\"{pk}\") DO UPDATE SET\n  {set_clause};"
    )
    code, out, err = psql(sql)
    if code != 0:
        print(f"  {table}: FAILED")
        print(err[-800:])
        sys.exit(1)
    print(f"  {table}: {len(rows)} rows upserted")


def main() -> None:
    if not LOCAL_DB.exists():
        print(f"local DB not found at {LOCAL_DB}")
        sys.exit(1)

    db = sqlite3.connect(str(LOCAL_DB))
    db.row_factory = sqlite3.Row

    def ts(v):
        if v is None:
            return "NULL"
        return f"{quote(v)}::timestamptz"

    def text(v):
        return quote(v)

    def jsonb(v):
        if v is None:
            return "NULL"
        # Accept either a JSON string (audit_log delta is TEXT in SQLite) or
        # a python dict / list (defensive).
        if isinstance(v, (dict, list)):
            v = json.dumps(v)
        return f"{quote(v)}::jsonb"

    def boolean(v):
        return "TRUE" if sqlite_bool(v) else "FALSE"

    def integer(v):
        return "NULL" if v is None else str(int(v))

    def enum(v, typename):
        if v is None:
            return "NULL"
        return f"{quote(v)}::\"{typename}\""

    # ---- check_types ------------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM check_types WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "name_ar": text(r["name_ar"]),
            "name_en": text(r["name_en"]),
            "has_subtypes": boolean(r["has_subtypes"]),
            "base_price_iqd": integer(r["base_price_iqd"]),
            "dye_supported": boolean(r["dye_supported"]),
            "report_supported": boolean(r["report_supported"]),
            "sort_order": integer(r["sort_order"]),
            "is_active": boolean(r["is_active"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("check_types", "id", rows)

    # ---- check_subtypes ---------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM check_subtypes WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "check_type_id": text(r["check_type_id"]),
            "name_ar": text(r["name_ar"]),
            "name_en": text(r["name_en"]),
            "price_iqd": integer(r["price_iqd"]),
            "sort_order": integer(r["sort_order"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("check_subtypes", "id", rows)

    # ---- doctors ----------------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM doctors WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "name": text(r["name"]),
            "specialty": text(r["specialty"]),
            "phone": text(r["phone"]),
            "is_active": boolean(r["is_active"]),
            "notes": text(r["notes"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("doctors", "id", rows)

    # ---- doctor_check_pricing --------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM doctor_check_pricing WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "doctor_id": text(r["doctor_id"]),
            "check_type_id": text(r["check_type_id"]),
            "check_subtype_id": text(r["check_subtype_id"]),
            "price_override_iqd": integer(r["price_override_iqd"]),
            "cut_kind": enum(r["cut_kind"], "CutKind"),
            "cut_value": integer(r["cut_value"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("doctor_check_pricing", "id", rows)

    # ---- operators -------------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM operators WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "name": text(r["name"]),
            "phone": text(r["phone"]),
            "base_cut_per_check_iqd": integer(r["base_cut_per_check_iqd"]),
            "is_active": boolean(r["is_active"]),
            "notes": text(r["notes"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("operators", "id", rows)

    # ---- operator_specialties --------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM operator_specialties WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "operator_id": text(r["operator_id"]),
            "check_type_id": text(r["check_type_id"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("operator_specialties", "id", rows)

    # ---- inventory_items -------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM inventory_items WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "name_ar": text(r["name_ar"]),
            "name_en": text(r["name_en"]),
            "unit": text(r["unit"]),
            "quantity_on_hand": integer(r["quantity_on_hand"]),
            "low_stock_threshold": integer(r["low_stock_threshold"]),
            "is_active": boolean(r["is_active"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("inventory_items", "id", rows)

    # ---- inventory_consumption_map ---------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM inventory_consumption_map WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "check_type_id": text(r["check_type_id"]),
            "check_subtype_id": text(r["check_subtype_id"]),
            "item_id": text(r["item_id"]),
            "quantity_per_check": integer(r["quantity_per_check"]),
            "on_dye_only": boolean(r["on_dye_only"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("inventory_consumption_map", "id", rows)

    # ---- patients --------------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM patients WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "name": text(r["name"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("patients", "id", rows)

    # ---- operator_shifts -------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM operator_shifts WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "operator_id": text(r["operator_id"]),
            "check_in_at": ts(r["check_in_at"]),
            "check_out_at": ts(r["check_out_at"]),
            "check_in_by_user_id": text(r["check_in_by_user_id"]),
            "check_out_by_user_id": text(r["check_out_by_user_id"]),
            "note": text(r["note"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("operator_shifts", "id", rows)

    # ---- visits ----------------------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM visits WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "patient_id": text(r["patient_id"]),
            "status": enum(r["status"], "VisitStatus"),
            "receptionist_user_id": text(r["receptionist_user_id"]),
            "check_type_id": text(r["check_type_id"]),
            "check_subtype_id": text(r["check_subtype_id"]),
            "doctor_id": text(r["doctor_id"]),
            "operator_id": text(r["operator_id"]),
            "dye": boolean(r["dye"]),
            "report": boolean(r["report"]),
            "locked_at": ts(r["locked_at"]),
            "voided_at": ts(r["voided_at"]),
            "voided_by_user_id": text(r["voided_by_user_id"]),
            "void_reason": text(r["void_reason"]),
            "price_snapshot_iqd": integer(r["price_snapshot_iqd"]),
            "dye_cost_snapshot_iqd": integer(r["dye_cost_snapshot_iqd"]),
            "report_cost_snapshot_iqd": integer(r["report_cost_snapshot_iqd"]),
            "doctor_cut_snapshot_iqd": integer(r["doctor_cut_snapshot_iqd"]),
            "operator_cut_snapshot_iqd": integer(r["operator_cut_snapshot_iqd"]),
            "internal_pct_snapshot": integer(r["internal_pct_snapshot"]),
            "total_amount_iqd_snapshot": integer(r["total_amount_iqd_snapshot"]),
            "patient_name_snapshot": text(r["patient_name_snapshot"]),
            "doctor_name_snapshot": text(r["doctor_name_snapshot"]),
            "operator_name_snapshot": text(r["operator_name_snapshot"]),
            "check_type_name_ar_snapshot": text(r["check_type_name_ar_snapshot"]),
            "check_type_name_en_snapshot": text(r["check_type_name_en_snapshot"]),
            "check_subtype_name_ar_snapshot": text(r["check_subtype_name_ar_snapshot"]),
            "check_subtype_name_en_snapshot": text(r["check_subtype_name_en_snapshot"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("visits", "id", rows)

    # ---- inventory_adjustments -------------------------------------------
    rows = []
    for r in db.execute("SELECT * FROM inventory_adjustments WHERE deleted_at IS NULL"):
        rows.append({
            "id": text(r["id"]),
            "item_id": text(r["item_id"]),
            "delta": integer(r["delta"]),
            "reason": enum(r["reason"], "AdjustmentReason"),
            "visit_id": text(r["visit_id"]),
            "note": text(r["note"]),
            "by_user_id": text(r["by_user_id"]),
            "created_at": ts(r["created_at"]),
            "updated_at": ts(r["updated_at"]),
            "deleted_at": ts(r["deleted_at"]),
            "version": integer(r["version"]),
            "last_synced_at": ts(r["last_synced_at"]),
            "origin_device_id": text(r["origin_device_id"]),
            "entity_id": text(r["entity_id"]),
        })
    upsert_rows("inventory_adjustments", "id", rows)

    # ---- Unblock the local outbox ----------------------------------------
    # Reset attempts so the engine retries on the next push tick (without
    # waiting out the exponential backoff window).
    out = sqlite3.connect(str(LOCAL_DB))
    out.execute(
        "UPDATE outbox SET attempts = 0, next_attempt_at = ?, last_error = NULL",
        ("1970-01-01T00:00:00Z",),
    )
    out.commit()
    rowcount = out.execute("SELECT COUNT(*) FROM outbox").fetchone()[0]
    print(f"\nreset {rowcount} outbox rows for immediate retry")


if __name__ == "__main__":
    main()
