import { readFile } from "node:fs/promises"

import * as arboriumHost from "@arborium/arborium/arborium_host.js"
import {
  registerGrammar,
  type ArboriumConfig,
  type Grammar,
} from "@arborium/arborium"
import * as rustGrammar from "@arborium/rust"
import * as tomlGrammar from "@arborium/toml"
import * as yamlGrammar from "@arborium/yaml"

const quietLogger = {
  debug: () => {},
  warn: console.warn,
  error: console.error,
}

let hostWasmPromise: Promise<Uint8Array<ArrayBuffer>> | null = null
let rustGrammarPromise: Promise<Grammar> | null = null
let tomlGrammarPromise: Promise<Grammar> | null = null
let yamlGrammarPromise: Promise<Grammar> | null = null

async function loadWasm(specifier: string): Promise<Uint8Array<ArrayBuffer>> {
  return await readFile(new URL(import.meta.resolve(specifier)))
}

async function loadHostWasm(): Promise<Uint8Array<ArrayBuffer>> {
  hostWasmPromise ??= loadWasm("@arborium/arborium/arborium_host_bg.wasm")

  return await hostWasmPromise
}

async function registerLocalGrammar(
  grammarModule: unknown,
  wasmSpecifier: string
): Promise<Grammar> {
  const [grammarWasm, hostWasm] = await Promise.all([
    loadWasm(wasmSpecifier),
    loadHostWasm(),
  ])

  const config = {
    logger: quietLogger,
    resolveHostJs: () => arboriumHost,
    resolveHostWasm: () => hostWasm,
  } satisfies ArboriumConfig

  return await registerGrammar(grammarModule, grammarWasm, config)
}

async function loadRustGrammar(): Promise<Grammar> {
  if (!rustGrammarPromise) {
    rustGrammarPromise = registerLocalGrammar(
      rustGrammar,
      "@arborium/rust/grammar_bg.wasm"
    )
  }

  return await rustGrammarPromise
}

async function loadTomlGrammar(): Promise<Grammar> {
  if (!tomlGrammarPromise) {
    tomlGrammarPromise = registerLocalGrammar(
      tomlGrammar,
      "@arborium/toml/grammar_bg.wasm"
    )
  }

  return await tomlGrammarPromise
}

export async function highlightRust(source: string): Promise<string> {
  const grammar = await loadRustGrammar()

  return await grammar.highlight(source)
}

export function wrapHighlightLines(
  html: string,
  lineNumbers: readonly number[]
): string {
  if (lineNumbers.length === 0) {
    return html
  }

  const highlighted = new Set(lineNumbers)
  const lines = html.split("\n")

  return lines
    .map((line, index) => {
      const isLast = index === lines.length - 1
      const lineBreak = isLast ? "" : "\n"

      if (highlighted.has(index)) {
        // Keep the newline inside the block span; an external one renders as a blank line in <pre>.
        return `<span class="transport-variant-line">${line}${lineBreak}</span>`
      }

      return isLast ? line : `${line}\n`
    })
    .join("")
}

export async function highlightToml(source: string): Promise<string> {
  const grammar = await loadTomlGrammar()

  return await grammar.highlight(source)
}

async function loadYamlGrammar(): Promise<Grammar> {
  if (!yamlGrammarPromise) {
    yamlGrammarPromise = registerLocalGrammar(
      yamlGrammar,
      "@arborium/yaml/grammar_bg.wasm"
    )
  }

  return await yamlGrammarPromise
}

export async function highlightYaml(source: string): Promise<string> {
  const grammar = await loadYamlGrammar()

  return await grammar.highlight(source)
}
