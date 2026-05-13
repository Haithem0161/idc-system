#!/usr/bin/env node
/**
 * i18n lint -- phase-08 §7.9.
 *
 * Walks every `.tsx` / `.ts` file under `src/` excluding `src/i18n/locales/`.
 * Fails on:
 * - Arabic literals (any character in `[؀-ۿ]`).
 * - English words `[A-Za-z]{4,}` inside `JSXText` and inside string-literal
 *   arguments of `aria-label`, `title`, `placeholder`, `alt` -- UNLESS the
 *   value sits inside a `t(...)` or `<Trans>` call.
 *
 * Allowlist: `tools/i18n-allowlist.txt`, one substring per line. A literal
 * matches a violation if any allowlist entry is contained in it.
 *
 * Implementation deliberately avoids a heavy AST parser: it uses a
 * line-oriented scan that recognises the common JSX prop patterns and the
 * tagged-template / function-call wrappers we care about. Trade-off:
 * may flag exotic call shapes (e.g. dynamic prop spreads); fix by adding
 * an allowlist entry rather than rewriting the linter.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

const ROOT = process.cwd()
const SRC = join(ROOT, 'src')
const LOCALES = join(SRC, 'i18n', 'locales')
const ALLOWLIST_FILE = join(ROOT, 'tools', 'i18n-allowlist.txt')

const ARABIC_RE = /[؀-ۿ]/
const ENGLISH_WORD_RE = /[A-Za-z]{4,}/
const PROP_RE = /(?:aria-label|title|placeholder|alt)=("[^"]+"|'[^']+')/g
const JSX_TEXT_RE = />([^<>{}\n]+)</g

function loadAllowlist () {
  try {
    return readFileSync(ALLOWLIST_FILE, 'utf-8')
      .split('\n')
      .map((s) => s.trim())
      .filter((s) => s.length > 0 && !s.startsWith('#'))
  } catch {
    return []
  }
}

function walk (dir, out = []) {
  for (const ent of readdirSync(dir)) {
    if (ent === 'node_modules' || ent === 'locales' || ent.startsWith('.')) continue
    const p = join(dir, ent)
    const s = statSync(p)
    if (s.isDirectory()) {
      if (p === LOCALES) continue
      walk(p, out)
    } else if (/\.(tsx|ts)$/.test(p) && !p.endsWith('.d.ts')) {
      out.push(p)
    }
  }
  return out
}

function isAllowed (literal, allow) {
  if (allow.some((a) => literal.includes(a))) return true
  return false
}

function isWrapped (line, idx) {
  // Crude check: walk left for `t(`, `<Trans` or template tag.
  const window = line.slice(Math.max(0, idx - 32), idx)
  if (/(?:^|[^a-zA-Z_$])t\($/.test(window + '(')) return true
  if (/<Trans\b/.test(line.slice(0, idx))) return true
  return false
}

function lint (file, allow) {
  const src = readFileSync(file, 'utf-8')
  const violations = []
  src.split('\n').forEach((line, i) => {
    // Skip imports, comments, type-only declarations, and console statements.
    const trimmed = line.trim()
    if (trimmed.startsWith('//')) return
    if (trimmed.startsWith('*') || trimmed.startsWith('/*')) return
    if (trimmed.startsWith('import ')) return
    if (trimmed.startsWith('export type') || trimmed.startsWith('type ')) return
    if (/^console\.(log|warn|error|info|debug|trace)/.test(trimmed)) return

    // Arabic anywhere in the line (excluding allowlist).
    if (ARABIC_RE.test(line)) {
      const matches = line.match(/[؀-ۿ][^"'<]*/g) ?? []
      for (const m of matches) {
        if (!isAllowed(m, allow)) {
          violations.push({ line: i + 1, kind: 'arabic', value: m.trim() })
        }
      }
    }

    // JSXText: text between > and < on the same line.
    let mt
    JSX_TEXT_RE.lastIndex = 0
    while ((mt = JSX_TEXT_RE.exec(line))) {
      const txt = mt[1].trim()
      if (txt.length === 0) continue
      if (!ENGLISH_WORD_RE.test(txt)) continue
      // Skip pure expressions like `{count}`.
      if (/^[\d\s.,:;\-_]+$/.test(txt)) continue
      if (isAllowed(txt, allow)) continue
      // Allow if within {t(...)} -- handled by the surrounding-line check.
      if (/\bt\(/.test(line)) continue
      violations.push({ line: i + 1, kind: 'jsx-text', value: txt })
    }

    // Prop string literals.
    let mp
    PROP_RE.lastIndex = 0
    while ((mp = PROP_RE.exec(line))) {
      const literal = mp[1].slice(1, -1)
      if (!ENGLISH_WORD_RE.test(literal)) continue
      if (isAllowed(literal, allow)) continue
      if (isWrapped(line, mp.index)) continue
      violations.push({ line: i + 1, kind: 'prop', value: literal })
    }
  })
  return violations
}

function main () {
  const allow = loadAllowlist()
  const files = walk(SRC)
  let total = 0
  for (const f of files) {
    const vs = lint(f, allow)
    if (vs.length === 0) continue
    total += vs.length
    const rel = relative(ROOT, f)
    console.log(`\n${rel}`)
    for (const v of vs) {
      console.log(`  ${v.line}:1  ${v.kind}  ${JSON.stringify(v.value)}`)
    }
  }
  if (total > 0) {
    console.error(`\n${total} i18n violation(s). Add legitimate literals to tools/i18n-allowlist.txt.`)
    process.exit(1)
  } else {
    console.log('lint-i18n: 0 violations')
  }
}

main()
