import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createBrowserRouter, Link, NavLink, Outlet, RouterProvider } from "react-router-dom";
import { BatchDetailPage, BatchesPage } from "./features/runs/BatchesPage";
import { TracePage } from "./features/trace/TracePage";
import { DashboardPage } from "./features/dashboard/DashboardPage";
import { ComparePage } from "./features/compare/ComparePage";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: 1, staleTime: 5000 } },
});

function Layout() {
  return (
    <div className="app">
      <nav className="topnav">
        <NavLink to="/" className="brand" end>
          agent-eval
        </NavLink>
        <div className="nav-links">
          <NavLink to="/" end className={({ isActive }) => isActive ? "nav-link active" : "nav-link"}>
            批次
          </NavLink>
          <NavLink to="/dashboard" className={({ isActive }) => isActive ? "nav-link active" : "nav-link"}>
            仪表盘
          </NavLink>
        </div>
      </nav>
      <Outlet />
    </div>
  );
}

const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <BatchesPage /> },
      { path: "/batches/:batchId", element: <BatchDetailPage /> },
      { path: "/runs/:runId", element: <TracePage /> },
      { path: "/dashboard", element: <DashboardPage /> },
      { path: "/compare", element: <ComparePage /> },
    ],
  },
]);

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
