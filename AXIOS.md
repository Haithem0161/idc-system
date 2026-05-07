# Axios Guide

Enterprise-grade patterns for HTTP requests with Axios.

## Installation

```bash
pnpm add axios
```

## Basic Usage

```tsx
import axios from 'axios'

// GET request
const response = await axios.get('/api/users')
console.log(response.data)

// POST request
const newUser = await axios.post('/api/users', {
  name: 'John',
  email: 'john@example.com',
})

// PUT request
await axios.put('/api/users/1', { name: 'Jane' })

// PATCH request
await axios.patch('/api/users/1', { name: 'Jane' })

// DELETE request
await axios.delete('/api/users/1')
```

## Axios Instance

Create a configured instance for your API.

```tsx
// src/lib/axios.ts
import axios from 'axios'

export const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL || 'https://api.example.com',
  timeout: 10000,
  headers: {
    'Content-Type': 'application/json',
  },
})

// Usage
import { api } from '@/lib/axios'

const users = await api.get('/users')
const newUser = await api.post('/users', { name: 'John' })
```

## Request Configuration

```tsx
const response = await axios({
  method: 'post',
  url: '/users',
  baseURL: 'https://api.example.com',
  headers: {
    'Content-Type': 'application/json',
    'Authorization': 'Bearer token',
  },
  params: {
    page: 1,
    limit: 10,
  },
  data: {
    name: 'John',
    email: 'john@example.com',
  },
  timeout: 5000,
  withCredentials: true,  // Send cookies cross-origin
  responseType: 'json',   // 'arraybuffer', 'blob', 'document', 'json', 'text', 'stream'
})
```

## Interceptors

### Request Interceptor

Add auth token to every request.

```tsx
// src/lib/axios.ts
import axios from 'axios'

export const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL,
})

// Add auth token to requests
api.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem('token')
    if (token) {
      config.headers.Authorization = `Bearer ${token}`
    }
    return config
  },
  (error) => {
    return Promise.reject(error)
  }
)
```

### Response Interceptor

Handle errors globally.

```tsx
api.interceptors.response.use(
  (response) => {
    // Any 2xx status
    return response
  },
  (error) => {
    // Any status outside 2xx
    if (error.response) {
      switch (error.response.status) {
        case 401:
          // Unauthorized - redirect to login
          localStorage.removeItem('token')
          window.location.href = '/login'
          break
        case 403:
          // Forbidden
          console.error('Access denied')
          break
        case 404:
          // Not found
          console.error('Resource not found')
          break
        case 500:
          // Server error
          console.error('Server error')
          break
      }
    } else if (error.request) {
      // Request made but no response
      console.error('Network error')
    } else {
      // Request setup error
      console.error('Request error:', error.message)
    }

    return Promise.reject(error)
  }
)
```

### Token Refresh Interceptor

Automatically refresh expired tokens.

```tsx
import axios from 'axios'

const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL,
})

let isRefreshing = false
let failedQueue: Array<{
  resolve: (token: string) => void
  reject: (error: Error) => void
}> = []

const processQueue = (error: Error | null, token: string | null = null) => {
  failedQueue.forEach((prom) => {
    if (error) {
      prom.reject(error)
    } else {
      prom.resolve(token!)
    }
  })
  failedQueue = []
}

api.interceptors.response.use(
  (response) => response,
  async (error) => {
    const originalRequest = error.config

    if (error.response?.status === 401 && !originalRequest._retry) {
      if (isRefreshing) {
        // Queue requests while refreshing
        return new Promise((resolve, reject) => {
          failedQueue.push({ resolve, reject })
        }).then((token) => {
          originalRequest.headers.Authorization = `Bearer ${token}`
          return api(originalRequest)
        })
      }

      originalRequest._retry = true
      isRefreshing = true

      try {
        const refreshToken = localStorage.getItem('refreshToken')
        const { data } = await axios.post('/auth/refresh', { refreshToken })

        localStorage.setItem('token', data.token)
        api.defaults.headers.common.Authorization = `Bearer ${data.token}`

        processQueue(null, data.token)
        return api(originalRequest)
      } catch (refreshError) {
        processQueue(refreshError as Error, null)
        localStorage.removeItem('token')
        localStorage.removeItem('refreshToken')
        window.location.href = '/login'
        return Promise.reject(refreshError)
      } finally {
        isRefreshing = false
      }
    }

    return Promise.reject(error)
  }
)

export { api }
```

## Error Handling

```tsx
import axios, { AxiosError } from 'axios'

interface ApiError {
  message: string
  code: string
  details?: Record<string, string[]>
}

async function fetchUsers() {
  try {
    const response = await api.get('/users')
    return response.data
  } catch (error) {
    if (axios.isAxiosError(error)) {
      const axiosError = error as AxiosError<ApiError>

      if (axiosError.response) {
        // Server responded with error status
        console.error('Status:', axiosError.response.status)
        console.error('Data:', axiosError.response.data)
        console.error('Headers:', axiosError.response.headers)

        // Handle validation errors
        if (axiosError.response.status === 422) {
          const details = axiosError.response.data.details
          // Show field errors
        }
      } else if (axiosError.request) {
        // Request made but no response received
        console.error('Network error - no response received')
      } else {
        // Error setting up request
        console.error('Error:', axiosError.message)
      }
    }
    throw error
  }
}
```

## TypeScript Types

```tsx
import axios, { AxiosResponse } from 'axios'

// Define response types
interface User {
  id: string
  name: string
  email: string
}

interface PaginatedResponse<T> {
  data: T[]
  meta: {
    total: number
    page: number
    pageSize: number
    totalPages: number
  }
}

// Typed requests
async function getUser(id: string): Promise<User> {
  const response = await api.get<User>(`/users/${id}`)
  return response.data
}

async function getUsers(page = 1): Promise<PaginatedResponse<User>> {
  const response = await api.get<PaginatedResponse<User>>('/users', {
    params: { page },
  })
  return response.data
}

async function createUser(data: Omit<User, 'id'>): Promise<User> {
  const response = await api.post<User>('/users', data)
  return response.data
}

async function updateUser(id: string, data: Partial<User>): Promise<User> {
  const response = await api.patch<User>(`/users/${id}`, data)
  return response.data
}

async function deleteUser(id: string): Promise<void> {
  await api.delete(`/users/${id}`)
}
```

## Request Cancellation

```tsx
import axios from 'axios'

// Using AbortController (recommended)
const controller = new AbortController()

const fetchData = async () => {
  try {
    const response = await api.get('/users', {
      signal: controller.signal,
    })
    return response.data
  } catch (error) {
    if (axios.isCancel(error)) {
      console.log('Request cancelled')
    }
    throw error
  }
}

// Cancel the request
controller.abort()

// In React useEffect
useEffect(() => {
  const controller = new AbortController()

  const fetchUsers = async () => {
    try {
      const response = await api.get('/users', {
        signal: controller.signal,
      })
      setUsers(response.data)
    } catch (error) {
      if (!axios.isCancel(error)) {
        setError(error)
      }
    }
  }

  fetchUsers()

  return () => {
    controller.abort()
  }
}, [])
```

## File Upload

```tsx
// Single file upload
async function uploadFile(file: File): Promise<string> {
  const formData = new FormData()
  formData.append('file', file)

  const response = await api.post<{ url: string }>('/upload', formData, {
    headers: {
      'Content-Type': 'multipart/form-data',
    },
  })

  return response.data.url
}

// With progress tracking
async function uploadFileWithProgress(
  file: File,
  onProgress: (percent: number) => void
): Promise<string> {
  const formData = new FormData()
  formData.append('file', file)

  const response = await api.post<{ url: string }>('/upload', formData, {
    headers: {
      'Content-Type': 'multipart/form-data',
    },
    onUploadProgress: (progressEvent) => {
      const percent = Math.round(
        (progressEvent.loaded * 100) / (progressEvent.total || 1)
      )
      onProgress(percent)
    },
  })

  return response.data.url
}

// Multiple files
async function uploadFiles(files: File[]): Promise<string[]> {
  const formData = new FormData()
  files.forEach((file) => {
    formData.append('files', file)
  })

  const response = await api.post<{ urls: string[] }>('/upload/multiple', formData, {
    headers: {
      'Content-Type': 'multipart/form-data',
    },
  })

  return response.data.urls
}
```

## Integration with React Query

```tsx
// src/api/users.ts
import { api } from '@/lib/axios'

export const usersApi = {
  getAll: async (page = 1) => {
    const response = await api.get('/users', { params: { page } })
    return response.data
  },

  getById: async (id: string) => {
    const response = await api.get(`/users/${id}`)
    return response.data
  },

  create: async (data: CreateUserDto) => {
    const response = await api.post('/users', data)
    return response.data
  },

  update: async (id: string, data: UpdateUserDto) => {
    const response = await api.patch(`/users/${id}`, data)
    return response.data
  },

  delete: async (id: string) => {
    await api.delete(`/users/${id}`)
  },
}

// src/hooks/useUsers.ts
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { usersApi } from '@/api/users'

export function useUsers(page = 1) {
  return useQuery({
    queryKey: ['users', page],
    queryFn: () => usersApi.getAll(page),
  })
}

export function useUser(id: string) {
  return useQuery({
    queryKey: ['users', id],
    queryFn: () => usersApi.getById(id),
    enabled: !!id,
  })
}

export function useCreateUser() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: usersApi.create,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] })
    },
  })
}
```

## File Organization

```
src/
├── lib/
│   └── axios.ts          # Axios instance with interceptors
├── api/
│   ├── index.ts          # Re-exports
│   ├── users.ts          # User API functions
│   ├── posts.ts          # Posts API functions
│   └── auth.ts           # Auth API functions
├── hooks/
│   ├── useUsers.ts       # React Query hooks for users
│   └── usePosts.ts       # React Query hooks for posts
└── types/
    └── api.ts            # API response types
```

## Complete Axios Setup

```tsx
// src/lib/axios.ts
import axios, { AxiosError, InternalAxiosRequestConfig } from 'axios'

const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL,
  timeout: 10000,
  headers: {
    'Content-Type': 'application/json',
  },
})

// Request interceptor
api.interceptors.request.use(
  (config: InternalAxiosRequestConfig) => {
    const token = localStorage.getItem('token')
    if (token) {
      config.headers.Authorization = `Bearer ${token}`
    }

    // Add request timestamp for debugging
    config.metadata = { startTime: new Date() }

    return config
  },
  (error) => Promise.reject(error)
)

// Response interceptor
api.interceptors.response.use(
  (response) => {
    // Log request duration in dev
    if (import.meta.env.DEV) {
      const duration = new Date().getTime() - response.config.metadata?.startTime?.getTime()
      console.log(`${response.config.method?.toUpperCase()} ${response.config.url} - ${duration}ms`)
    }
    return response
  },
  async (error: AxiosError) => {
    if (error.response?.status === 401) {
      localStorage.removeItem('token')
      window.location.href = '/login'
    }
    return Promise.reject(error)
  }
)

export { api }

// Extend axios config type for metadata
declare module 'axios' {
  interface InternalAxiosRequestConfig {
    metadata?: {
      startTime?: Date
    }
  }
}
```

## Best Practices

1. **Create an instance** - Don't use global axios, create configured instances
2. **Centralize base URL** - Use environment variables
3. **Add interceptors** - Handle auth tokens and errors globally
4. **Type everything** - Define response types for better DX
5. **Handle cancellation** - Cancel requests on unmount
6. **Use with React Query** - Axios for transport, React Query for caching
7. **Separate API functions** - Keep API calls organized by domain
8. **Log in development** - Add request/response logging for debugging
