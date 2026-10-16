import { mkdirSync, readdirSync, readFileSync } from 'node:fs'
import { join, relative, sep } from 'node:path'

import { tempDir } from './temp-dir.ts'
import type { LoadedConfig } from '../lib/config.ts'

/**
 * A project directory: a `lemmon.config.ts` naming `www` as the input and `dist` as the
 * output, with `passthrough` its globs, and `files` under `www/`.
 */
export function project(files: Record<string, string>, passthrough: string[] = []): string {
  const fields = JSON.stringify({ input: 'www', output: 'dist', passthrough })
  const entries = Object.entries(files).map(([path, text]) => [`www/${path}`, text])
  const dir = tempDir({
    'lemmon.config.ts': `export default ${fields}`,
    ...Object.fromEntries(entries),
  })
  mkdirSync(join(dir, 'www'), { recursive: true })
  return dir
}

/** The config {@link project} writes, as `loadConfig` would load it. */
export function configOf(dir: string, passthrough: string[] = []): LoadedConfig {
  return {
    file: join(dir, 'lemmon.config.ts'),
    input: join(dir, 'www'),
    output: join(dir, 'dist'),
    passthrough,
  }
}

/** Every file under `dir`, by its path from there with `/` separators, as text. */
export function tree(dir: string): Record<string, string> {
  const files: Record<string, string> = {}
  for (const entry of readdirSync(dir, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) continue
    const file = join(entry.parentPath, entry.name)
    files[relative(dir, file).split(sep).join('/')] = readFileSync(file, 'utf8')
  }
  return files
}

/**
 * The bindings file of a small typed site: a `Post` collection whose posts may name a
 * hero from a data file, a `Home` page type, a site motto from the environment, and a
 * two-step layout chain — each `Post` and `Home` rendered through its own layout,
 * which wraps `base`.
 */
export const SITE_LEM = `<lem-type Photo>
  caption: string
</lem-type>

<lem-type Post extends Page>
  title: string
  hero: Photo? by site.photos
  layout: (Post, html): html = post
</lem-type>

<lem-type Home extends Page>
  title: string
  layout: (Home, html): html = home
</lem-type>

<lem-let posts: [Post] from "posts/*.md" />
<lem-let photos: {string: Photo} from "_data/photos.json" />
<lem-let motto: string from env("MOTTO") />
`

/** The layouts the site's types render through. */
export const LAYOUTS_LEM = `<lem-fn base(title: string, body: html)>
  <title><lem.title> · <lem.site.motto></title>
  <meta name="generator" content="<lem.build.generator>">
  <main><lem.body></main>
</lem-fn>

<lem-fn post(p: Post, body: html)>
  <lem-let inner>
    <h1><lem.p.title></h1>
    <lem-if p.hero>
      <figure><lem.p.hero.caption></figure>
    </lem-if>
    <lem.body>
  </lem-let>
  <lem.base(p.title, inner)>
</lem-fn>

<lem-fn home(h: Home, body: html)>
  <lem.base(h.title, body)>
</lem-fn>
`

/**
 * The whole site, by path under the input: the bindings and layouts above, two posts
 * the glob holds, a home page template its own `type:` names, a document nothing
 * types, the photo index, a stylesheet for a passthrough glob to copy, and a WebC
 * component the walk ignores.
 */
export const SITE: Record<string, string> = {
  'site.lem': SITE_LEM,
  'layouts.lem': LAYOUTS_LEM,
  'posts/a.md': '---\ntitle: A\nhero: sunset\n---\nHello *a*.\n',
  'posts/b.md': '---\ntitle: B\n---\nHello b.\n',
  'index.lem':
    '---\ntype: Home\ntitle: Home\nurl: /\n---\n<lem-for p in site.posts>\n' +
    '  <a href="<lem.p.url>"><lem.p.title></a>\n</lem-for>\n',
  'notes/loose.md':
    '---\ntitle: Loose\n---\n<lem.page.title>\n\nBuilt at:\n\n<lem.build.time>\n',
  '_data/photos.json': '{\n  "sunset": { "caption": "A sunset" }\n}\n',
  'assets/site.css': 'main { margin: 0 }\n',
  'components/old.webc': '<p>old</p>\n',
}

/** The passthrough globs {@link SITE} is built with. */
export const PASSTHROUGH = ['assets/**']

/** The environment {@link SITE} is built in. */
export const ENV = { MOTTO: 'hi' }
