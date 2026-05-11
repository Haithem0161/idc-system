import { createBrowserRouter } from "react-router"
import App from "@/App"
import { AppShell } from "@/components/shell/app-shell"
import HomePage from "@/pages/home"
import NotFoundPage from "@/pages/not-found"

export const router = createBrowserRouter([
  {
    path: "/",
    Component: App,
    children: [
      {
        path: "",
        Component: AppShell,
        children: [
          { index: true, Component: HomePage },
        ],
      },
    ],
  },
  { path: "*", Component: NotFoundPage },
])
