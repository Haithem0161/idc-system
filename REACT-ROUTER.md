# React Router Guide

Enterprise-grade patterns for client-side routing with React Router v7.

## Installation

```bash
pnpm add react-router
```

## Basic Setup

```tsx
// src/main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider } from 'react-router'
import App from './App'
import HomePage from './pages/Home'
import AboutPage from './pages/About'

const router = createBrowserRouter([
  {
    path: '/',
    element: <App />,
    children: [
      { index: true, element: <HomePage /> },
      { path: 'about', element: <AboutPage /> },
    ],
  },
])

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
)
```

```tsx
// src/App.tsx
import { Outlet } from 'react-router'
import { Navbar } from './components/Navbar'

export default function App() {
  return (
    <div>
      <Navbar />
      <main>
        <Outlet />  {/* Child routes render here */}
      </main>
    </div>
  )
}
```

## Route Configuration

### Basic Routes

```tsx
const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      { index: true, element: <HomePage /> },           // /
      { path: 'about', element: <AboutPage /> },        // /about
      { path: 'contact', element: <ContactPage /> },    // /contact
    ],
  },
])
```

### Dynamic Routes

```tsx
const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      { path: 'users/:userId', element: <UserProfile /> },           // /users/123
      { path: 'posts/:postId/comments/:commentId', element: <Comment /> },
      { path: 'files/*', element: <FileExplorer /> },                // Catch-all
    ],
  },
])
```

### Nested Routes

```tsx
const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      {
        path: 'dashboard',
        element: <DashboardLayout />,
        children: [
          { index: true, element: <DashboardHome /> },    // /dashboard
          { path: 'analytics', element: <Analytics /> },   // /dashboard/analytics
          { path: 'settings', element: <Settings /> },     // /dashboard/settings
        ],
      },
    ],
  },
])
```

## Navigation Hooks

### useNavigate

Programmatic navigation.

```tsx
import { useNavigate } from 'react-router'

function LoginForm() {
  const navigate = useNavigate()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    await login()

    // Navigate to route
    navigate('/dashboard')

    // Navigate with state
    navigate('/dashboard', { state: { from: '/login' } })

    // Replace history (no back button)
    navigate('/dashboard', { replace: true })

    // Navigate with query params
    navigate({ pathname: '/search', search: '?q=react' })

    // Go back/forward
    navigate(-1)  // Back
    navigate(1)   // Forward
    navigate(-2)  // Back 2 pages
  }

  return <form onSubmit={handleSubmit}>...</form>
}
```

### useParams

Access dynamic route parameters.

```tsx
import { useParams } from 'react-router'

// Route: /users/:userId
function UserProfile() {
  const { userId } = useParams()
  return <h1>User: {userId}</h1>
}

// Route: /posts/:postId/comments/:commentId
function Comment() {
  const { postId, commentId } = useParams()
  return <div>Post {postId}, Comment {commentId}</div>
}

// Catch-all route: /files/*
function FileExplorer() {
  const { '*': filepath } = useParams()
  return <div>Path: {filepath}</div>
}
```

### useSearchParams

Access and modify URL query parameters.

```tsx
import { useSearchParams } from 'react-router'

function SearchPage() {
  const [searchParams, setSearchParams] = useSearchParams()

  const query = searchParams.get('q') || ''
  const page = searchParams.get('page') || '1'

  const handleSearch = (newQuery: string) => {
    setSearchParams({ q: newQuery, page: '1' })
  }

  const handlePageChange = (newPage: number) => {
    setSearchParams(prev => {
      prev.set('page', String(newPage))
      return prev
    })
  }

  return (
    <div>
      <input
        value={query}
        onChange={e => handleSearch(e.target.value)}
      />
      <button onClick={() => handlePageChange(Number(page) + 1)}>
        Next Page
      </button>
    </div>
  )
}
```

### useLocation

Access current location object.

```tsx
import { useLocation } from 'react-router'

function CurrentPage() {
  const location = useLocation()

  console.log(location.pathname)  // /dashboard
  console.log(location.search)    // ?tab=settings
  console.log(location.hash)      // #section1
  console.log(location.state)     // { from: '/login' }

  return <div>Current path: {location.pathname}</div>
}
```

## Navigation Components

### Link

Basic navigation link.

```tsx
import { Link } from 'react-router'

function Navbar() {
  return (
    <nav>
      <Link to="/">Home</Link>
      <Link to="/about">About</Link>
      <Link to="/contact" state={{ from: 'navbar' }}>Contact</Link>
      <Link to={{ pathname: '/search', search: '?q=react' }}>Search</Link>
    </nav>
  )
}
```

### NavLink

Navigation link with active state styling.

```tsx
import { NavLink } from 'react-router'

function Navbar() {
  return (
    <nav>
      {/* className as function */}
      <NavLink
        to="/dashboard"
        className={({ isActive, isPending }) =>
          isActive ? 'active' : isPending ? 'pending' : ''
        }
      >
        Dashboard
      </NavLink>

      {/* style as function */}
      <NavLink
        to="/settings"
        style={({ isActive }) => ({
          fontWeight: isActive ? 'bold' : 'normal',
        })}
      >
        Settings
      </NavLink>

      {/* Children as function */}
      <NavLink to="/profile">
        {({ isActive }) => (
          <span className={isActive ? 'active' : ''}>Profile</span>
        )}
      </NavLink>
    </nav>
  )
}
```

## Data Loading (Loaders)

Fetch data before rendering a route.

```tsx
import { useLoaderData } from 'react-router'

// Define loader function
async function usersLoader() {
  const response = await fetch('/api/users')
  const users = await response.json()
  return { users }
}

// Use in component
function UsersPage() {
  const { users } = useLoaderData() as { users: User[] }

  return (
    <ul>
      {users.map(user => (
        <li key={user.id}>{user.name}</li>
      ))}
    </ul>
  )
}

// Configure route
const router = createBrowserRouter([
  {
    path: '/users',
    element: <UsersPage />,
    loader: usersLoader,
  },
])
```

### Loader with Params

```tsx
async function userLoader({ params }: { params: { userId: string } }) {
  const response = await fetch(`/api/users/${params.userId}`)
  if (!response.ok) {
    throw new Response('User not found', { status: 404 })
  }
  return response.json()
}

const router = createBrowserRouter([
  {
    path: '/users/:userId',
    element: <UserProfile />,
    loader: userLoader,
  },
])
```

## Form Handling (Actions)

Handle form submissions server-side style.

```tsx
import { Form, useActionData, useNavigation, redirect } from 'react-router'

// Define action function
async function createUserAction({ request }: { request: Request }) {
  const formData = await request.formData()
  const name = formData.get('name')
  const email = formData.get('email')

  try {
    const response = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, email }),
    })

    if (!response.ok) {
      return { error: 'Failed to create user' }
    }

    return redirect('/users')
  } catch (error) {
    return { error: 'Network error' }
  }
}

// Use in component
function CreateUserPage() {
  const actionData = useActionData() as { error?: string } | undefined
  const navigation = useNavigation()
  const isSubmitting = navigation.state === 'submitting'

  return (
    <Form method="post">
      {actionData?.error && <p className="error">{actionData.error}</p>}

      <input name="name" placeholder="Name" required />
      <input name="email" type="email" placeholder="Email" required />

      <button type="submit" disabled={isSubmitting}>
        {isSubmitting ? 'Creating...' : 'Create User'}
      </button>
    </Form>
  )
}

// Configure route
const router = createBrowserRouter([
  {
    path: '/users/new',
    element: <CreateUserPage />,
    action: createUserAction,
  },
])
```

## Error Handling

```tsx
import { useRouteError, isRouteErrorResponse } from 'react-router'

function ErrorBoundary() {
  const error = useRouteError()

  if (isRouteErrorResponse(error)) {
    return (
      <div>
        <h1>{error.status} {error.statusText}</h1>
        <p>{error.data}</p>
      </div>
    )
  }

  return (
    <div>
      <h1>Oops!</h1>
      <p>Something went wrong.</p>
    </div>
  )
}

const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    errorElement: <ErrorBoundary />,
    children: [
      {
        path: 'users/:userId',
        element: <UserProfile />,
        errorElement: <ErrorBoundary />,  // Route-specific error
      },
    ],
  },
])
```

## Protected Routes

```tsx
import { Navigate, useLocation } from 'react-router'
import { useAuth } from '@/hooks/useAuth'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated } = useAuth()
  const location = useLocation()

  if (!isAuthenticated) {
    return <Navigate to="/login" state={{ from: location }} replace />
  }

  return <>{children}</>
}

// Usage in router
const router = createBrowserRouter([
  {
    path: '/dashboard',
    element: (
      <ProtectedRoute>
        <DashboardPage />
      </ProtectedRoute>
    ),
  },
])
```

## Lazy Loading Routes

```tsx
import { lazy, Suspense } from 'react'

const DashboardPage = lazy(() => import('./pages/Dashboard'))
const SettingsPage = lazy(() => import('./pages/Settings'))

const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      {
        path: 'dashboard',
        element: (
          <Suspense fallback={<div>Loading...</div>}>
            <DashboardPage />
          </Suspense>
        ),
      },
      {
        path: 'settings',
        element: (
          <Suspense fallback={<div>Loading...</div>}>
            <SettingsPage />
          </Suspense>
        ),
      },
    ],
  },
])
```

## Route Organization

```
src/
├── routes/
│   ├── index.tsx           # Route configuration
│   ├── root.tsx            # Root layout
│   └── pages/
│       ├── Home.tsx
│       ├── About.tsx
│       └── dashboard/
│           ├── index.tsx   # Dashboard home
│           ├── layout.tsx  # Dashboard layout
│           └── settings.tsx
├── components/
└── main.tsx
```

```tsx
// src/routes/index.tsx
import { createBrowserRouter } from 'react-router'
import RootLayout from './root'
import HomePage from './pages/Home'
import AboutPage from './pages/About'
import DashboardLayout from './pages/dashboard/layout'
import DashboardHome from './pages/dashboard'
import DashboardSettings from './pages/dashboard/settings'

export const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      { index: true, element: <HomePage /> },
      { path: 'about', element: <AboutPage /> },
      {
        path: 'dashboard',
        element: <DashboardLayout />,
        children: [
          { index: true, element: <DashboardHome /> },
          { path: 'settings', element: <DashboardSettings /> },
        ],
      },
    ],
  },
])
```

## Best Practices

1. **Use `createBrowserRouter`** - Preferred over `<BrowserRouter>` for data APIs
2. **Colocate loaders/actions** - Keep them near route components
3. **Handle loading states** - Use `useNavigation()` for pending UI
4. **Type your loaders** - Use TypeScript for loader return types
5. **Lazy load routes** - Code-split large route components
6. **Use error boundaries** - Handle errors at route level
7. **Protect routes** - Wrap private routes in auth checks
8. **Use `<NavLink>`** - For navigation with active states
