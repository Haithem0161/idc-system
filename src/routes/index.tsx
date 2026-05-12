import { createBrowserRouter } from "react-router"

import App from "@/App"
import { AppShell } from "@/components/shell/app-shell"
import { RequireAuth } from "@/components/auth/require-role"
import AdminGate from "@/routes/admin-gate"
import FirstRunPage from "@/pages/auth/first-run"
import LockPage from "@/pages/auth/lock"
import LoginPage from "@/pages/auth/login"
import NoAccessPage from "@/pages/auth/no-access"
import RootRedirect from "@/pages/index/redirect"
import HomePage from "@/pages/home"
import NotFoundPage from "@/pages/not-found"
import UsersListPage from "@/pages/admin/users/list"
import UserDetailPage from "@/pages/admin/users/detail"
import SettingsPage from "@/pages/admin/settings"

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
          { path: "home", Component: HomePage },
          {
            path: "admin",
            Component: AdminGate,
            children: [
              { path: "users", Component: UsersListPage },
              { path: "users/:id", Component: UserDetailPage },
              { path: "settings", Component: SettingsPage },
            ],
          },
        ],
      },
      { path: "*", Component: NotFoundPage },
    ],
  },
])
