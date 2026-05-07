# Framer Motion Guide

## Installation

```bash
pnpm add motion
```

## Basic Animation

```tsx
import { motion } from "motion/react"

<motion.div
  initial={{ opacity: 0, y: 20 }}
  animate={{ opacity: 1, y: 0 }}
  transition={{ duration: 0.5 }}
/>
```

## Transition Types

### Spring (Default for physical properties)

```tsx
// Physics-based (recommended)
<motion.div
  animate={{ x: 100 }}
  transition={{ type: "spring", stiffness: 400, damping: 25 }}
/>

// Duration-based spring
<motion.div
  animate={{ scale: 1.2 }}
  transition={{ type: "spring", duration: 0.8, bounce: 0.3 }}
/>
```

**Spring parameters:**
- `stiffness` - Higher = snappier (default: 100)
- `damping` - Higher = less oscillation (default: 10)
- `mass` - Higher = slower, heavier feel (default: 1)
- `bounce` - 0 to 1, overrides stiffness/damping

### Tween (Default for opacity, colors)

```tsx
<motion.div
  animate={{ opacity: 1 }}
  transition={{
    type: "tween",
    duration: 0.3,
    ease: "easeOut" // or [0.42, 0, 0.58, 1] cubic-bezier
  }}
/>
```

**Common easings:** `"linear"`, `"easeIn"`, `"easeOut"`, `"easeInOut"`, `"circOut"`, `"backOut"`

### Per-Property Transitions

```tsx
<motion.div
  animate={{ x: 100, opacity: 1 }}
  transition={{
    default: { type: "spring", stiffness: 300 },
    opacity: { duration: 0.2, ease: "linear" }
  }}
/>
```

## Variants

Orchestrate animations across component trees:

```tsx
const container = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: {
      when: "beforeChildren",
      staggerChildren: 0.1,
      delayChildren: 0.2,
    }
  }
}

const item = {
  hidden: { opacity: 0, y: 20 },
  visible: { opacity: 1, y: 0 }
}

<motion.ul variants={container} initial="hidden" animate="visible">
  <motion.li variants={item} />
  <motion.li variants={item} />
  <motion.li variants={item} />
</motion.ul>
```

**Orchestration options:**
- `when: "beforeChildren" | "afterChildren"` - Sequence parent/child animations
- `staggerChildren: number` - Delay between each child
- `delayChildren: number` - Delay before children start
- `staggerDirection: 1 | -1` - Stagger direction

## Exit Animations (AnimatePresence)

```tsx
import { AnimatePresence, motion } from "motion/react"

<AnimatePresence mode="wait">
  {isVisible && (
    <motion.div
      key="modal"
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.9 }}
    />
  )}
</AnimatePresence>
```

**AnimatePresence props:**
- `mode="wait"` - Wait for exiting element before entering new one
- `mode="popLayout"` - Remove exiting elements from layout flow
- `onExitComplete` - Callback when all exit animations complete

## Layout Animations

```tsx
// Animate layout changes automatically
<motion.div layout>
  {isExpanded && <p>Extra content</p>}
</motion.div>

// Shared element transitions
{items.map(item => (
  <motion.div key={item.id} layoutId={item.id}>
    {item.isSelected && <motion.div layoutId="highlight" />}
  </motion.div>
))}
```

**Layout props:**
- `layout` - Animate position/size changes
- `layout="position"` - Only animate position
- `layout="size"` - Only animate size
- `layoutId` - Shared element transitions across components

## Gesture Animations

```tsx
<motion.button
  whileHover={{ scale: 1.05 }}
  whileTap={{ scale: 0.95 }}
  whileFocus={{ outline: "2px solid blue" }}
/>
```

### Drag

```tsx
<motion.div
  drag
  dragConstraints={{ left: 0, right: 300, top: 0, bottom: 300 }}
  dragElastic={0.2}
  dragMomentum={false}
  whileDrag={{ scale: 1.1 }}
/>

// Drag with ref constraints
const constraintsRef = useRef(null)

<div ref={constraintsRef}>
  <motion.div drag dragConstraints={constraintsRef} />
</div>
```

### Programmatic Drag Controls

```tsx
import { useDragControls, motion } from "motion/react"

const dragControls = useDragControls()

<div onPointerDown={(e) => dragControls.start(e)}>Drag handle</div>
<motion.div drag="x" dragControls={dragControls} />
```

## Scroll Animations

### Scroll-Triggered (whileInView)

```tsx
<motion.div
  initial={{ opacity: 0, y: 50 }}
  whileInView={{ opacity: 1, y: 0 }}
  viewport={{ once: true, margin: "-100px" }}
  transition={{ duration: 0.6 }}
/>
```

**Viewport options:**
- `once: true` - Only animate once
- `margin` - Offset trigger point (like rootMargin)
- `amount: "some" | "all" | number` - How much must be visible

### Scroll-Linked (useScroll)

```tsx
import { useScroll, useTransform, motion } from "motion/react"

function ProgressBar() {
  const { scrollYProgress } = useScroll()

  return <motion.div style={{ scaleX: scrollYProgress }} />
}

// Element-specific scroll tracking
function ParallaxImage() {
  const ref = useRef(null)
  const { scrollYProgress } = useScroll({
    target: ref,
    offset: ["start end", "end start"]
  })
  const y = useTransform(scrollYProgress, [0, 1], [100, -100])

  return <motion.img ref={ref} style={{ y }} />
}
```

## Motion Values

```tsx
import { useMotionValue, useTransform, useSpring } from "motion/react"

function Component() {
  const x = useMotionValue(0)

  // Transform: map x range to opacity range
  const opacity = useTransform(x, [-200, 0, 200], [0, 1, 0])

  // Spring: smooth following
  const smoothX = useSpring(x, { stiffness: 300, damping: 30 })

  // Derived calculation
  const rotate = useTransform(() => x.get() * 0.5)

  return <motion.div style={{ x, opacity }} drag="x" />
}
```

## Programmatic Animation (useAnimate)

```tsx
import { useAnimate, stagger } from "motion/react"

function Component() {
  const [scope, animate] = useAnimate()

  async function handleClick() {
    // Animate single element
    await animate(scope.current, { scale: 1.2 }, { duration: 0.2 })

    // Animate children with stagger
    await animate("li", { opacity: 1, x: 0 }, { delay: stagger(0.1) })
  }

  // Sequence animations
  async function sequence() {
    const controls = animate([
      [scope.current, { opacity: 1 }],
      ["h1", { y: 0 }, { at: "-0.3" }], // Start 0.3s before previous ends
      ["p", { opacity: 1 }, { at: "<" }], // Start with previous
    ])

    controls.speed = 0.5
    // controls.pause(), controls.play(), controls.stop()
  }

  return <div ref={scope}>...</div>
}
```

## Performance Optimization

### Hardware-Accelerated Properties

Prefer these (GPU-accelerated):
- `opacity`
- `transform` (x, y, scale, rotate)
- `filter`
- `clipPath`

Avoid animating (trigger layout/paint):
- `width`, `height`
- `top`, `left`, `right`, `bottom`
- `margin`, `padding`
- `borderRadius` (use `will-change` if needed)

### Animate Transform Directly

```tsx
// Best for complex transforms
<motion.div
  initial={{ transform: "translateX(-100px) scale(0.8)" }}
  animate={{ transform: "translateX(0px) scale(1)" }}
/>
```

### Use will-change Sparingly

```tsx
// Only for properties that need layer promotion
<motion.div
  style={{ willChange: "transform" }}
  animate={{ borderRadius: "50%" }}
/>
```

## Bundle Size Optimization

### LazyMotion + m component

```tsx
// features.ts
import { domAnimation } from "motion/react"
export default domAnimation
// Or domMax for all features including layout animations

// App.tsx
import { LazyMotion } from "motion/react"
import * as m from "motion/react-m"

const loadFeatures = () => import("./features").then(mod => mod.default)

function App() {
  return (
    <LazyMotion features={loadFeatures} strict>
      <m.div animate={{ opacity: 1 }} />
    </LazyMotion>
  )
}
```

**Feature bundles:**
- `domAnimation` (~16kb) - Basic animations, variants, gestures
- `domMax` (~27kb) - + Layout animations, drag, viewport detection

## Global Configuration (MotionConfig)

```tsx
import { MotionConfig } from "motion/react"

<MotionConfig reducedMotion="user" transition={{ duration: 0.3 }}>
  <App />
</MotionConfig>
```

**reducedMotion options:**
- `"user"` - Respect prefers-reduced-motion (default)
- `"always"` - Force reduced motion
- `"never"` - Ignore preference

## File Organization

```
src/
├── components/
│   └── animations/
│       ├── FadeIn.tsx
│       ├── SlideIn.tsx
│       └── variants.ts      # Shared variant definitions
├── lib/
│   └── motion-features.ts   # LazyMotion features export
└── hooks/
    └── useScrollAnimation.ts
```

## Key Rules

1. **Use spring for physical motion** - More natural than duration-based
2. **Prefer transform properties** - GPU-accelerated, 60fps
3. **Use variants for orchestration** - Cleaner than inline, enables staggering
4. **Always add `key` in AnimatePresence** - Required for exit animations
5. **Use `layout` sparingly** - Expensive, only when needed
6. **LazyMotion in production** - Reduce bundle size significantly
7. **Respect reduced motion** - Use MotionConfig with `reducedMotion="user"`
8. **Avoid layout-triggering animations** - No width/height, use transform
