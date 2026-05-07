# Zod Guide

Enterprise-grade patterns for schema validation with Zod 4 (TypeScript-first validation library).

## Installation

```bash
pnpm add zod
```

## Basic Usage

```typescript
import { z } from 'zod'

// Define a schema
const userSchema = z.object({
  name: z.string(),
  email: z.email(),
  age: z.number().min(0),
})

// Infer TypeScript type from schema
type User = z.infer<typeof userSchema>

// Parse data (throws on error)
const user = userSchema.parse({
  name: 'John',
  email: 'john@example.com',
  age: 25,
})

// Safe parse (returns result object)
const result = userSchema.safeParse(data)
if (result.success) {
  console.log(result.data) // typed as User
} else {
  console.log(result.error.issues)
}
```

## Primitive Types

```typescript
// Strings
z.string()
z.string().min(1)                    // Min length
z.string().max(100)                  // Max length
z.string().length(5)                 // Exact length
z.string().trim()                    // Trim whitespace
z.string().toLowerCase()             // Transform to lowercase
z.string().toUpperCase()             // Transform to uppercase
z.string().regex(/^[a-z]+$/)         // Regex pattern
z.string().startsWith('https://')    // Must start with
z.string().endsWith('.com')          // Must end with
z.string().includes('@')             // Must include

// Numbers
z.number()
z.number().min(0)                    // >= 0
z.number().max(100)                  // <= 100
z.number().int()                     // Integer only
z.number().positive()                // > 0
z.number().negative()                // < 0
z.number().nonnegative()             // >= 0
z.number().nonpositive()             // <= 0
z.number().finite()                  // Not Infinity
z.number().multipleOf(5)             // Divisible by 5

// BigInt
z.bigint()
z.bigint().min(0n)
z.bigint().max(100n)

// Booleans
z.boolean()

// Dates
z.date()
z.date().min(new Date('2020-01-01'))
z.date().max(new Date())

// Other primitives
z.undefined()
z.null()
z.void()
z.any()
z.unknown()
z.never()
```

## String Format Validators (Zod 4)

Zod 4 uses top-level functions for string formats (better tree-shaking):

```typescript
// Email, URL, UUID
z.email()                            // Email address
z.url()                              // URL
z.uuid()                             // RFC 9562/4122 UUID
z.guid()                             // Any 8-4-4-4-12 hex pattern

// IDs
z.nanoid()
z.cuid()
z.cuid2()
z.ulid()

// Encoding
z.base64()
z.base64url()

// Network
z.ipv4()                             // IPv4 address
z.ipv6()                             // IPv6 address
z.cidrv4()                           // IPv4 CIDR range
z.cidrv6()                           // IPv6 CIDR range

// ISO formats
z.iso.date()                         // ISO date (YYYY-MM-DD)
z.iso.time()                         // ISO time (HH:MM:SS)
z.iso.datetime()                     // ISO datetime
z.iso.duration()                     // ISO duration

// Emoji
z.emoji()                            // Single emoji character

// Deprecated (still works but prefer top-level):
// z.string().email() → z.email()
// z.string().url() → z.url()
// z.string().uuid() → z.uuid()
```

## Objects

```typescript
// Basic object
const userSchema = z.object({
  name: z.string(),
  email: z.email(),
})

// Optional fields
const userSchema = z.object({
  name: z.string(),
  nickname: z.string().optional(),   // string | undefined
})

// Nullable fields
const userSchema = z.object({
  name: z.string(),
  avatar: z.string().nullable(),     // string | null
})

// Default values
const userSchema = z.object({
  role: z.string().default('user'),  // Applies if undefined
  theme: z.enum(['light', 'dark']).default('light'),
})

// Strict objects (error on unknown keys)
z.strictObject({
  name: z.string(),
})

// Loose objects (allow unknown keys)
z.looseObject({
  name: z.string(),
})

// Object manipulation
const baseUser = z.object({
  id: z.string(),
  name: z.string(),
  email: z.email(),
  role: z.string(),
})

// Pick specific fields
const nameOnly = baseUser.pick({ name: true })

// Omit fields
const withoutId = baseUser.omit({ id: true })

// Make all fields optional
const partialUser = baseUser.partial()

// Make specific fields optional
const optionalEmail = baseUser.partial({ email: true })

// Make all fields required
const requiredUser = partialUser.required()

// Extend with new fields
const adminUser = baseUser.extend({
  permissions: z.array(z.string()),
})
```

## Arrays and Tuples

```typescript
// Arrays
z.array(z.string())                  // string[]
z.array(z.number()).min(1)           // At least 1 element
z.array(z.number()).max(10)          // At most 10 elements
z.array(z.number()).length(5)        // Exactly 5 elements
z.array(z.number()).nonempty()       // At least 1 element (type narrowing)

// Tuples (fixed-length arrays with specific types)
z.tuple([z.string(), z.number()])    // [string, number]
z.tuple([z.string(), z.number()]).rest(z.boolean())  // [string, number, ...boolean[]]

// Sets
z.set(z.string())                    // Set<string>
z.set(z.string()).min(1).max(10)
```

## Records and Maps

```typescript
// Record (object with dynamic keys)
z.record(z.string())                 // { [key: string]: string }
z.record(z.string(), z.number())     // { [key: string]: number }

// Record with enum keys (all keys required in Zod 4)
z.record(z.enum(['a', 'b', 'c']), z.number())
// Type: { a: number; b: number; c: number }

// Partial record (optional keys)
z.partialRecord(z.enum(['a', 'b', 'c']), z.number())
// Type: { a?: number; b?: number; c?: number }

// Maps
z.map(z.string(), z.number())        // Map<string, number>
```

## Enums and Literals

```typescript
// Zod enums
z.enum(['pending', 'active', 'archived'])
// Extracts: 'pending' | 'active' | 'archived'

// Native TypeScript enums
enum Status {
  Pending = 'PENDING',
  Active = 'ACTIVE',
}
z.nativeEnum(Status)

// Literals
z.literal('active')                  // Exactly 'active'
z.literal(42)                        // Exactly 42
z.literal(true)                      // Exactly true

// Multiple literals (Zod 4)
z.literal([200, 201, 204])           // 200 | 201 | 204
// Replaces: z.union([z.literal(200), z.literal(201), z.literal(204)])
```

## Unions and Discriminated Unions

```typescript
// Union (try each schema until one succeeds)
const stringOrNumber = z.union([z.string(), z.number()])
// or shorthand:
const stringOrNumber = z.string().or(z.number())

// Discriminated union (more efficient for objects)
const resultSchema = z.discriminatedUnion('status', [
  z.object({ status: z.literal('success'), data: z.string() }),
  z.object({ status: z.literal('error'), message: z.string() }),
])

// Advanced discriminated union (Zod 4)
const advancedResult = z.discriminatedUnion('status', [
  z.object({ status: z.literal('success'), data: z.string() }),
  // Union discriminator
  z.object({ status: z.union([z.literal('pending'), z.literal('processing')]) }),
  // Pipe discriminator
  z.object({ status: z.literal('failed').transform(v => v.toUpperCase()) }),
])

// Intersection
const nameAndAge = z.intersection(
  z.object({ name: z.string() }),
  z.object({ age: z.number() })
)
// or shorthand:
const nameAndAge = z.object({ name: z.string() }).and(z.object({ age: z.number() }))
```

## Type Inference

```typescript
// Infer input type (before transforms)
type UserInput = z.input<typeof userSchema>

// Infer output type (after transforms)
type User = z.output<typeof userSchema>

// Shorthand for output type
type User = z.infer<typeof userSchema>
```

## Transforms

```typescript
// Transform values
const trimmedString = z.string().transform(s => s.trim())

const userSchema = z.object({
  name: z.string().transform(s => s.toLowerCase()),
  createdAt: z.string().transform(s => new Date(s)),
})

// Coercion (convert input types)
z.coerce.string()                    // Converts to string
z.coerce.number()                    // Converts to number
z.coerce.boolean()                   // Falsy → false, truthy → true
z.coerce.date()                      // Converts to Date

// String to boolean (env-style, Zod 4)
z.stringbool()
// 'true', '1', 'yes', 'on', 'y', 'enabled' → true
// 'false', '0', 'no', 'off', 'n', 'disabled' → false

// Custom truthy/falsy values
z.stringbool({
  truthy: ['yes', 'true', '1'],
  falsy: ['no', 'false', '0'],
})
```

## Custom Validation

```typescript
// Simple refinement
const positiveNumber = z.number().refine(n => n > 0, {
  error: 'Must be positive',
})

// Refinement with dynamic error
const passwordSchema = z.string().refine(
  val => val.length >= 8,
  { error: 'Password must be at least 8 characters' }
)

// SuperRefine for complex validation
const passwordSchema = z.string().superRefine((val, ctx) => {
  if (val.length < 8) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Password must be at least 8 characters',
    })
  }
  if (!/[A-Z]/.test(val)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Password must contain an uppercase letter',
    })
  }
  if (!/[0-9]/.test(val)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Password must contain a number',
    })
  }
})

// Cross-field validation
const signupSchema = z.object({
  password: z.string().min(8),
  confirmPassword: z.string(),
}).refine(data => data.password === data.confirmPassword, {
  error: 'Passwords do not match',
  path: ['confirmPassword'],  // Attach error to this field
})
```

## Error Handling

```typescript
// Zod 4 unified error parameter
z.string().min(5, { error: 'Too short' })

// Dynamic error based on issue
z.string({
  error: (issue) => {
    if (issue.input === undefined) return 'This field is required'
    return 'Must be a string'
  }
})

// Error handling with safeParse
const result = schema.safeParse(data)
if (!result.success) {
  // Access all issues
  console.log(result.error.issues)

  // Formatted errors
  console.log(result.error.format())

  // Flatten to simple object
  console.log(result.error.flatten())
}

// Try-catch with parse
try {
  const data = schema.parse(input)
} catch (error) {
  if (error instanceof z.ZodError) {
    console.log(error.issues)
  }
}
```

## Integration with React Hook Form

```typescript
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'

const loginSchema = z.object({
  email: z.email({ error: 'Invalid email address' }),
  password: z.string().min(8, { error: 'Password must be at least 8 characters' }),
})

type LoginForm = z.infer<typeof loginSchema>

function LoginForm() {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginForm>({
    resolver: zodResolver(loginSchema),
  })

  const onSubmit = (data: LoginForm) => {
    console.log(data)
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <input {...register('email')} />
      {errors.email && <span>{errors.email.message}</span>}

      <input type="password" {...register('password')} />
      {errors.password && <span>{errors.password.message}</span>}

      <button type="submit">Login</button>
    </form>
  )
}
```

## Common Patterns

### API Response Validation

```typescript
const apiResponseSchema = z.object({
  data: z.array(z.object({
    id: z.string(),
    name: z.string(),
    createdAt: z.string().transform(s => new Date(s)),
  })),
  meta: z.object({
    total: z.number(),
    page: z.number(),
    pageSize: z.number(),
  }),
})

type ApiResponse = z.infer<typeof apiResponseSchema>

async function fetchUsers(): Promise<ApiResponse> {
  const res = await fetch('/api/users')
  const json = await res.json()
  return apiResponseSchema.parse(json)
}
```

### Environment Variables

```typescript
const envSchema = z.object({
  NODE_ENV: z.enum(['development', 'production', 'test']),
  DATABASE_URL: z.string().url(),
  API_KEY: z.string().min(1),
  PORT: z.coerce.number().default(3000),
  DEBUG: z.stringbool().default(false),
})

export const env = envSchema.parse(process.env)
```

### Form Schema Factory

```typescript
// Reusable field schemas
const fields = {
  email: z.email({ error: 'Invalid email' }),
  password: z.string().min(8, { error: 'Min 8 characters' }),
  name: z.string().min(1, { error: 'Name is required' }),
  phone: z.string().regex(/^\+?[1-9]\d{1,14}$/, { error: 'Invalid phone' }).optional(),
}

// Compose into form schemas
const loginSchema = z.object({
  email: fields.email,
  password: fields.password,
})

const signupSchema = z.object({
  name: fields.name,
  email: fields.email,
  password: fields.password,
  phone: fields.phone,
})

const profileSchema = z.object({
  name: fields.name,
  phone: fields.phone,
})
```

### Nullable vs Optional vs Nullish

```typescript
z.string().optional()    // string | undefined
z.string().nullable()    // string | null
z.string().nullish()     // string | null | undefined

// With defaults
z.string().optional().default('default')  // Always returns string
z.string().nullable().default('default')  // Returns string (null → 'default')
```

### Branded Types

```typescript
const UserId = z.string().uuid().brand<'UserId'>()
type UserId = z.infer<typeof UserId>

const PostId = z.string().uuid().brand<'PostId'>()
type PostId = z.infer<typeof PostId>

// Now TypeScript distinguishes between them
function getUser(id: UserId) { /* ... */ }
function getPost(id: PostId) { /* ... */ }

const userId = UserId.parse('123e4567-e89b-12d3-a456-426614174000')
const postId = PostId.parse('123e4567-e89b-12d3-a456-426614174001')

getUser(userId)  // ✅
getUser(postId)  // ❌ TypeScript error
```

## Best Practices

1. **Define schemas at module level** - Avoid recreating schemas on every render
2. **Use `safeParse` in production** - Gracefully handle validation errors
3. **Leverage type inference** - Use `z.infer<>` instead of manual types
4. **Compose schemas** - Build complex schemas from simple, reusable parts
5. **Use top-level validators** - Prefer `z.email()` over `z.string().email()`
6. **Add meaningful error messages** - Use the `error` parameter
7. **Validate at boundaries** - API responses, form inputs, env vars
8. **Use branded types** - For type-safe IDs and special strings
