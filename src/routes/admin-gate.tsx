import { Outlet } from "react-router"

import { RequireRole } from "@/components/auth/require-role"

export default function AdminGate () {
  return (
    <RequireRole roles={["superadmin"]}>
      <Outlet />
    </RequireRole>
  )
}
