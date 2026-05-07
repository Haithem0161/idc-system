# TanStack React Query Guide

## Installation

```bash
pnpm add @tanstack/react-query @tanstack/react-query-devtools
pnpm add -D @tanstack/eslint-plugin-query
```

## QueryClient Setup

Create a QueryClient with production-ready defaults:

```typescript
// src/lib/query-client.ts
import { QueryClient } from '@tanstack/react-query'

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,           // 1 minute before data considered stale
      gcTime: 5 * 60 * 1000,          // 5 minutes garbage collection
      retry: 3,                        // Retry failed requests 3 times
      retryDelay: (attempt) =>         // Exponential backoff with 30s cap
        Math.min(attempt > 1 ? 2 ** attempt * 1000 : 1000, 30 * 1000),
      refetchOnWindowFocus: false,     // Disable for most apps
    },
    mutations: {
      retry: 0,                        // Don't retry mutations by default
    },
  },
})
```

Provider setup in main.tsx:

```typescript
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import { queryClient } from './lib/query-client'

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <YourApp />
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  )
}
```

## Query Key Factory Pattern

Centralize query keys for type safety and consistency:

```typescript
// src/lib/query-keys.ts
export const queryKeys = {
  users: {
    all: ['users'] as const,
    lists: () => [...queryKeys.users.all, 'list'] as const,
    list: (filters: UserFilters) => [...queryKeys.users.lists(), filters] as const,
    details: () => [...queryKeys.users.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.users.details(), id] as const,
  },
  posts: {
    all: ['posts'] as const,
    list: (filters?: PostFilters) => [...queryKeys.posts.all, 'list', filters] as const,
    detail: (id: string) => [...queryKeys.posts.all, 'detail', id] as const,
  },
} as const
```

## Query Options Pattern (Type-Safe)

Use `queryOptions` helper for reusable, type-safe query definitions:

```typescript
// src/queries/users.ts
import { queryOptions } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

export const userQueryOptions = (userId: string) =>
  queryOptions({
    queryKey: queryKeys.users.detail(userId),
    queryFn: () => fetchUser(userId),
    staleTime: 5 * 60 * 1000,
  })

export const usersListQueryOptions = (filters: UserFilters) =>
  queryOptions({
    queryKey: queryKeys.users.list(filters),
    queryFn: () => fetchUsers(filters),
  })

// Usage - type inference works everywhere
const { data } = useQuery(userQueryOptions(userId))
await queryClient.prefetchQuery(userQueryOptions(userId))
const cached = queryClient.getQueryData(userQueryOptions(userId).queryKey)
//    ^? User | undefined (automatically inferred)
```

## useQuery Patterns

Basic query with proper loading/error states:

```typescript
function UserProfile({ userId }: { userId: string }) {
  const { data, isPending, isError, error, refetch } = useQuery({
    queryKey: queryKeys.users.detail(userId),
    queryFn: () => fetchUser(userId),
    enabled: !!userId, // Only fetch when userId exists
  })

  if (isPending) return <Skeleton />
  if (isError) return <ErrorMessage error={error} retry={refetch} />
  return <Profile user={data} />
}
```

Data transformation with `select`:

```typescript
const { data: userNames } = useQuery({
  queryKey: queryKeys.users.all,
  queryFn: fetchUsers,
  select: useCallback((users: User[]) => users.map(u => u.name), []),
})
```

## Dependent Queries

Chain queries where one depends on another:

```typescript
const { data: user } = useQuery({
  queryKey: ['user', email],
  queryFn: () => getUserByEmail(email),
})

const { data: projects } = useQuery({
  queryKey: ['projects', user?.id],
  queryFn: () => getProjectsByUser(user!.id),
  enabled: !!user?.id, // Only runs after user is fetched
})
```

## Parallel Queries with useQueries

Fetch multiple queries in parallel:

```typescript
const { data: userIds } = useQuery({
  queryKey: ['users'],
  queryFn: getUsersData,
  select: (users) => users.map((user) => user.id),
})

const usersMessages = useQueries({
  queries: userIds
    ? userIds.map((id) => ({
        queryKey: ['messages', id],
        queryFn: () => getMessagesByUsers(id),
      }))
    : [],
})
```

## useMutation with Optimistic Updates

```typescript
function TodoList() {
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: updateTodo,
    onMutate: async (newTodo) => {
      // Cancel outgoing refetches
      await queryClient.cancelQueries({ queryKey: ['todos'] })

      // Snapshot previous value
      const previousTodos = queryClient.getQueryData(['todos'])

      // Optimistically update
      queryClient.setQueryData(['todos'], (old: Todo[]) => [...old, newTodo])

      // Return context for rollback
      return { previousTodos }
    },
    onError: (err, newTodo, context) => {
      // Rollback on error
      queryClient.setQueryData(['todos'], context?.previousTodos)
    },
    onSettled: () => {
      // Always refetch after mutation
      queryClient.invalidateQueries({ queryKey: ['todos'] })
    },
  })

  return (
    <button
      onClick={() => mutation.mutate(newTodo)}
      disabled={mutation.isPending}
    >
      {mutation.isPending ? 'Saving...' : 'Save'}
    </button>
  )
}
```

## Mutation with Cache Update (No Refetch)

When API returns the updated entity:

```typescript
const mutation = useMutation({
  mutationFn: editTodo,
  onSuccess: (updatedTodo) => {
    // Update cache directly with response data
    queryClient.setQueryData(['todo', { id: updatedTodo.id }], updatedTodo)
  },
})
```

## Infinite Queries

```typescript
const {
  data,
  fetchNextPage,
  hasNextPage,
  isFetchingNextPage,
} = useInfiniteQuery({
  queryKey: ['projects'],
  queryFn: ({ pageParam }) => fetchProjects(pageParam),
  initialPageParam: 0,
  getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  getPreviousPageParam: (firstPage) => firstPage.prevCursor ?? undefined,
  maxPages: 10, // Optional: limit cached pages
})

// Render all pages
{data?.pages.map((page) =>
  page.items.map((item) => <ProjectCard key={item.id} project={item} />)
)}

// Load more button
<button onClick={() => fetchNextPage()} disabled={!hasNextPage || isFetchingNextPage}>
  {isFetchingNextPage ? 'Loading...' : hasNextPage ? 'Load More' : 'Nothing more'}
</button>
```

## Cache Invalidation Patterns

```typescript
const queryClient = useQueryClient()

// Invalidate single query
queryClient.invalidateQueries({ queryKey: ['todos', todoId] })

// Invalidate all queries starting with 'todos'
queryClient.invalidateQueries({ queryKey: ['todos'] })

// Invalidate but don't refetch
queryClient.invalidateQueries({ queryKey: ['todos'], refetchType: 'none' })

// Invalidate and refetch inactive queries too
queryClient.invalidateQueries({ queryKey: ['todos'], refetchType: 'all' })

// Remove from cache entirely
queryClient.removeQueries({ queryKey: ['todos', todoId] })
```

## Error Boundaries

```typescript
import { QueryErrorResetBoundary } from '@tanstack/react-query'
import { ErrorBoundary } from 'react-error-boundary'

function App() {
  return (
    <QueryErrorResetBoundary>
      {({ reset }) => (
        <ErrorBoundary onReset={reset} FallbackComponent={ErrorFallback}>
          <Suspense fallback={<Loading />}>
            <YourApp />
          </Suspense>
        </ErrorBoundary>
      )}
    </QueryErrorResetBoundary>
  )
}

// Per-query error boundary opt-in
useQuery({
  queryKey: ['critical-data'],
  queryFn: fetchCriticalData,
  throwOnError: true, // Propagate to error boundary
  // Or conditionally: throwOnError: (error) => error.status >= 500
})
```

## ESLint Configuration

Add to eslint.config.js:

```javascript
import pluginQuery from '@tanstack/eslint-plugin-query'

export default [
  // ... other configs
  {
    plugins: {
      '@tanstack/query': pluginQuery,
    },
    rules: {
      '@tanstack/query/exhaustive-deps': 'error',
      '@tanstack/query/stable-query-client': 'error',
      '@tanstack/query/no-rest-destructuring': 'warn',
    },
  },
]
```

## File Organization

```
src/
├── lib/
│   ├── query-client.ts      # QueryClient configuration
│   └── query-keys.ts        # Query key factory
├── queries/
│   ├── users.ts             # User query options & hooks
│   ├── posts.ts             # Post query options & hooks
│   └── index.ts             # Re-exports
├── mutations/
│   ├── users.ts             # User mutations
│   └── posts.ts             # Post mutations
└── api/
    └── client.ts            # Fetch wrapper / API client
```

## Key Rules

1. **Query keys must be deterministic** - same inputs = same key array
2. **Always use query key factory** - never hardcode keys inline
3. **Use `queryOptions` helper** - ensures type safety across all usages
4. **Prefer `invalidateQueries` over refetch** - lets React Query decide when to refetch
5. **Use `enabled` for conditional fetching** - not ternaries in queryFn
6. **Return `undefined` from `getNextPageParam`** - not `null` or `false` to indicate no more pages
7. **Wrap `select` callbacks in `useCallback`** - prevents unnecessary re-renders
8. **Don't destructure mutation results with `...rest`** - breaks reactivity
