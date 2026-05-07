import axios, { type InternalAxiosRequestConfig } from "axios";
import { isEmbeddedMode, refreshEmbeddedToken } from "@/lib/embedded";

const api = axios.create({
  baseURL: import.meta.env.VITE_API_BASE_URL || "/api",
  timeout: 15000,
  headers: {
    "Content-Type": "application/json",
  },
});

// Request interceptor — attach auth token
api.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem("token");
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  (error) => Promise.reject(error),
);

// Response interceptor — handle 401 with token refresh
api.interceptors.response.use(
  (response) => response,
  async (error) => {
    const originalRequest = error.config as InternalAxiosRequestConfig & {
      _retry?: boolean;
    };

    if (error.response?.status === 401 && !originalRequest._retry) {
      originalRequest._retry = true;

      if (isEmbeddedMode()) {
        // Embedded mode: refresh token from Rust HTTP server's /api/auth
        const refreshed = await refreshEmbeddedToken();
        if (refreshed) {
          const newToken = localStorage.getItem("token");
          if (newToken && originalRequest.headers) {
            originalRequest.headers.Authorization = `Bearer ${newToken}`;
          }
          return api(originalRequest);
        }
        // Refresh failed — reject without redirect (Business OS handles re-auth)
        return Promise.reject(error);
      }

      // Standalone mode: clear token and redirect to login
      localStorage.removeItem("token");
      window.location.href = "/login";
    }

    return Promise.reject(error);
  },
);

export default api;
