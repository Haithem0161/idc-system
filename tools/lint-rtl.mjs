#!/usr/bin/env node
/**
 * RTL chevron / arrow lint -- phase-08 §7.18.
 *
 * Walks `src/` for `.tsx` files. Fails when a directional lucide icon is
 * imported and used without an explicit `rtl:rotate-180` className.
 *
 * Tracked icons (the design system needs every directional cue to mirror
 * in RTL): `ChevronLeft`, `ChevronRight`, `ArrowLeft`, `ArrowRight`,
 * `MoveLeft`, `MoveRight`, `ChevronFirst`, `ChevronLast`.
 *
 * Rule: every JSX usage of one of these icons MUST contain
 * `rtl:rotate-180` somewhere in its class names. The check is line-local
 * for simplicity; multi-line JSX should keep the className on the same
 * line as the icon for readability anyway.
 *
 * Allowlist: `tools/rtl-allowlist.txt`. Add a substring per line.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

const ROOT = process.cwd()
const SRC = join(ROOT, 'src')
const ALLOWLIST_FILE = join(ROOT, 'tools', 'rtl-allowlist.txt')

const TRACKED = [
  'ChevronLeft',
  'ChevronRight',
  'ArrowLeft',
  'ArrowRight',
  'MoveLeft',
  'MoveRight',
  'ChevronFirst',
  'ChevronLast',
]

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
    if (ent === 'node_modules' || ent.startsWith('.')) continue
    const p = join(dir, ent)
    const s = statSync(p)
    if (s.isDirectory()) walk(p, out)
    else if (/\.tsx$/.test(p)) out.push(p)
  }
  return out
}

function lint (file, allow) {
  const src = readFileSync(file, 'utf-8')
  const violations = []
  src.split('\n').forEach((line, i) => {
    for (const icon of TRACKED) {
      const usageRe = new RegExp(`<${icon}\\b[^/>]*/?>`, 'g')
      let m
      while ((m = usageRe.exec(line))) {
        const tag = m[0]
        if (allow.some((a) => tag.includes(a))) continue
        if (tag.includes('rtl:rotate-180')) continue
        violations.push({ line: i + 1, icon, snippet: tag.trim() })
      }
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
      console.log(`  ${v.line}:1  ${v.icon}  ${v.snippet}`)
    }
  }
  if (total > 0) {
    console.error(`\n${total} RTL chevron violation(s). Add 'rtl:rotate-180' to className, or whitelist in tools/rtl-allowlist.txt.`)
    process.exit(1)
  } else {
    console.log('lint-rtl: 0 violations')
  }
}

main()
