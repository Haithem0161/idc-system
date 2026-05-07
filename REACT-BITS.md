# React Bits Guide

Enterprise-grade patterns for using React Bits animated components with React + Tailwind CSS.

React Bits is a collection of high-quality, animated, interactive React components for building stunning user interfaces.

## Installation

Components are installed individually via the shadcn CLI using the React Bits registry URL.

```bash
# Format: https://reactbits.dev/r/<Component>-<LANG>-<STYLE>
# LANG: TS (TypeScript) or JS (JavaScript)
# STYLE: TW (Tailwind CSS) or CSS (vanilla CSS)

# Example: Install BlurText with TypeScript + Tailwind
pnpm dlx shadcn@latest add https://reactbits.dev/r/BlurText-TS-TW

# Example: Install with JavaScript + CSS
pnpm dlx shadcn@latest add https://reactbits.dev/r/BlurText-JS-CSS
```

**Recommended:** Use `TS-TW` (TypeScript + Tailwind) for this project.

## Component Categories

### Text Animations

| Component | Description | Dependencies |
|-----------|-------------|--------------|
| BlurText | Progressive blur-to-focus reveal | - |
| SplitText | Character/word split animations | gsap |
| GradientText | Animated gradient text | - |
| RotatingText | Text rotation with stagger | motion |
| TextType | Typewriter effect | gsap |
| CircularText | Text arranged on circular path | - |
| ShinyText | Shimmering text effect | - |
| TextReveal | Scroll-triggered text reveal | gsap |

### Backgrounds

| Component | Description | Dependencies |
|-----------|-------------|--------------|
| Aurora | Animated gradient aurora | - |
| Particles | WebGL 3D particle system | ogl |
| Plasma | Organic flowing effects | - |
| Lightning | Lightning bolt effects | - |
| Waves | Animated wave patterns | - |
| Grid | Animated grid background | - |

### Animations

| Component | Description | Dependencies |
|-----------|-------------|--------------|
| AnimatedContent | Scroll-triggered animations | gsap |
| AnimatedList | Staggered list entrance | - |
| FadeIn | Simple fade animations | motion |
| SlideIn | Directional slide animations | motion |

### Interactive

| Component | Description | Dependencies |
|-----------|-------------|--------------|
| Magnet | Magnetic pull effect on hover | - |
| ClickSpark | Particle explosion on click | - |
| TiltCard | 3D tilt on hover | - |
| Spotlight | Cursor-following spotlight | - |
| Dock | macOS-style dock | motion |

---

## Text Animations

### BlurText

Progressive blur-to-focus reveal animation for headlines.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/BlurText-TS-TW
```

```tsx
import BlurText from '@/components/ui/BlurText'

// Basic usage
<BlurText
  text="Welcome to our website"
  delay={150}
  className="text-6xl font-black"
/>

// Advanced with callbacks
<BlurText
  text="Transform your experience"
  delay={100}
  animateBy="characters"        // "words" | "characters"
  direction="top"               // "top" | "bottom"
  threshold={0.3}               // Intersection threshold
  stepDuration={0.4}
  animationFrom={{ filter: 'blur(20px)', opacity: 0, y: 100 }}
  animationTo={{ filter: 'blur(0px)', opacity: 1, y: 0 }}
  onAnimationComplete={() => console.log('Done!')}
  className="text-4xl"
/>
```

**Props:**
- `text` (string) - Text to animate
- `delay` (number) - Delay between each word/character (ms)
- `animateBy` ("words" | "characters") - Animation unit
- `direction` ("top" | "bottom") - Animation direction
- `threshold` (number) - Viewport intersection threshold (0-1)
- `animationFrom` (object) - Initial animation state
- `animationTo` (object | array) - Target animation state(s)
- `onAnimationComplete` (function) - Callback when complete

### GradientText

Animated gradient text with customizable colors and direction.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/GradientText-TS-TW
```

```tsx
import GradientText from '@/components/ui/GradientText'

<GradientText
  colors={['#ff0000', '#00ff00', '#0000ff']}
  animationSpeed={5}
  direction="horizontal"      // "horizontal" | "vertical" | "diagonal"
  pauseOnHover={true}
  showBorder={false}
  className="text-5xl font-bold"
>
  Gradient Headline
</GradientText>
```

**Props:**
- `children` (ReactNode) - Text content
- `colors` (string[]) - Gradient colors array
- `animationSpeed` (number) - Animation cycle duration (seconds)
- `direction` ("horizontal" | "vertical" | "diagonal") - Gradient direction
- `pauseOnHover` (boolean) - Pause animation on hover
- `yoyo` (boolean) - Reverse animation at end

### RotatingText

Text rotation with staggered character animations.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/RotatingText-TS-TW
```

```tsx
import RotatingText from '@/components/ui/RotatingText'

<RotatingText
  texts={['Innovative', 'Creative', 'Dynamic']}
  rotationInterval={2000}
  splitBy="characters"        // "characters" | "words"
  staggerDuration={0.05}
  loop={true}
  auto={true}
/>
```

**Props:**
- `texts` (string[]) - Array of texts to rotate
- `rotationInterval` (number) - Time between rotations (ms)
- `splitBy` ("characters" | "words") - How to split text
- `staggerDuration` (number) - Delay between each element
- `loop` (boolean) - Loop back to start
- `auto` (boolean) - Auto-start rotation

### TextType

Typewriter effect with cursor and multi-text support.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/TextType-TS-TW
```

```tsx
import TextType from '@/components/ui/TextType'

<TextType
  text={['Build faster.', 'Ship sooner.', 'Scale effortlessly.']}
  typingSpeed={75}
  deletingSpeed={50}
  pauseDuration={1500}
  loop={true}
  showCursor={true}
  cursorCharacter="|"
  className="text-2xl"
/>
```

**Props:**
- `text` (string | string[]) - Text(s) to type
- `typingSpeed` (number) - Typing speed (ms per character)
- `deletingSpeed` (number) - Deleting speed (ms per character)
- `pauseDuration` (number) - Pause after typing (ms)
- `loop` (boolean) - Loop through texts
- `showCursor` (boolean) - Show blinking cursor
- `cursorCharacter` (ReactNode) - Custom cursor

### CircularText

Arrange text along a circular path with optional rotation.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/CircularText-TS-TW
```

```tsx
import CircularText from '@/components/ui/CircularText'

<CircularText
  text="REACT BITS • REACT BITS • "
  radius={100}
  fontSize={16}
  rotateText={true}
  rotationSpeed={0.5}
  direction="clockwise"       // "clockwise" | "counterclockwise"
  className="text-yellow-400 font-bold"
/>
```

---

## Backgrounds

### Aurora

Animated gradient aurora background effect.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/Aurora-TS-TW
```

```tsx
import Aurora from '@/components/ui/Aurora'

// Full-page aurora background
<div className="relative min-h-screen">
  <Aurora
    colors={['#E0E7FF', '#DBEAFE', '#E0F2FE']}
    speed={0.3}
    blur={100}
    opacity={0.5}
  />
  <main className="relative z-10">
    {/* Content */}
  </main>
</div>
```

**Props:**
- `colors` (string[]) - Aurora colors
- `speed` (number) - Animation speed multiplier
- `blur` (number) - Blur amount (px)
- `opacity` (number) - Background opacity (0-1)

### Particles

WebGL-powered 3D particle system. **Requires `ogl` library.**

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/Particles-TS-TW
pnpm add ogl
```

```tsx
import Particles from '@/components/ui/Particles'

<section className="relative h-screen">
  <Particles
    particleCount={1000}
    particleSize={2}
    color="#ffffff"
    speed={0.5}
    className="particles-bg"
  />
  <div className="relative z-10">
    <h1>Welcome</h1>
  </div>
</section>
```

### Plasma

Organic flowing plasma effect using WebGL.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/Plasma-TS-TW
```

```tsx
import Plasma from '@/components/ui/Plasma'

<Plasma
  color1="#FF0080"
  color2="#7928CA"
  color3="#00DFD8"
  speed={0.5}
  blur={30}
/>
```

### Lightning

Animated lightning bolt background effect.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/Lightning-TS-TW
```

```tsx
import Lightning from '@/components/ui/Lightning'

<Lightning
  hue={230}
  speed={1}
  intensity={1}
  size={1}
/>
```

---

## Interactive Components

### Magnet

Creates a magnetic pull effect when cursor approaches.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/Magnet-TS-TW
```

```tsx
import Magnet from '@/components/ui/Magnet'

// Magnetic button
<Magnet magnitude={0.3} maxDistance={150}>
  <button className="px-6 py-3 bg-primary text-white rounded-lg">
    Hover Me
  </button>
</Magnet>

// Stronger magnetic effect for cards
<Magnet magnitude={0.5} maxDistance={200} damping={20} stiffness={150}>
  <div className="p-6 bg-card rounded-xl">
    Interactive Card
  </div>
</Magnet>
```

**Props:**
- `magnitude` (number) - Pull strength (0-1)
- `maxDistance` (number) - Max distance for effect (px)
- `damping` (number) - Motion damping
- `stiffness` (number) - Motion stiffness

### ClickSpark

Particle explosion effect on click.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/ClickSpark-TS-TW
```

```tsx
import ClickSpark from '@/components/ui/ClickSpark'

// Global click sparks
<>
  <ClickSpark
    colors={['#FF6B6B', '#4ECDC4', '#45B7D1', '#FFA07A']}
    particleCount={25}
    particleSize={8}
    speed={4}
    gravity={0.8}
  />
  <main>Click anywhere!</main>
</>

// Contained within element
<div className="interactive-area relative">
  <ClickSpark
    colors={['#FFD700', '#FFA500']}
    containerSelector=".interactive-area"
  />
  Click inside this box
</div>
```

### TiltCard

3D tilt effect on hover.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/TiltCard-TS-TW
```

```tsx
import TiltCard from '@/components/ui/TiltCard'

<TiltCard
  maxTilt={15}
  scale={1.05}
  perspective={1000}
  className="p-6 bg-card rounded-xl"
>
  <h3>Tilt Card</h3>
  <p>Hover to see 3D effect</p>
</TiltCard>
```

---

## Animations

### AnimatedContent

Scroll-triggered animations with GSAP. **Requires `gsap` library.**

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/AnimatedContent-TS-TW
pnpm add gsap
```

```tsx
import AnimatedContent from '@/components/ui/AnimatedContent'

<AnimatedContent
  distance={100}
  direction="vertical"        // "vertical" | "horizontal"
  reverse={false}
  duration={0.8}
  ease="power3.out"
  initialOpacity={0}
  scale={0.95}
  threshold={0.2}
  delay={0.1}
  onComplete={() => console.log('Animated!')}
>
  <div className="p-8 bg-card rounded-xl">
    This content animates on scroll
  </div>
</AnimatedContent>
```

### AnimatedList

Staggered entrance animation for list items.

```bash
pnpm dlx shadcn@latest add https://reactbits.dev/r/AnimatedList-TS-TW
```

```tsx
import AnimatedList from '@/components/ui/AnimatedList'

<AnimatedList stagger={0.1} duration={0.4} className="space-y-4">
  {items.map((item) => (
    <div key={item.id} className="p-4 bg-card rounded-lg">
      {item.title}
    </div>
  ))}
</AnimatedList>
```

---

## Dependencies

Some components require additional libraries:

```bash
# For GSAP-based animations (TextType, AnimatedContent, SplitText, TextReveal)
pnpm add gsap

# For WebGL backgrounds (Particles)
pnpm add ogl

# For motion-based components (RotatingText, Dock, FadeIn, SlideIn)
pnpm add motion
```

## Performance Best Practices

### 1. Lazy Load Heavy Components

```tsx
import { lazy, Suspense } from 'react'

const Particles = lazy(() => import('@/components/ui/Particles'))

function HeroSection() {
  return (
    <Suspense fallback={<div className="bg-background" />}>
      <Particles />
    </Suspense>
  )
}
```

### 2. Conditional Rendering

```tsx
// Only render on larger screens
const [showParticles, setShowParticles] = useState(false)

useEffect(() => {
  setShowParticles(window.innerWidth > 768)
}, [])

{showParticles && <Particles />}
```

### 3. Reduce Motion for Accessibility

```tsx
// Check user preference
const prefersReducedMotion = window.matchMedia(
  '(prefers-reduced-motion: reduce)'
).matches

<BlurText
  text="Welcome"
  delay={prefersReducedMotion ? 0 : 150}
  animateBy={prefersReducedMotion ? 'words' : 'characters'}
/>
```

### 4. Cleanup WebGL Components

```tsx
// WebGL components (Particles, Plasma) clean up automatically on unmount
// But ensure they're conditionally rendered when navigating away
{isVisible && <Particles />}
```

## File Organization

```
src/
├── components/
│   ├── ui/
│   │   ├── BlurText.tsx      # React Bits components
│   │   ├── Aurora.tsx
│   │   ├── Particles.tsx
│   │   ├── Magnet.tsx
│   │   ├── button.tsx        # shadcn components
│   │   └── ...
│   └── ...
└── ...
```

## Common Patterns

### Hero Section with Multiple Effects

```tsx
import Aurora from '@/components/ui/Aurora'
import BlurText from '@/components/ui/BlurText'
import Magnet from '@/components/ui/Magnet'
import { Button } from '@/components/ui/button'

export function HeroSection() {
  return (
    <section className="relative min-h-screen flex items-center justify-center">
      <Aurora
        colors={['#E0E7FF', '#DBEAFE', '#F0FDFA']}
        speed={0.2}
        opacity={0.4}
      />

      <div className="relative z-10 text-center space-y-8">
        <BlurText
          text="Build Something Amazing"
          delay={100}
          animateBy="words"
          className="text-6xl font-black"
        />

        <p className="text-xl text-muted-foreground max-w-2xl mx-auto">
          Create stunning user experiences with animated components
        </p>

        <Magnet magnitude={0.2}>
          <Button size="lg">Get Started</Button>
        </Magnet>
      </div>
    </section>
  )
}
```

### Feature Cards with Tilt

```tsx
import TiltCard from '@/components/ui/TiltCard'
import AnimatedList from '@/components/ui/AnimatedList'

export function Features() {
  const features = [
    { title: 'Fast', description: 'Optimized performance' },
    { title: 'Accessible', description: 'Built with a11y in mind' },
    { title: 'Customizable', description: 'Tailwind-first design' },
  ]

  return (
    <AnimatedList stagger={0.15} className="grid grid-cols-3 gap-6">
      {features.map((feature) => (
        <TiltCard
          key={feature.title}
          maxTilt={10}
          className="p-6 bg-card rounded-xl"
        >
          <h3 className="text-xl font-bold">{feature.title}</h3>
          <p className="text-muted-foreground">{feature.description}</p>
        </TiltCard>
      ))}
    </AnimatedList>
  )
}
```
