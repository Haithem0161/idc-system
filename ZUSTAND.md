# Zustand Guide

Enterprise-grade patterns for lightweight state management with Zustand.

## Installation

```bash
pnpm add zustand
```

## Basic Usage

```tsx
import { create } from 'zustand'

interface BearStore {
  bears: number
  increase: () => void
  decrease: () => void
  reset: () => void
}

const useBearStore = create<BearStore>((set) => ({
  bears: 0,
  increase: () => set((state) => ({ bears: state.bears + 1 })),
  decrease: () => set((state) => ({ bears: state.bears - 1 })),
  reset: () => set({ bears: 0 }),
}))

// Use in component
function BearCounter() {
  const bears = useBearStore((state) => state.bears)
  const increase = useBearStore((state) => state.increase)

  return (
    <div>
      <h1>{bears} bears</h1>
      <button onClick={increase}>Add bear</button>
    </div>
  )
}
```

## Selectors

### Basic Selectors

```tsx
// Select single value (re-renders only when this value changes)
const bears = useBearStore((state) => state.bears)

// Select action (stable reference, no re-renders)
const increase = useBearStore((state) => state.increase)

// Select multiple values (creates new object, use shallow)
import { useShallow } from 'zustand/shallow'

const { bears, increase } = useBearStore(
  useShallow((state) => ({ bears: state.bears, increase: state.increase }))
)

// Or destructure from array
const [bears, increase] = useBearStore(
  useShallow((state) => [state.bears, state.increase])
)
```

### Computed Values

```tsx
interface Store {
  items: Item[]
  filter: 'all' | 'active' | 'completed'
}

// Compute in selector
const filteredItems = useStore((state) => {
  switch (state.filter) {
    case 'active':
      return state.items.filter((item) => !item.completed)
    case 'completed':
      return state.items.filter((item) => item.completed)
    default:
      return state.items
  }
})

// Or compute in store
const useStore = create<Store>((set, get) => ({
  items: [],
  filter: 'all',
  getFilteredItems: () => {
    const { items, filter } = get()
    // ... filter logic
  },
}))
```

## Async Actions

```tsx
interface UserStore {
  user: User | null
  loading: boolean
  error: string | null
  fetchUser: (id: string) => Promise<void>
}

const useUserStore = create<UserStore>((set) => ({
  user: null,
  loading: false,
  error: null,

  fetchUser: async (id) => {
    set({ loading: true, error: null })

    try {
      const response = await fetch(`/api/users/${id}`)
      if (!response.ok) throw new Error('Failed to fetch')
      const user = await response.json()
      set({ user, loading: false })
    } catch (error) {
      set({ error: (error as Error).message, loading: false })
    }
  },
}))

// Usage
function UserProfile({ userId }: { userId: string }) {
  const { user, loading, error, fetchUser } = useUserStore()

  useEffect(() => {
    fetchUser(userId)
  }, [userId, fetchUser])

  if (loading) return <div>Loading...</div>
  if (error) return <div>Error: {error}</div>
  if (!user) return null

  return <div>{user.name}</div>
}
```

## Middleware

### Persist (localStorage)

```tsx
import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

interface SettingsStore {
  theme: 'light' | 'dark'
  language: string
  setTheme: (theme: 'light' | 'dark') => void
  setLanguage: (language: string) => void
}

const useSettingsStore = create<SettingsStore>()(
  persist(
    (set) => ({
      theme: 'light',
      language: 'en',
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
    }),
    {
      name: 'settings-storage',  // localStorage key
      storage: createJSONStorage(() => localStorage),  // default

      // Optional: persist only specific fields
      partialize: (state) => ({
        theme: state.theme,
        language: state.language,
      }),
    }
  )
)

// Use sessionStorage instead
const useSessionStore = create<Store>()(
  persist(
    (set) => ({ /* ... */ }),
    {
      name: 'session-storage',
      storage: createJSONStorage(() => sessionStorage),
    }
  )
)
```

### DevTools

```tsx
import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

const useStore = create<Store>()(
  devtools(
    (set) => ({
      bears: 0,
      increase: () => set(
        (state) => ({ bears: state.bears + 1 }),
        undefined,
        'bears/increase'  // Action name in DevTools
      ),
    }),
    { name: 'BearStore' }  // Store name in DevTools
  )
)
```

### Immer (Mutable Updates)

```tsx
import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'

interface TodoStore {
  todos: Todo[]
  addTodo: (text: string) => void
  toggleTodo: (id: string) => void
  updateTodo: (id: string, text: string) => void
}

const useTodoStore = create<TodoStore>()(
  immer((set) => ({
    todos: [],

    addTodo: (text) => set((state) => {
      state.todos.push({ id: crypto.randomUUID(), text, completed: false })
    }),

    toggleTodo: (id) => set((state) => {
      const todo = state.todos.find((t) => t.id === id)
      if (todo) todo.completed = !todo.completed
    }),

    updateTodo: (id, text) => set((state) => {
      const todo = state.todos.find((t) => t.id === id)
      if (todo) todo.text = text
    }),
  }))
)
```

### Combining Middleware

```tsx
import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'
import { immer } from 'zustand/middleware/immer'

const useStore = create<Store>()(
  devtools(
    persist(
      immer((set) => ({
        // ... state and actions
      })),
      { name: 'app-storage' }
    ),
    { name: 'AppStore' }
  )
)
```

## Slices Pattern

Organize large stores into modular slices.

```tsx
import { create, StateCreator } from 'zustand'

// Define slice types
interface UserSlice {
  user: User | null
  setUser: (user: User | null) => void
}

interface CartSlice {
  items: CartItem[]
  addItem: (item: CartItem) => void
  removeItem: (id: string) => void
  clearCart: () => void
}

interface SettingsSlice {
  theme: 'light' | 'dark'
  toggleTheme: () => void
}

// Combined store type
type AppStore = UserSlice & CartSlice & SettingsSlice

// Create slices
const createUserSlice: StateCreator<AppStore, [], [], UserSlice> = (set) => ({
  user: null,
  setUser: (user) => set({ user }),
})

const createCartSlice: StateCreator<AppStore, [], [], CartSlice> = (set) => ({
  items: [],
  addItem: (item) => set((state) => ({ items: [...state.items, item] })),
  removeItem: (id) => set((state) => ({
    items: state.items.filter((item) => item.id !== id),
  })),
  clearCart: () => set({ items: [] }),
})

const createSettingsSlice: StateCreator<AppStore, [], [], SettingsSlice> = (set) => ({
  theme: 'light',
  toggleTheme: () => set((state) => ({
    theme: state.theme === 'light' ? 'dark' : 'light',
  })),
})

// Combine slices
const useAppStore = create<AppStore>()((...args) => ({
  ...createUserSlice(...args),
  ...createCartSlice(...args),
  ...createSettingsSlice(...args),
}))

// Export typed selectors
export const useUser = () => useAppStore((state) => state.user)
export const useCart = () => useAppStore((state) => state.items)
export const useTheme = () => useAppStore((state) => state.theme)
```

## Access State Outside React

```tsx
// Get current state
const bears = useBearStore.getState().bears

// Set state directly
useBearStore.setState({ bears: 10 })

// Subscribe to changes
const unsub = useBearStore.subscribe((state, prevState) => {
  console.log('State changed:', state.bears)
})

// Cleanup subscription
unsub()

// Subscribe to specific slice
import { subscribeWithSelector } from 'zustand/middleware'

const useStore = create<Store>()(
  subscribeWithSelector((set) => ({
    bears: 0,
    // ...
  }))
)

// Subscribe to specific field
const unsub = useStore.subscribe(
  (state) => state.bears,
  (bears, prevBears) => {
    console.log('Bears changed:', prevBears, '->', bears)
  }
)
```

## TypeScript Patterns

### Typed Store Factory

```tsx
import { create } from 'zustand'

// Generic store factory
function createCounterStore(initialCount = 0) {
  return create<{
    count: number
    increment: () => void
    decrement: () => void
    reset: () => void
  }>((set) => ({
    count: initialCount,
    increment: () => set((s) => ({ count: s.count + 1 })),
    decrement: () => set((s) => ({ count: s.count - 1 })),
    reset: () => set({ count: initialCount }),
  }))
}

// Create instances
const usePageViews = createCounterStore(0)
const useCartCount = createCounterStore(0)
```

### Separate Actions from State

```tsx
interface State {
  bears: number
  fishes: number
}

interface Actions {
  addBear: () => void
  addFish: () => void
  reset: () => void
}

const initialState: State = {
  bears: 0,
  fishes: 0,
}

const useStore = create<State & Actions>((set) => ({
  ...initialState,

  addBear: () => set((s) => ({ bears: s.bears + 1 })),
  addFish: () => set((s) => ({ fishes: s.fishes + 1 })),
  reset: () => set(initialState),
}))
```

## File Organization

```
src/
├── stores/
│   ├── index.ts              # Re-exports all stores
│   ├── useUserStore.ts       # User store
│   ├── useCartStore.ts       # Cart store
│   ├── useSettingsStore.ts   # Settings store
│   └── slices/               # For slices pattern
│       ├── userSlice.ts
│       ├── cartSlice.ts
│       └── settingsSlice.ts
└── ...
```

```tsx
// src/stores/index.ts
export { useUserStore } from './useUserStore'
export { useCartStore } from './useCartStore'
export { useSettingsStore } from './useSettingsStore'
```

## Best Practices

1. **Use selectors** - Select only what you need to minimize re-renders
2. **Use `useShallow`** - When selecting multiple values as object/array
3. **Keep stores focused** - One store per domain, use slices for large apps
4. **Persist wisely** - Only persist what's necessary (user prefs, not cache)
5. **Name actions in devtools** - Helps debugging in Redux DevTools
6. **Type everything** - Use TypeScript interfaces for state and actions
7. **Actions as methods** - Keep actions inside the store, not separate
8. **Reset functionality** - Always include a way to reset to initial state
