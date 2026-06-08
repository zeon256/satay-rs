import { readFile } from "node:fs/promises"

import * as arboriumHost from "@arborium/arborium/arborium_host.js"
import {
  registerGrammar,
  type ArboriumConfig,
  type Grammar,
} from "@arborium/arborium"
import * as rustGrammar from "@arborium/rust"
import * as tomlGrammar from "@arborium/toml"

const quietLogger = {
  debug: () => {},
  warn: console.warn,
  error: console.error,
}

let hostWasmPromise: Promise<Uint8Array<ArrayBuffer>> | null = null
let rustGrammarPromise: Promise<Grammar> | null = null
let tomlGrammarPromise: Promise<Grammar> | null = null

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

export async function highlightToml(source: string): Promise<string> {
  const grammar = await loadTomlGrammar()

  return await grammar.highlight(source)
}
