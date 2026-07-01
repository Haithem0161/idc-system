import { createBrowserRouter, Outlet } from "react-router"

import App from "@/App"
import { AppShell } from "@/components/shell/app-shell"
import { RequireAuth } from "@/components/auth/require-role"
import AdminGate from "@/routes/admin-gate"
import FirstRunPage from "@/pages/auth/first-run"
import LockPage from "@/pages/auth/lock"
import LoginPage from "@/pages/auth/login"
import NoAccessPage from "@/pages/auth/no-access"
import RootRedirect from "@/pages/index/redirect"
import NotFoundPage from "@/pages/not-found"
import UsersListPage from "@/pages/admin/users/list"
import UserDetailPage from "@/pages/admin/users/detail"
import SettingsPage from "@/pages/admin/settings"
import CheckTypesListPage from "@/pages/admin/check-types/list"
import CheckTypeDetailPage from "@/pages/admin/check-types/detail"
import DoctorsListPage from "@/pages/admin/doctors/list"
import DoctorDetailPage from "@/pages/admin/doctors/detail"
import OperatorsListPage from "@/pages/admin/operators/list"
import OperatorDetailPage from "@/pages/admin/operators/detail"
import MandoubsListPage from "@/pages/admin/mandoubs/list"
import MandoubDetailPage from "@/pages/admin/mandoubs/detail"
import InventoryCatalogListPage from "@/pages/admin/inventory/list"
import InventoryItemDetailPage from "@/pages/admin/inventory/detail"
import ShiftsPage from "@/pages/reception/shifts"
import ChecksGridPage from "@/pages/reception/checks-grid"
import CheckWorkspacePage from "@/pages/reception/check-workspace"
import NewVisitTabbedPage from "@/pages/reception/new-visit-tabbed"
import VisitDetailPage from "@/pages/reception/visit-detail"
import PatientsListPage from "@/pages/reception/patients/list"
import PatientDetailPage from "@/pages/reception/patients/detail"
import { ReceptionShell } from "@/components/reception/reception-shell"
import InventoryListPage from "@/pages/inventory/list"
import InventoryItemDetailOpsPage from "@/pages/inventory/detail"
import InventoryAdjustPage from "@/pages/inventory/adjust"
import AccountingDashboardPage from "@/pages/accounting/dashboard"
import AccountingExplorerPage from "@/pages/accounting/explorer"
import AccountingVisitDrillPage from "@/pages/accounting/visit-drill"
import AccountingDailyClosePage from "@/pages/accounting/daily-close"
import AuditPage from "@/pages/audit"
import SyncConflictsPage from "@/pages/sync/conflicts"
import { RequireRole } from "@/components/auth/require-role"

export const router = createBrowserRouter([
  {
    Component: App,
    children: [
      { path: "/login", Component: LoginPage },
      { path: "/setup/first-run", Component: FirstRunPage },
      { path: "/lock", Component: LockPage },
      { path: "/no-access", Component: NoAccessPage },
      {
        path: "/",
        element: (
          <RequireAuth>
            <AppShell />
          </RequireAuth>
        ),
        children: [
          { index: true, Component: RootRedirect },
          {
            path: "admin",
            Component: AdminGate,
            children: [
              { path: "users", Component: UsersListPage },
              { path: "users/:id", Component: UserDetailPage },
              { path: "check-types", Component: CheckTypesListPage },
              { path: "check-types/:id", Component: CheckTypeDetailPage },
              { path: "doctors", Component: DoctorsListPage },
              { path: "doctors/:id", Component: DoctorDetailPage },
              { path: "operators", Component: OperatorsListPage },
              { path: "operators/:id", Component: OperatorDetailPage },
              { path: "mandoubs", Component: MandoubsListPage },
              { path: "mandoubs/:id", Component: MandoubDetailPage },
              { path: "inventory", Component: InventoryCatalogListPage },
              { path: "inventory/:id", Component: InventoryItemDetailPage },
              { path: "settings", Component: SettingsPage },
            ],
          },
          {
            path: "reception",
            element: (
              <RequireRole roles={["receptionist", "superadmin"]}>
                <ReceptionShell />
              </RequireRole>
            ),
            children: [
              { index: true, Component: ChecksGridPage },
              { path: "shifts", Component: ShiftsPage },
              { path: "new", Component: NewVisitTabbedPage },
              { path: "checks/:slug", Component: CheckWorkspacePage },
              { path: "visits/:id", Component: VisitDetailPage },
              { path: "patients", Component: PatientsListPage },
              { path: "patients/:id", Component: PatientDetailPage },
            ],
          },
          {
            path: "inventory",
            element: (
              <RequireRole roles={["receptionist", "superadmin"]}>
                <Outlet />
              </RequireRole>
            ),
            children: [
              { index: true, Component: InventoryListPage },
              { path: "adjust", Component: InventoryAdjustPage },
              { path: "items/:id", Component: InventoryItemDetailOpsPage },
            ],
          },
          {
            path: "accounting",
            element: (
              <RequireRole roles={["accountant", "superadmin"]}>
                <Outlet />
              </RequireRole>
            ),
            handle: { crumb: () => "Accounting" },
            children: [
              { index: true, Component: AccountingDashboardPage },
              {
                path: "explore",
                handle: { crumb: () => "Explorer" },
                children: [
                  { index: true, Component: AccountingExplorerPage },
                  { path: ":entity", Component: AccountingExplorerPage },
                  { path: ":entity/:id", Component: AccountingExplorerPage },
                ],
              },
              { path: "visits/:id", Component: AccountingVisitDrillPage },
              {
                path: "daily-close",
                Component: AccountingDailyClosePage,
                handle: { crumb: () => "Daily close" },
              },
            ],
          },
          {
            path: "audit",
            element: (
              <RequireRole roles={["superadmin"]}>
                <AuditPage />
              </RequireRole>
            ),
          },
          {
            path: "sync/conflicts",
            element: (
              <RequireRole roles={["superadmin"]}>
                <SyncConflictsPage />
              </RequireRole>
            ),
          },
        ],
      },
      { path: "*", Component: NotFoundPage },
    ],
  },
])
