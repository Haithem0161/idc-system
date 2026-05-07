# shadcn/ui Guide

Enterprise-grade patterns for using shadcn/ui component library with React + Vite + Tailwind CSS.

## Installation

```bash
# Initialize shadcn in your Vite project
pnpm dlx shadcn@latest init

# Follow prompts:
# - Style: New York (recommended)
# - Base color: Neutral/Slate/Zinc
# - CSS variables: Yes
# - Tailwind CSS config location
# - Components location: src/components
# - Utils location: src/lib/utils
```

## Configuration (components.json)

```json
{
  "style": "new-york",
  "rsc": false,
  "tsx": true,
  "tailwind": {
    "config": "tailwind.config.js",
    "css": "src/index.css",
    "baseColor": "neutral",
    "cssVariables": true
  },
  "aliases": {
    "components": "@/components",
    "utils": "@/lib/utils",
    "ui": "@/components/ui",
    "lib": "@/lib",
    "hooks": "@/hooks"
  },
  "iconLibrary": "lucide"
}
```

## CLI Commands

```bash
# Add specific component
pnpm dlx shadcn@latest add button

# Add multiple components
pnpm dlx shadcn@latest add button card dialog

# Add all components
pnpm dlx shadcn@latest add --all

# Overwrite existing components
pnpm dlx shadcn@latest add button --overwrite

# Check for updates
pnpm dlx shadcn@latest diff
```

## cn() Utility

The `cn()` function merges Tailwind classes intelligently, resolving conflicts.

```typescript
// src/lib/utils.ts
import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
```

**Usage:**

```tsx
import { cn } from '@/lib/utils'

// Conditional classes
<div className={cn('base-class', isActive && 'active-class')} />

// Override classes safely
<Button className={cn('custom-padding', className)} />

// Multiple conditions
<span className={cn(
  'text-sm',
  variant === 'destructive' && 'text-red-500',
  disabled && 'opacity-50 cursor-not-allowed'
)} />
```

## Component Variants (CVA)

Use `class-variance-authority` for type-safe component variants.

```typescript
import { cva, type VariantProps } from 'class-variance-authority'

const buttonVariants = cva(
  // Base styles
  'inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground hover:bg-primary/90',
        destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
        outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
        secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
        ghost: 'hover:bg-accent hover:text-accent-foreground',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-10 px-4 py-2',
        sm: 'h-9 rounded-md px-3',
        lg: 'h-11 rounded-md px-8',
        icon: 'h-10 w-10',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
)

interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}
```

## CSS Variables Theming

Define theme colors in your CSS using HSL values:

```css
/* src/index.css */
@layer base {
  :root {
    --background: 0 0% 100%;
    --foreground: 222.2 84% 4.9%;
    --card: 0 0% 100%;
    --card-foreground: 222.2 84% 4.9%;
    --popover: 0 0% 100%;
    --popover-foreground: 222.2 84% 4.9%;
    --primary: 222.2 47.4% 11.2%;
    --primary-foreground: 210 40% 98%;
    --secondary: 210 40% 96.1%;
    --secondary-foreground: 222.2 47.4% 11.2%;
    --muted: 210 40% 96.1%;
    --muted-foreground: 215.4 16.3% 46.9%;
    --accent: 210 40% 96.1%;
    --accent-foreground: 222.2 47.4% 11.2%;
    --destructive: 0 84.2% 60.2%;
    --destructive-foreground: 210 40% 98%;
    --border: 214.3 31.8% 91.4%;
    --input: 214.3 31.8% 91.4%;
    --ring: 222.2 84% 4.9%;
    --radius: 0.5rem;
  }

  .dark {
    --background: 222.2 84% 4.9%;
    --foreground: 210 40% 98%;
    --card: 222.2 84% 4.9%;
    --card-foreground: 210 40% 98%;
    --popover: 222.2 84% 4.9%;
    --popover-foreground: 210 40% 98%;
    --primary: 210 40% 98%;
    --primary-foreground: 222.2 47.4% 11.2%;
    --secondary: 217.2 32.6% 17.5%;
    --secondary-foreground: 210 40% 98%;
    --muted: 217.2 32.6% 17.5%;
    --muted-foreground: 215 20.2% 65.1%;
    --accent: 217.2 32.6% 17.5%;
    --accent-foreground: 210 40% 98%;
    --destructive: 0 62.8% 30.6%;
    --destructive-foreground: 210 40% 98%;
    --border: 217.2 32.6% 17.5%;
    --input: 217.2 32.6% 17.5%;
    --ring: 212.7 26.8% 83.9%;
  }
}
```

## Dark Mode (ThemeProvider)

```tsx
// src/components/theme-provider.tsx
import { createContext, useContext, useEffect, useState } from 'react'

type Theme = 'dark' | 'light' | 'system'

type ThemeProviderProps = {
  children: React.ReactNode
  defaultTheme?: Theme
  storageKey?: string
}

type ThemeProviderState = {
  theme: Theme
  setTheme: (theme: Theme) => void
}

const ThemeProviderContext = createContext<ThemeProviderState | undefined>(undefined)

export function ThemeProvider({
  children,
  defaultTheme = 'system',
  storageKey = 'vite-ui-theme',
}: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(
    () => (localStorage.getItem(storageKey) as Theme) || defaultTheme
  )

  useEffect(() => {
    const root = window.document.documentElement
    root.classList.remove('light', 'dark')

    if (theme === 'system') {
      const systemTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light'
      root.classList.add(systemTheme)
      return
    }

    root.classList.add(theme)
  }, [theme])

  const value = {
    theme,
    setTheme: (theme: Theme) => {
      localStorage.setItem(storageKey, theme)
      setTheme(theme)
    },
  }

  return (
    <ThemeProviderContext.Provider value={value}>
      {children}
    </ThemeProviderContext.Provider>
  )
}

export const useTheme = () => {
  const context = useContext(ThemeProviderContext)
  if (!context) throw new Error('useTheme must be used within ThemeProvider')
  return context
}
```

**Theme Toggle Component:**

```tsx
// src/components/mode-toggle.tsx
import { Moon, Sun } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useTheme } from '@/components/theme-provider'

export function ModeToggle() {
  const { setTheme } = useTheme()

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon">
          <Sun className="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
          <Moon className="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
          <span className="sr-only">Toggle theme</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => setTheme('light')}>Light</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme('dark')}>Dark</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme('system')}>System</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
```

**App Setup:**

```tsx
// src/App.tsx
import { ThemeProvider } from '@/components/theme-provider'

function App() {
  return (
    <ThemeProvider defaultTheme="system" storageKey="app-theme">
      {/* Your app content */}
    </ThemeProvider>
  )
}
```

## Core Components

### Commonly Used

| Component | Dependencies | Usage |
|-----------|--------------|-------|
| `button` | @radix-ui/react-slot | Actions, CTAs |
| `card` | - | Content containers |
| `dialog` | @radix-ui/react-dialog | Modals, confirmations |
| `dropdown-menu` | @radix-ui/react-dropdown-menu | Menus, actions |
| `input` | - | Text input fields |
| `label` | @radix-ui/react-label | Form labels |
| `select` | @radix-ui/react-select | Dropdown selection |
| `form` | react-hook-form, zod | Form validation |
| `table` | - | Data tables |
| `tabs` | @radix-ui/react-tabs | Tabbed interfaces |
| `toast`/`sonner` | sonner | Notifications |
| `tooltip` | @radix-ui/react-tooltip | Hover hints |
| `sheet` | @radix-ui/react-dialog | Side panels |
| `skeleton` | - | Loading states |
| `avatar` | @radix-ui/react-avatar | User images |
| `badge` | - | Status indicators |

### Installation Example

```bash
# Common component set for a typical app
pnpm dlx shadcn@latest add button card dialog dropdown-menu input label form table tabs sonner tooltip sheet skeleton avatar badge
```

## File Organization

```
src/
├── components/
│   ├── ui/                 # shadcn components (auto-generated)
│   │   ├── button.tsx
│   │   ├── card.tsx
│   │   ├── dialog.tsx
│   │   └── ...
│   ├── theme-provider.tsx  # Theme context
│   ├── mode-toggle.tsx     # Dark mode toggle
│   └── ...                 # Custom components
├── lib/
│   └── utils.ts            # cn() utility
├── hooks/
│   └── ...                 # Custom hooks
└── ...
```

## Component Composition

### Extending Components

```tsx
// Custom button with loading state
import { Button, type ButtonProps } from '@/components/ui/button'
import { Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'

interface LoadingButtonProps extends ButtonProps {
  loading?: boolean
}

export function LoadingButton({
  loading,
  children,
  disabled,
  className,
  ...props
}: LoadingButtonProps) {
  return (
    <Button
      disabled={loading || disabled}
      className={cn(className)}
      {...props}
    >
      {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
      {children}
    </Button>
  )
}
```

### Compound Components

```tsx
// Page header with actions
import { cn } from '@/lib/utils'

interface PageHeaderProps {
  children: React.ReactNode
  className?: string
}

export function PageHeader({ children, className }: PageHeaderProps) {
  return (
    <div className={cn('flex items-center justify-between pb-4', className)}>
      {children}
    </div>
  )
}

PageHeader.Title = function PageHeaderTitle({
  children
}: { children: React.ReactNode }) {
  return <h1 className="text-2xl font-bold">{children}</h1>
}

PageHeader.Actions = function PageHeaderActions({
  children
}: { children: React.ReactNode }) {
  return <div className="flex items-center gap-2">{children}</div>
}

// Usage
<PageHeader>
  <PageHeader.Title>Dashboard</PageHeader.Title>
  <PageHeader.Actions>
    <Button>New Item</Button>
  </PageHeader.Actions>
</PageHeader>
```

## Forms with React Hook Form + Zod

```tsx
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'

const formSchema = z.object({
  email: z.string().email('Invalid email address'),
  password: z.string().min(8, 'Password must be at least 8 characters'),
})

type FormData = z.infer<typeof formSchema>

export function LoginForm() {
  const form = useForm<FormData>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      email: '',
      password: '',
    },
  })

  function onSubmit(data: FormData) {
    console.log(data)
  }

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
        <FormField
          control={form.control}
          name="email"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Email</FormLabel>
              <FormControl>
                <Input placeholder="email@example.com" {...field} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
        <FormField
          control={form.control}
          name="password"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Password</FormLabel>
              <FormControl>
                <Input type="password" {...field} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
        <Button type="submit">Sign In</Button>
      </form>
    </Form>
  )
}
```

## Best Practices

1. **Don't modify ui/ components directly** - Create wrapper components instead
2. **Use CSS variables** - Enables easy theming and dark mode
3. **Leverage cn()** - Always use for conditional/merged classes
4. **Type your variants** - Use VariantProps for type-safe props
5. **Composition over modification** - Wrap and extend, don't edit source
6. **Consistent spacing** - Use Tailwind spacing scale (gap-4, p-6, etc.)
7. **Accessible by default** - shadcn uses Radix UI primitives with built-in a11y
