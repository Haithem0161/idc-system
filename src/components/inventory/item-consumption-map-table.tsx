import { Link } from "react-router"
import { useTranslation } from "react-i18next"

import type { InventoryConsumptionMapRecord } from "@/lib/ipc"

interface Props {
  rows: InventoryConsumptionMapRecord[]
}

/**
 * Read-only rendering of the consumption rules that reference this item.
 * Edits route through the admin module (`/admin/inventory/:id`).
 */
export function ItemConsumptionMapTable ({ rows }: Props) {
  const { t } = useTranslation()

  return (
    <div className="panel overflow-hidden">
      <div className="panel-head flex items-center justify-between">
        <div>
          <span className="panel-title">
            {t("inventory.item.consumption_map.title")}
          </span>
          <div className="text-[12px] text-ink-3 mt-1">
            {t("inventory.item.consumption_map.subtitle")}
          </div>
        </div>
      </div>
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("inventory.item.consumption_map.columns.check_type")}</th>
            <th>{t("inventory.item.consumption_map.columns.subtype")}</th>
            <th className="text-end">
              {t("inventory.item.consumption_map.columns.qty")}
            </th>
            <th className="text-end">
              {t("inventory.item.consumption_map.columns.on_dye_only")}
            </th>
            <th className="text-end">{/* edit-link */}</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={5} className="text-[12px] text-ink-3">
                {t("inventory.item.consumption_map.empty")}
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={row.id}>
                <td className="font-mono text-[12px] text-ink-3">
                  {row.check_type_id.slice(0, 8)}
                </td>
                <td className="text-[12px] text-ink-3">
                  {row.check_subtype_id ? (
                    <span className="font-mono">
                      {row.check_subtype_id.slice(0, 8)}
                    </span>
                  ) : (
                    <span className="text-ink-4">
                      {t("inventory.item.consumption_map.all_subtypes")}
                    </span>
                  )}
                </td>
                <td className="text-end font-mono">
                  {row.quantity_per_check.toLocaleString()}
                </td>
                <td className="text-end text-[12px] text-ink-3">
                  {row.on_dye_only ? "✓" : "—"}
                </td>
                <td className="text-end">
                  <Link
                    to={`/admin/check-types/${row.check_type_id}`}
                    className="text-[12px] font-medium text-ink-2 hover:text-crimson hover:underline underline-offset-4"
                  >
                    {t("inventory.item.consumption_map.edit_link")}
                  </Link>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}
