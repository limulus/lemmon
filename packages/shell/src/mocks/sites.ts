import { mkdirSync, readdirSync, readFileSync } from 'node:fs'
import { join, relative, sep } from 'node:path'

import { tempDir } from './temp-dir.ts'
import type { LoadedConfig } from '../lib/config.ts'
import type { Plugin } from '../lib/plugins.ts'

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

/**
 * The config {@link project} writes, as `loadConfig` would load it, listing `plugins` —
 * which a config written as JSON cannot hold, so they are given here alone.
 */
export function configOf(
  dir: string,
  passthrough: string[] = [],
  plugins: Plugin[] = []
): LoadedConfig {
  return {
    file: join(dir, 'lemmon.config.ts'),
    input: join(dir, 'www'),
    output: join(dir, 'dist'),
    served: [],
    passthrough,
    plugins,
    dev: { headers: {} },
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
 * A page type for pages that want no layout around them (D48): every page has a type,
 * and a `Memo`'s layout passes its body through, but for a draft's marker — read so that
 * the layout reads its record, which a layout that ignores it is warned for until `∆gsV`.
 */
export const MEMO_LEM = `<lem-type Memo extends Page>
  title: string = ""
  linkUrl: string?
  layout: (Memo, html): html = memo
</lem-type>

<lem-fn memo(m: Memo, body: html)>
  <lem-if m.draft>(a draft) </lem-if><lem.body>
</lem-fn>
`

/**
 * The bindings file of a minimal site: {@link MEMO_LEM} and a glob that makes every
 * document a `Memo`.
 */
export const BARE_LEM = `${MEMO_LEM}<lem-let pages: [Memo] from "**/*.md" />\n`

/**
 * The bindings file of a small typed site: a `Post` collection whose posts may name a
 * hero from a data file, a `Home` page type, a site motto from the environment, a
 * two-step layout chain — each `Post` and `Home` rendered through its own layout,
 * which wraps `base` — and the notes, `Memo`s rendered as they are.
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
${MEMO_LEM}<lem-let notes: [Memo] from "notes/*.md" />
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
 * the glob holds, a home page template its own `type:` names, a note the notes glob
 * holds, the photo index, a stylesheet for a passthrough glob to copy, and a WebC
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

/**
 * What a site that generates pages adds to its bindings file (D43): a topic page type
 * whose defaults give each page its title and url from its item, and which declares its
 * neighbours; its layout; and the list a generator pages over.
 */
export const TOPICS_LEM = `<lem-type Topic extends Page>
  topic: string
  title: string = "TILs: " + self.topic
  url: url = url("/tils/" + self.topic + "/")
  prev: Topic?
  next: Topic?
  layout: (Topic, html): html = topical
</lem-type>

<lem-fn topical(t: Topic, body: html)>
  <lem.base(t.title, body)>
</lem-fn>

<lem-let topics: [string] = ["git", "nodejs", "rust"] />
`

/** A generator of a page per item of {@link TOPICS_LEM}'s list, linking to its neighbours. */
export const TOPIC_LEM = `---
type: Topic
for:
  topic: site.topics
---
<lem-if page.prev><a rel="prev" href="<lem.page.prev.url>"><lem.page.prev.title></a></lem-if>
<lem-if page.next><a rel="next" href="<lem.page.next.url>"><lem.page.next.title></a></lem-if>
`

/** {@link SITE} with the generator `topics.lem` over {@link TOPICS_LEM}'s list. */
export const GENERATING: Record<string, string> = {
  ...SITE,
  'site.lem': SITE_LEM + TOPICS_LEM,
  'topics.lem': TOPIC_LEM,
}

/** The passthrough globs {@link SITE} is built with. */
export const PASSTHROUGH = ['assets/**']

/** The environment {@link SITE} is built in. */
export const ENV = { MOTTO: 'hi' }
