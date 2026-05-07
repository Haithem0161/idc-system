import { createBrowserRouter } from "react-router"
import App from "@/App"
import HomePage from "@/pages/home"
import NotFoundPage from "@/pages/not-found"

export const router = createBrowserRouter([
  {
    path: "/",
    Component: App,
    children: [
      { index: true, Component: HomePage },
    ],
  },
  { path: "*", Component: NotFoundPage },
])
