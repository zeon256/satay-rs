import { useState } from "react"
import { CheckIcon, CopyIcon } from "@phosphor-icons/react"

type CopyButtonProps = {
  value: string
  label?: string
}

export function CopyButton({ value, label = "Copy command" }: CopyButtonProps) {
  const [copied, setCopied] = useState(false)

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      setCopied(false)
    }
  }

  return (
    <button
      type="button"
      onClick={handleCopy}
      aria-label={label}
      className="inline-flex size-9 shrink-0 items-center justify-center rounded-none border border-border/50 text-cream-muted transition-colors hover:bg-cream/5 hover:text-cream focus-visible:ring-2 focus-visible:ring-ember/60 focus-visible:outline-none"
    >
      {copied ? (
        <CheckIcon className="size-4 text-ember" />
      ) : (
        <CopyIcon className="size-4" />
      )}
    </button>
  )
}
