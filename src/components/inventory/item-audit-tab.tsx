import { useTranslation } from "react-i18next"

/**
 * Audit tab for an inventory item. Phase 8 owns the full audit query IPC
 * (`audit_query`); until then this tab surfaces the contract and a friendly
 * empty state so users know where audit details land.
 */
export function ItemAuditTab ({ itemId }: { itemId: string }) {
  const { t } = useTranslation()
  return (
    <div className="panel">
      <div className="panel-head">
        <span className="panel-title">{t("inventory.item.audit.title")}</span>
        <div className="text-[12px] text-ink-3 mt-1">
          {t("inventory.item.audit.subtitle")}
        </div>
      </div>
      <div className="panel-body">
        <div className="rounded border border-line-2 bg-paper-2 p-4 text-[12px] text-ink-3">
          <div className="font-mono">entity=inventory_items</div>
          <div className="font-mono">entity_id={itemId}</div>
        </div>
        <p className="mt-3 text-[12px] text-ink-3">
          {t("inventory.item.audit.empty")}
        </p>
      </div>
    </div>
  )
}
