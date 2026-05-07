# React Helmet Guide

Enterprise-grade patterns for managing document head and SEO with React Helmet Async.

## Installation

```bash
pnpm add react-helmet-async
```

## Basic Setup

```tsx
// src/main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { HelmetProvider } from 'react-helmet-async'
import App from './App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HelmetProvider>
      <App />
    </HelmetProvider>
  </StrictMode>
)
```

## Basic Usage

```tsx
import { Helmet } from 'react-helmet-async'

function HomePage() {
  return (
    <>
      <Helmet>
        <title>Home | My Website</title>
        <meta name="description" content="Welcome to our website" />
      </Helmet>

      <h1>Welcome Home</h1>
    </>
  )
}
```

## SEO Essentials

### Page Title and Description

```tsx
<Helmet>
  <title>Product Name - Tagline | Brand</title>
  <meta name="description" content="A compelling description under 160 characters that summarizes the page content." />
</Helmet>
```

### Open Graph (Facebook, LinkedIn)

```tsx
<Helmet>
  <meta property="og:type" content="website" />
  <meta property="og:url" content="https://example.com/page" />
  <meta property="og:title" content="Page Title" />
  <meta property="og:description" content="Page description for social sharing" />
  <meta property="og:image" content="https://example.com/og-image.jpg" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:site_name" content="Site Name" />
  <meta property="og:locale" content="en_US" />
</Helmet>
```

### Twitter Cards

```tsx
<Helmet>
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:site" content="@username" />
  <meta name="twitter:creator" content="@username" />
  <meta name="twitter:title" content="Page Title" />
  <meta name="twitter:description" content="Page description" />
  <meta name="twitter:image" content="https://example.com/twitter-image.jpg" />
</Helmet>
```

### Canonical URL

```tsx
<Helmet>
  <link rel="canonical" href="https://example.com/canonical-page" />
</Helmet>
```

## SEO Component

Create a reusable SEO component for consistent meta tags.

```tsx
// src/components/SEO.tsx
import { Helmet } from 'react-helmet-async'

interface SEOProps {
  title: string
  description: string
  image?: string
  url?: string
  type?: 'website' | 'article' | 'product'
  article?: {
    publishedTime?: string
    modifiedTime?: string
    author?: string
    tags?: string[]
  }
  noindex?: boolean
}

const SITE_NAME = 'My Website'
const DEFAULT_IMAGE = 'https://example.com/default-og.jpg'
const BASE_URL = 'https://example.com'

export function SEO({
  title,
  description,
  image = DEFAULT_IMAGE,
  url,
  type = 'website',
  article,
  noindex = false,
}: SEOProps) {
  const fullTitle = `${title} | ${SITE_NAME}`
  const fullUrl = url ? `${BASE_URL}${url}` : BASE_URL

  return (
    <Helmet>
      {/* Basic */}
      <title>{fullTitle}</title>
      <meta name="description" content={description} />
      <link rel="canonical" href={fullUrl} />

      {/* Robots */}
      {noindex && <meta name="robots" content="noindex, nofollow" />}

      {/* Open Graph */}
      <meta property="og:type" content={type} />
      <meta property="og:url" content={fullUrl} />
      <meta property="og:title" content={title} />
      <meta property="og:description" content={description} />
      <meta property="og:image" content={image} />
      <meta property="og:site_name" content={SITE_NAME} />

      {/* Twitter */}
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:title" content={title} />
      <meta name="twitter:description" content={description} />
      <meta name="twitter:image" content={image} />

      {/* Article specific */}
      {article?.publishedTime && (
        <meta property="article:published_time" content={article.publishedTime} />
      )}
      {article?.modifiedTime && (
        <meta property="article:modified_time" content={article.modifiedTime} />
      )}
      {article?.author && (
        <meta property="article:author" content={article.author} />
      )}
      {article?.tags?.map((tag) => (
        <meta key={tag} property="article:tag" content={tag} />
      ))}
    </Helmet>
  )
}
```

### Usage

```tsx
// Home page
function HomePage() {
  return (
    <>
      <SEO
        title="Home"
        description="Welcome to our amazing website"
        url="/"
      />
      <h1>Home</h1>
    </>
  )
}

// Product page
function ProductPage({ product }: { product: Product }) {
  return (
    <>
      <SEO
        title={product.name}
        description={product.description}
        image={product.image}
        url={`/products/${product.slug}`}
        type="product"
      />
      <ProductDetails product={product} />
    </>
  )
}

// Blog article
function ArticlePage({ article }: { article: Article }) {
  return (
    <>
      <SEO
        title={article.title}
        description={article.excerpt}
        image={article.coverImage}
        url={`/blog/${article.slug}`}
        type="article"
        article={{
          publishedTime: article.publishedAt,
          modifiedTime: article.updatedAt,
          author: article.author.name,
          tags: article.tags,
        }}
      />
      <ArticleContent article={article} />
    </>
  )
}

// Private/admin page (no indexing)
function AdminPage() {
  return (
    <>
      <SEO
        title="Admin Dashboard"
        description="Admin area"
        noindex={true}
      />
      <AdminDashboard />
    </>
  )
}
```

## i18n Integration

For multilingual sites with Arabic/English support.

```tsx
import { Helmet } from 'react-helmet-async'
import { useTranslation } from 'react-i18next'

interface I18nSEOProps {
  titleKey: string
  descriptionKey: string
  url: string
}

export function I18nSEO({ titleKey, descriptionKey, url }: I18nSEOProps) {
  const { t, i18n } = useTranslation()
  const isRTL = i18n.dir() === 'rtl'
  const lang = i18n.language

  return (
    <Helmet>
      <html lang={lang} dir={isRTL ? 'rtl' : 'ltr'} />
      <title>{t(titleKey)}</title>
      <meta name="description" content={t(descriptionKey)} />

      {/* Alternate language links */}
      <link rel="alternate" hrefLang="en" href={`https://example.com/en${url}`} />
      <link rel="alternate" hrefLang="ar" href={`https://example.com/ar${url}`} />
      <link rel="alternate" hrefLang="x-default" href={`https://example.com/en${url}`} />

      {/* Open Graph locale */}
      <meta property="og:locale" content={lang === 'ar' ? 'ar_SA' : 'en_US'} />
      <meta property="og:locale:alternate" content={lang === 'ar' ? 'en_US' : 'ar_SA'} />
    </Helmet>
  )
}
```

## Additional Meta Tags

### Favicon and Icons

```tsx
<Helmet>
  <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
  <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png" />
  <link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png" />
  <link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png" />
  <link rel="manifest" href="/site.webmanifest" />
</Helmet>
```

### Theme Color

```tsx
<Helmet>
  <meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)" />
  <meta name="theme-color" content="#0a0a0a" media="(prefers-color-scheme: dark)" />
</Helmet>
```

### Viewport

```tsx
<Helmet>
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
</Helmet>
```

### Keywords (less important for SEO now)

```tsx
<Helmet>
  <meta name="keywords" content="react, typescript, web development" />
</Helmet>
```

### Author

```tsx
<Helmet>
  <meta name="author" content="Your Name" />
</Helmet>
```

## Structured Data (JSON-LD)

```tsx
import { Helmet } from 'react-helmet-async'

interface ProductStructuredDataProps {
  product: {
    name: string
    description: string
    image: string
    price: number
    currency: string
    availability: 'InStock' | 'OutOfStock'
    brand: string
  }
}

export function ProductStructuredData({ product }: ProductStructuredDataProps) {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'Product',
    name: product.name,
    description: product.description,
    image: product.image,
    brand: {
      '@type': 'Brand',
      name: product.brand,
    },
    offers: {
      '@type': 'Offer',
      price: product.price,
      priceCurrency: product.currency,
      availability: `https://schema.org/${product.availability}`,
    },
  }

  return (
    <Helmet>
      <script type="application/ld+json">
        {JSON.stringify(structuredData)}
      </script>
    </Helmet>
  )
}

// Organization structured data
export function OrganizationStructuredData() {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'Organization',
    name: 'Company Name',
    url: 'https://example.com',
    logo: 'https://example.com/logo.png',
    sameAs: [
      'https://twitter.com/company',
      'https://linkedin.com/company/company',
    ],
  }

  return (
    <Helmet>
      <script type="application/ld+json">
        {JSON.stringify(structuredData)}
      </script>
    </Helmet>
  )
}

// Breadcrumb structured data
export function BreadcrumbStructuredData({ items }: { items: { name: string; url: string }[] }) {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: items.map((item, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: item.name,
      item: item.url,
    })),
  }

  return (
    <Helmet>
      <script type="application/ld+json">
        {JSON.stringify(structuredData)}
      </script>
    </Helmet>
  )
}
```

## Default Helmet (App Level)

Set defaults in your root layout.

```tsx
// src/App.tsx
import { Helmet } from 'react-helmet-async'

export default function App() {
  return (
    <>
      <Helmet
        defaultTitle="My Website"
        titleTemplate="%s | My Website"
      >
        <html lang="en" />
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#000000" />
        <link rel="icon" href="/favicon.ico" />
      </Helmet>

      {/* Rest of app */}
    </>
  )
}
```

## Best Practices

1. **Use `react-helmet-async`** - Not `react-helmet` (deprecated, memory leaks)
2. **Wrap with `HelmetProvider`** - Required at app root
3. **Create reusable SEO component** - Consistent meta tags across pages
4. **Set defaults at app level** - Use `defaultTitle` and `titleTemplate`
5. **Always include canonical** - Prevents duplicate content issues
6. **Optimize images** - OG images should be 1200x630px
7. **Test with validators** - Use Facebook Debugger, Twitter Card Validator
8. **Add structured data** - Helps search engines understand content
9. **Support multilingual** - Use hreflang for alternate languages
