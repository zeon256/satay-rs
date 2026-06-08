# Satay website

Marketing site for [Satay](https://github.com/zeon256/satay-rs), built with Astro, React, TypeScript, and Tailwind CSS.

## Development

```bash
bun install
bun run dev
```

## Scripts

- `bun run dev` — local dev server
- `bun run build` — production build
- `bun run preview` — preview production build
- `bun run typecheck` — Astro + TypeScript check
- `bun run lint` — oxlint
- `bun run format` — oxfmt

## Adding a project to "Built with Satay"

Projects shown on the landing page are listed in [`src/data/users.ts`](src/data/users.ts).

1. Fork [zeon256/satay-rs](https://github.com/zeon256/satay-rs).
2. Add an entry to the `satayUsers` array in `website/src/data/users.ts`:

```ts
{
  name: "your-crate",
  description: "One sentence about what your project does.",
  href: "https://github.com/you/your-crate",
  repo: "https://github.com/you/your-crate",
  logo: "/users/your-crate.webp", // optional — site path or absolute URL
  logoAlt: "your-crate logo", // optional
  crates: "https://crates.io/crates/your-crate", // optional
  docs: "https://docs.rs/your-crate", // optional
},
```

3. **Optional logo:** add a square image (`.webp`, `.png`, or `.svg`, ideally ~128×128) to `public/users/your-crate.webp` and set the `logo` field to `/users/your-crate.webp`. Projects without a logo show a two-letter monogram instead.

4. Open a pull request against `main`.

The site also links directly to the GitHub editor for that file so contributors can submit from the landing page.
