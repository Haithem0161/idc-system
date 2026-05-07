# React Best Practices & Architecture Guide

## Project Structure

```
src/
├── app/                      # App shell, providers, router
│   ├── App.tsx               # Root component
│   ├── providers.tsx         # Combined providers wrapper
│   └── router.tsx            # Route definitions
├── components/               # Shared/reusable components
│   ├── ui/                   # Primitives (Button, Input, Modal)
│   ├── forms/                # Form components
│   └── layout/               # Layout components (Header, Sidebar)
├── features/                 # Feature-based modules
│   ├── auth/
│   │   ├── components/       # Feature-specific components
│   │   ├── hooks/            # Feature-specific hooks
│   │   ├── api.ts            # API functions
│   │   ├── types.ts          # TypeScript types
│   │   └── index.ts          # Public exports
│   ├── dashboard/
│   └── settings/
├── hooks/                    # Shared custom hooks
├── lib/                      # Utilities, configs, constants
│   ├── utils.ts
│   ├── constants.ts
│   └── query-client.ts
├── types/                    # Global TypeScript types
└── styles/                   # Global styles
```

## Provider Architecture

Compose providers in a single wrapper to avoid nesting hell:

```tsx
// src/app/providers.tsx
import { QueryClientProvider } from '@tanstack/react-query'
import { queryClient } from '@/lib/query-client'

type ProvidersProps = { children: React.ReactNode }

export function Providers({ children }: ProvidersProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <ThemeProvider>
          {children}
        </ThemeProvider>
      </AuthProvider>
    </QueryClientProvider>
  )
}

// src/main.tsx
import { StrictMode } from 'react'
import { Providers } from './app/providers'
import { App } from './app/App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Providers>
      <App />
    </Providers>
  </StrictMode>
)
```

## React Router Setup

```tsx
// src/app/router.tsx
import { createBrowserRouter, RouterProvider, Outlet } from 'react-router'
import { lazy, Suspense } from 'react'

// Lazy load routes
const Dashboard = lazy(() => import('@/features/dashboard'))
const Settings = lazy(() => import('@/features/settings'))

// Layout with Outlet for nested routes
function RootLayout() {
  return (
    <div className="app-layout">
      <Header />
      <main>
        <Suspense fallback={<PageSkeleton />}>
          <Outlet />
        </Suspense>
      </main>
    </div>
  )
}

const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    errorElement: <ErrorPage />,
    children: [
      { index: true, element: <Home /> },
      { path: 'dashboard', element: <Dashboard /> },
      {
        path: 'settings',
        element: <Settings />,
        children: [
          { index: true, element: <SettingsGeneral /> },
          { path: 'profile', element: <SettingsProfile /> },
        ],
      },
    ],
  },
  { path: '*', element: <NotFound /> },
])

export function AppRouter() {
  return <RouterProvider router={router} />
}
```

## Component Patterns

### Composition Over Props

```tsx
// Prefer composition
<Card>
  <CardHeader>
    <CardTitle>Title</CardTitle>
  </CardHeader>
  <CardContent>Content</CardContent>
</Card>

// Over prop drilling
<Card title="Title" content="Content" headerIcon={<Icon />} />
```

### Container/Presentational Split

```tsx
// Container: handles logic, data fetching
function UserProfileContainer({ userId }: { userId: string }) {
  const { data, isPending } = useQuery(userQueryOptions(userId))

  if (isPending) return <ProfileSkeleton />
  return <UserProfile user={data} />
}

// Presentational: pure UI, receives data via props
function UserProfile({ user }: { user: User }) {
  return (
    <div>
      <Avatar src={user.avatar} />
      <h1>{user.name}</h1>
    </div>
  )
}
```

### Compound Components

```tsx
const TabsContext = createContext<TabsContextValue | null>(null)

function Tabs({ children, defaultValue }: TabsProps) {
  const [active, setActive] = useState(defaultValue)

  return (
    <TabsContext value={{ active, setActive }}>
      <div className="tabs">{children}</div>
    </TabsContext>
  )
}

function TabsList({ children }: { children: React.ReactNode }) {
  return <div className="tabs-list">{children}</div>
}

function TabsTrigger({ value, children }: TabsTriggerProps) {
  const { active, setActive } = use(TabsContext)!
  return (
    <button
      className={active === value ? 'active' : ''}
      onClick={() => setActive(value)}
    >
      {children}
    </button>
  )
}

function TabsContent({ value, children }: TabsContentProps) {
  const { active } = use(TabsContext)!
  if (active !== value) return null
  return <div>{children}</div>
}

// Usage
<Tabs defaultValue="tab1">
  <TabsList>
    <TabsTrigger value="tab1">Tab 1</TabsTrigger>
    <TabsTrigger value="tab2">Tab 2</TabsTrigger>
  </TabsList>
  <TabsContent value="tab1">Content 1</TabsContent>
  <TabsContent value="tab2">Content 2</TabsContent>
</Tabs>
```

## State Management

### useState - Local Component State

```tsx
// Simple state
const [count, setCount] = useState(0)

// Object state - always spread
const [form, setForm] = useState({ name: '', email: '' })
setForm(prev => ({ ...prev, name: 'John' }))

// Lazy initialization for expensive computations
const [data, setData] = useState(() => computeExpensiveValue())

// Updater function for state based on previous
setCount(prev => prev + 1)
```

### useReducer - Complex State Logic

```tsx
type State = { items: Item[]; loading: boolean; error: string | null }
type Action =
  | { type: 'FETCH_START' }
  | { type: 'FETCH_SUCCESS'; payload: Item[] }
  | { type: 'FETCH_ERROR'; payload: string }

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'FETCH_START':
      return { ...state, loading: true, error: null }
    case 'FETCH_SUCCESS':
      return { ...state, loading: false, items: action.payload }
    case 'FETCH_ERROR':
      return { ...state, loading: false, error: action.payload }
    default:
      return state
  }
}

const [state, dispatch] = useReducer(reducer, {
  items: [],
  loading: false,
  error: null,
})
```

### Context - Global/Shared State

```tsx
// 1. Create typed context
type AuthContextValue = {
  user: User | null
  login: (credentials: Credentials) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

// 2. Provider component
export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null)

  const login = async (credentials: Credentials) => {
    const user = await authApi.login(credentials)
    setUser(user)
  }

  const logout = () => {
    authApi.logout()
    setUser(null)
  }

  return (
    <AuthContext value={{ user, login, logout }}>
      {children}
    </AuthContext>
  )
}

// 3. Custom hook with null check
export function useAuth() {
  const context = use(AuthContext)
  if (!context) {
    throw new Error('useAuth must be used within AuthProvider')
  }
  return context
}
```

## Performance Optimization

### React.memo - Skip Re-renders

```tsx
// Only re-renders when props change (shallow comparison)
const ExpensiveList = memo(function ExpensiveList({ items }: Props) {
  return items.map(item => <ListItem key={item.id} item={item} />)
})

// Custom comparison
const MemoizedComponent = memo(MyComponent, (prevProps, nextProps) => {
  return prevProps.id === nextProps.id
})
```

### useMemo - Cache Computed Values

```tsx
// Cache expensive computation
const sortedItems = useMemo(
  () => items.toSorted((a, b) => a.name.localeCompare(b.name)),
  [items]
)

// Cache object/array references for child components
const config = useMemo(() => ({ theme, locale }), [theme, locale])
```

### useCallback - Cache Functions

```tsx
// Cache callback to prevent child re-renders
const handleClick = useCallback((id: string) => {
  setSelected(id)
}, [])

// With dependencies
const handleSubmit = useCallback(async (data: FormData) => {
  await submitForm(userId, data)
}, [userId])
```

### When to Optimize

**DO use memo/useMemo/useCallback:**
- Component renders frequently with same props
- Passing callbacks to memoized children
- Expensive computations in render
- Values used in dependency arrays

**DON'T over-optimize:**
- Simple components that render quickly
- Primitives (strings, numbers) as props
- Components that always need fresh data

## Custom Hooks

### Extracting Reusable Logic

```tsx
// useLocalStorage
function useLocalStorage<T>(key: string, initialValue: T) {
  const [value, setValue] = useState<T>(() => {
    const stored = localStorage.getItem(key)
    return stored ? JSON.parse(stored) : initialValue
  })

  useEffect(() => {
    localStorage.setItem(key, JSON.stringify(value))
  }, [key, value])

  return [value, setValue] as const
}

// useDebounce
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState(value)

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedValue(value), delay)
    return () => clearTimeout(timer)
  }, [value, delay])

  return debouncedValue
}

// useMediaQuery
function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(
    () => window.matchMedia(query).matches
  )

  useEffect(() => {
    const mediaQuery = window.matchMedia(query)
    const handler = (e: MediaQueryListEvent) => setMatches(e.matches)

    mediaQuery.addEventListener('change', handler)
    return () => mediaQuery.removeEventListener('change', handler)
  }, [query])

  return matches
}
```

## useEffect Patterns

### Setup and Cleanup

```tsx
useEffect(() => {
  const controller = new AbortController()

  async function fetchData() {
    try {
      const res = await fetch(url, { signal: controller.signal })
      const data = await res.json()
      setData(data)
    } catch (e) {
      if (e.name !== 'AbortError') setError(e)
    }
  }

  fetchData()
  return () => controller.abort()
}, [url])
```

### Event Listeners

```tsx
useEffect(() => {
  function handleResize() {
    setWidth(window.innerWidth)
  }

  window.addEventListener('resize', handleResize)
  return () => window.removeEventListener('resize', handleResize)
}, [])
```

### Sync with External System

```tsx
useEffect(() => {
  const connection = createConnection(serverUrl, roomId)
  connection.connect()
  return () => connection.disconnect()
}, [serverUrl, roomId])
```

## Code Splitting & Lazy Loading

```tsx
import { lazy, Suspense } from 'react'

// Lazy load components
const HeavyChart = lazy(() => import('./HeavyChart'))
const AdminPanel = lazy(() => import('./AdminPanel'))

// Named export
const Settings = lazy(() =>
  import('./Settings').then(mod => ({ default: mod.Settings }))
)

// With Suspense boundary
function Dashboard() {
  return (
    <Suspense fallback={<ChartSkeleton />}>
      <HeavyChart data={data} />
    </Suspense>
  )
}

// Route-level splitting (in router config)
const routes = [
  {
    path: '/admin',
    lazy: () => import('./features/admin'),
  },
]
```

## Error Boundaries

```tsx
import { Component, type ReactNode } from 'react'

type Props = { children: ReactNode; fallback: ReactNode }
type State = { hasError: boolean; error: Error | null }

class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('Error caught:', error, info.componentStack)
    // Log to error reporting service
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback
    }
    return this.props.children
  }
}

// Usage
<ErrorBoundary fallback={<ErrorPage />}>
  <App />
</ErrorBoundary>

// Per-feature boundaries
<ErrorBoundary fallback={<ChartError />}>
  <Chart />
</ErrorBoundary>
```

## Refs & DOM Access

### Basic Ref

```tsx
function TextInput() {
  const inputRef = useRef<HTMLInputElement>(null)

  function focusInput() {
    inputRef.current?.focus()
  }

  return <input ref={inputRef} />
}
```

### Forward Ref with useImperativeHandle

```tsx
type InputHandle = {
  focus: () => void
  clear: () => void
}

const CustomInput = forwardRef<InputHandle, InputProps>(
  function CustomInput(props, ref) {
    const inputRef = useRef<HTMLInputElement>(null)

    useImperativeHandle(ref, () => ({
      focus: () => inputRef.current?.focus(),
      clear: () => {
        if (inputRef.current) inputRef.current.value = ''
      },
    }), [])

    return <input ref={inputRef} {...props} />
  }
)

// Usage
const inputRef = useRef<InputHandle>(null)
inputRef.current?.focus()
inputRef.current?.clear()
```

## TypeScript Patterns

### Component Props

```tsx
// Props with children
type CardProps = {
  title: string
  children: React.ReactNode
}

// Optional props with defaults
type ButtonProps = {
  variant?: 'primary' | 'secondary'
  size?: 'sm' | 'md' | 'lg'
  disabled?: boolean
  onClick?: () => void
  children: React.ReactNode
}

function Button({
  variant = 'primary',
  size = 'md',
  disabled = false,
  ...props
}: ButtonProps) { ... }

// Extending HTML elements
type InputProps = React.ComponentPropsWithoutRef<'input'> & {
  label: string
  error?: string
}

// Polymorphic component
type BoxProps<T extends React.ElementType> = {
  as?: T
  children: React.ReactNode
} & React.ComponentPropsWithoutRef<T>

function Box<T extends React.ElementType = 'div'>({
  as,
  ...props
}: BoxProps<T>) {
  const Component = as || 'div'
  return <Component {...props} />
}
```

### Event Handlers

```tsx
function Form() {
  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
  }

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    console.log(e.target.value)
  }

  const handleClick = (e: React.MouseEvent<HTMLButtonElement>) => {
    console.log(e.currentTarget)
  }

  return (
    <form onSubmit={handleSubmit}>
      <input onChange={handleChange} />
      <button onClick={handleClick}>Submit</button>
    </form>
  )
}
```

## Key Rules

1. **Components must be pure** - Same props = same output, no side effects in render
2. **State is immutable** - Never mutate, always create new objects/arrays
3. **Lift state up** - Share state by moving it to closest common ancestor
4. **Colocate state** - Keep state as close as possible to where it's used
5. **Single responsibility** - One component = one purpose
6. **Composition over inheritance** - Use children and render props
7. **Custom hooks for reuse** - Extract shared logic into hooks
8. **Lazy load routes** - Code split at route level minimum
9. **Error boundaries per feature** - Isolate failures
10. **TypeScript strictly** - No `any`, explicit return types for hooks
