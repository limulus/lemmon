import type { Diagnostic } from '@limulus/lemmon-core'
import { loadCore } from '@limulus/lemmon-core'
import { describe, expect, it } from 'vitest'

import {
  derivedUrl,
  findFrontMatter,
  opensFrontMatter,
  readPage,
  type Page,
} from './front-matter.ts'
import { at, covered } from '../mocks/spans.ts'

/** The page `text` reads as, which a test expects to read cleanly. */
function page(text: string, path = 'feed/post/index.md'): Page {
  const read = readPage(path, text)
  expect(read.diagnostics).toEqual([])
  if (read.page === undefined) throw new Error('no page')
  return read.page
}

/** The one diagnostic reading `text` reports, which a test expects to leave no page. */
function refused(text: string): Diagnostic {
  const read = readPage('post.md', text)
  expect(read.page).toBeUndefined()
  expect(read.diagnostics).toHaveLength(1)
  return read.diagnostics[0]
}

describe('findFrontMatter()', () => {
  it('should find nothing in a file that does not open with a rule', () => {
    expect(findFrontMatter('')).toBeUndefined()
    expect(findFrontMatter('text\n---\na: 1\n---\n')).toBeUndefined()
    expect(findFrontMatter('\n---\na: 1\n---\n')).toBeUndefined()
  })

  it('should find the block, and the YAML between its two lines', () => {
    const text = '---\na: 1\n---\nbody'
    const block = findFrontMatter(text)

    expect(block).toEqual({ end: 13, inner: { start: 4, end: 9 } })
    expect(text.slice(block?.inner.start, block?.inner.end)).toBe('a: 1\n')
    expect(text.slice(block?.end)).toBe('body')
  })

  it('should accept a carriage return before each line’s newline', () => {
    const text = '---\r\na: 1\r\n---\r\nbody'
    const block = findFrontMatter(text)

    expect(text.slice(block?.inner.start, block?.inner.end)).toBe('a: 1\r\n')
    expect(text.slice(block?.end)).toBe('body')
  })

  it('should close at the very next rule, even with nothing between', () => {
    expect(findFrontMatter('---\n---\n')).toEqual({ end: 8, inner: { start: 4, end: 4 } })
  })

  it('should close at a last line with no newline', () => {
    expect(findFrontMatter('---\na: 1\n---')).toEqual({
      end: 12,
      inner: { start: 4, end: 9 },
    })
  })

  it('should take only a line that is exactly three dashes as a rule', () => {
    expect(findFrontMatter('--- \na: 1\n---\n')).toBeUndefined()
    expect(findFrontMatter('----\na: 1\n---\n')).toBeUndefined()
    expect(findFrontMatter('---\na: 1\n--- \n')).toBeUndefined()
  })

  it('should find nothing where an opening line is never closed', () => {
    expect(findFrontMatter('---\na: 1\n')).toBeUndefined()
    expect(findFrontMatter('---')).toBeUndefined()
  })
})

describe('opensFrontMatter()', () => {
  it('should hold for a text opening with a rule, whether or not one closes it', () => {
    expect(opensFrontMatter('---\ntitle: a\n---\nBody\n')).toBe(true)
    expect(opensFrontMatter('---\r\ntitle: a\n')).toBe(true)
    expect(opensFrontMatter('---')).toBe(true)
  })

  it('should not hold for a text opening with anything else, or empty', () => {
    expect(opensFrontMatter('Body\n---\ntitle: a\n---\n')).toBe(false)
    expect(opensFrontMatter(' ---\n')).toBe(false)
    expect(opensFrontMatter('----\n')).toBe(false)
    expect(opensFrontMatter('')).toBe(false)
  })
})

describe('readPage()', () => {
  describe('with no front matter', () => {
    it('should load only what the shell derives, with nothing to span', () => {
      expect(page('# Hello\n')).toEqual({
        loaded: {
          value: { url: '/feed/post/', inputPath: 'feed/post/index.md' },
          path: 'feed/post/index.md',
          spans: {},
        },
        body: 0,
      })
    })

    it('should treat an opening line that is never closed as no front matter', () => {
      expect(page('---\ntitle: x\n').loaded.value).toEqual({
        url: '/feed/post/',
        inputPath: 'feed/post/index.md',
      })
    })
  })

  describe('with front matter', () => {
    const text = [
      '---',
      'title: Hello',
      'tags: [a, b, c]',
      'hero:',
      '  caption: Falls',
      '  credit: null',
      'draft: true',
      'count: 3',
      '---',
      'Body',
    ].join('\n')

    it('should load its fields in order, then the url and input path it lacks', () => {
      expect(page(text).loaded.value).toEqual({
        title: 'Hello',
        tags: ['a', 'b', 'c'],
        hero: { caption: 'Falls', credit: null },
        draft: true,
        count: 3,
        url: '/feed/post/',
        inputPath: 'feed/post/index.md',
      })
      expect(Object.keys(page(text).loaded.value)).toEqual([
        'title',
        'tags',
        'hero',
        'draft',
        'count',
        'url',
        'inputPath',
      ])
    })

    it('should span every field path from its key through its value', () => {
      const { spans } = page(text).loaded

      expect(spans.title).toEqual(at(text, 'title: Hello'))
      expect(spans.tags).toEqual(at(text, 'tags: [a, b, c]'))
      expect(spans['tags.2']).toEqual(at(text, '[a, b, «c»]'))
      expect(spans.hero).toEqual(at(text, 'hero:\n  caption: Falls\n  credit: null'))
      expect(spans['hero.caption']).toEqual(at(text, 'caption: Falls'))
      expect(spans['hero.credit']).toEqual(at(text, 'credit: null'))
      expect(spans.count).toEqual(at(text, 'count: 3'))
    })

    it('should span the whole value from its first key to its last value', () => {
      const { spans } = page(text).loaded

      expect(covered(text, spans[''])).toBe(text.slice(4, text.indexOf('count: 3') + 8))
    })

    it('should span the derived fields nowhere, since no text gave them', () => {
      const { spans } = page(text).loaded

      expect(spans.url).toBeUndefined()
      expect(spans.inputPath).toBeUndefined()
    })

    it('should give the byte offset of the body', () => {
      const withWide = '---\nsubhead: I’m here\n---\nBody'

      expect(page(withWide).body).toBe(at(withWide, 'Body').start)
      expect(page(withWide).body).toBe(withWide.indexOf('Body') + 2)
    })

    it('should count bytes for offsets, UTF-16 units for columns past wide text', () => {
      const wide =
        '---\nsubhead: I’m here — 😀 really\ntitle: x\nhero: {by: 😀, at: y}\n---\n'
      const { spans } = page(wide).loaded

      expect(spans.title).toEqual(at(wide, 'title: x'))
      expect(spans['hero.at']).toEqual(at(wide, 'at: y'))
      // `’` and `—` are three bytes and one code unit each, and `😀` four bytes and two
      // code units, so the bytes before `at:` outnumber its code units by eight; on its
      // own line, two of them are the second `😀`'s.
      expect(spans['hero.at']).toEqual({
        start: wide.indexOf('at: y') + 8,
        end: wide.indexOf('at: y') + 8 + 5,
        range: {
          start: { line: 3, character: 15 },
          end: { line: 3, character: 20 },
        },
      })
      expect(spans.subhead.range.end).toEqual({ line: 1, character: 29 })
    })

    it('should end a block scalar’s span at its last character, not its line break', () => {
      const folded = '---\nsubhead: >-\n  I’m rebuilding.\n  It’s time.\ndate: x\n---\n'
      const { spans, value } = page(folded).loaded

      expect(value.subhead).toBe('I’m rebuilding. It’s time.')
      expect(spans.subhead).toEqual(
        at(folded, 'subhead: >-\n  I’m rebuilding.\n  It’s time.')
      )
    })

    it('should read CRLF front matter, its spans stopping before the line ends', () => {
      const crlf = '---\r\ntitle: x\r\ntags:\r\n  - a\r\n---\r\nBody'
      const { spans, value } = page(crlf).loaded

      expect(value).toMatchObject({ title: 'x', tags: ['a'] })
      expect(spans.title).toEqual(at(crlf, 'title: x'))
      expect(spans.tags).toEqual(at(crlf, 'tags:\r\n  - a'))
    })

    it('should read a key with no value as null, spanning the key', () => {
      const bare = '---\n? note\ntitle:\n---\n'
      const { spans, value } = page(bare).loaded

      expect(value).toMatchObject({ note: null, title: null })
      expect(spans.note).toEqual(at(bare, 'note'))
      expect(spans.title).toEqual(at(bare, 'title:'))
    })

    it('should keep a `__proto__` key as a field of its own', () => {
      const { value } = page('---\n__proto__: x\n---\n').loaded

      expect(Object.hasOwn(value, '__proto__')).toBe(true)
      expect(Object.getPrototypeOf(value)).toBe(Object.prototype)
    })

    it('should read a timestamp as the string it is written as', () => {
      const { value } = page('---\ndate: 2024-11-24 11:35:00 -07:00\n---\n').loaded

      expect(value.date).toBe('2024-11-24 11:35:00 -07:00')
    })

    it('should span the whole block for front matter empty or only comments', () => {
      const empty = '---\n---\nBody'
      const comments = '---\n# nothing yet\n---\n'

      expect(page(empty).loaded.spans).toEqual({ '': at(empty, '---\n---') })
      expect(page(empty).loaded.value).toEqual({
        url: '/feed/post/',
        inputPath: 'feed/post/index.md',
      })
      expect(page(comments).loaded.spans).toEqual({
        '': at(comments, '---\n# nothing yet\n---'),
      })
    })
  })

  describe('the `type:` key', () => {
    it('should name the record, spanning its value, and be stripped', () => {
      const text = '---\ntype: Article\ntitle: x\n---\n'
      const read = page(text)

      expect(read.record).toEqual({ name: 'Article', span: at(text, 'Article') })
      expect(read.loaded.value).not.toHaveProperty('type')
      expect(read.loaded.spans).not.toHaveProperty('type')
    })

    it('should span a quoted value with its quotes', () => {
      const text = '---\ntype: "Article"\n---\n'

      expect(page(text).record).toEqual({ name: 'Article', span: at(text, '"Article"') })
    })

    it('should be read only at the top level', () => {
      const text = '---\nhero:\n  type: photo\n---\n'

      expect(page(text).record).toBeUndefined()
      expect(page(text).loaded.value.hero).toEqual({ type: 'photo' })
    })

    it('should be refused when its value is not a string, spanning the entry', () => {
      const list = '---\ntype: [Article]\n---\n'
      const empty = '---\ntype:\ntitle: x\n---\n'
      const bare = '---\n? type\n---\n'
      const message = '`type:` names a record, so its value must be a string'

      expect(refused(list)).toMatchObject({ span: at(list, 'type: [Article]'), message })
      expect(refused(empty)).toMatchObject({ span: at(empty, 'type:'), message })
      expect(refused(bare)).toMatchObject({ span: at(bare, '«type»\n'), message })
    })

    it('should give the core a record span that lands on the value', async () => {
      const text = '---\ntitle: I’m here — 😀\ntype: Missive\n---\n<lem.page.title>\n'
      const record = page(text, 'post.md').record
      if (record === undefined) throw new Error('no record')
      const { name, span } = record
      const core = await loadCore()

      // `{ start, end }` of the span: the core refuses the `range` beside them.
      const recordSpan = { start: span.start, end: span.end }
      const { diagnostics } = core.compile(
        [{ path: 'post.md', text, record: name, recordSpan }],
        {
          ambient: { page: null },
          document: 'page',
        }
      )
      core.free()

      expect(diagnostics).toEqual([
        {
          path: 'post.md',
          span: at(text, 'Missive'),
          severity: 'error',
          message: 'unknown type `Missive`',
          labels: [],
        },
      ])
    })
  })

  describe('the `for:` key', () => {
    const text = '---\ntype: TilTopicPage\nfor:\n  topic: site.tilTopics\n---\n'

    it('should be read and stripped, with spans keyed from its value', () => {
      const read = page(text, 'tils/topic.lem')

      expect(read.for).toEqual({
        value: { topic: 'site.tilTopics' },
        spans: {
          '': at(text, 'topic: site.tilTopics'),
          topic: at(text, 'topic: site.tilTopics'),
        },
      })
      expect(read.loaded.value).not.toHaveProperty('for')
    })

    it('should leave the url to the generator’s type', () => {
      expect(page(text, 'tils/topic.lem').loaded.value).toEqual({
        inputPath: 'tils/topic.lem',
      })
    })

    it('should span the key when it has no value', () => {
      const bare = '---\n? for\n---\n'

      expect(page(bare).for).toEqual({ value: null, spans: { '': at(bare, '«for»\n') } })
    })
  })

  describe('the url', () => {
    it('should be kept when the front matter gives one, as should the input path', () => {
      const text = '---\nurl: /feed.xml\ninputPath: elsewhere\n---\n'

      expect(page(text, 'feed.lem').loaded.value).toEqual({
        url: '/feed.xml',
        inputPath: 'elsewhere',
      })
    })

    it('should be derived from the path otherwise', () => {
      expect(page('---\n---\n', 'feed/index.lem').loaded.value.url).toBe('/feed/')
    })
  })

  describe('the input path', () => {
    it('should start with the input directory, where the project names it', () => {
      const read = readPage('feed/the-rebound/index.md', '# x\n', { inputDir: 'www' })

      expect(read.page?.loaded.value.inputPath).toBe('www/feed/the-rebound/index.md')
      expect(read.page?.loaded.path).toBe('feed/the-rebound/index.md')
    })

    it('should be the core’s path when the input is the project directory', () => {
      const read = readPage('a.md', '---\n---\n', { inputDir: '' })

      expect(read.page?.loaded.value.inputPath).toBe('a.md')
    })
  })

  describe('front matter that cannot be loaded', () => {
    it('should report malformed YAML where the parser stopped', () => {
      const text = '---\ntitle: a: b\n---\n'

      expect(refused(text)).toEqual({
        path: 'post.md',
        span: at(text, 'title: «a»'),
        severity: 'error',
        message:
          'front matter is not valid YAML: ' +
          'Nested mappings are not allowed in compact mappings',
        labels: [],
      })
    })

    it('should report a duplicate key at its second place, labelling the first', () => {
      const text = '---\ntitle: a\ntitle: b\n---\n'

      expect(refused(text)).toEqual({
        path: 'post.md',
        span: at(text, 'title: a\n«title»'),
        severity: 'error',
        message: '`title` is already given',
        labels: [
          { path: 'post.md', span: at(text, '«title»: a'), message: 'first given here' },
        ],
      })
    })

    it('should name a nested duplicate by its field path', () => {
      const text = '---\nhero:\n  caption: a\n  caption: b\n---\n'

      expect(refused(text)).toMatchObject({
        span: at(text, 'caption: a\n  «caption»'),
        message: '`hero.caption` is already given',
      })
    })

    it('should report a duplicate `type:` key, rather than pick one', () => {
      const text = '---\ntype: A\ntype: B\n---\n'

      expect(refused(text).span).toEqual(at(text, 'type: A\n«type»'))
    })

    it('should report front matter that is not a map, over the whole of it', () => {
      const list = '---\n- a\n- b\n---\n'
      const scalar = '---\njust words\n---\n'
      const message = 'front matter must be a map of fields'

      expect(refused(list)).toMatchObject({ span: at(list, '- a\n- b'), message })
      expect(refused(scalar)).toMatchObject({ span: at(scalar, 'just words'), message })
    })

    it('should report a key that is not a string, at the key', () => {
      const number = '---\n2024: x\n---\n'
      const nested = '---\nhero:\n  [a]: x\n---\n'
      const message = 'a front-matter key must be a string'

      expect(refused(number)).toMatchObject({ span: at(number, '2024'), message })
      expect(refused(nested)).toMatchObject({ span: at(nested, '[a]'), message })
    })

    it('should report an alias, where an anchor is no shorthand the core can load', () => {
      const text = '---\nhero: &h x\nalt: *h\n---\n'

      expect(refused(text)).toMatchObject({
        span: at(text, '*h'),
        message: 'front matter cannot use an alias; write the value out',
      })
    })

    it('should report a number JSON cannot carry', () => {
      const text = '---\nscale: [1, .inf]\n---\n'

      expect(refused(text)).toMatchObject({
        span: at(text, '.inf'),
        message: 'front matter cannot carry Infinity: not a finite number',
      })
    })

    it('should report an integer that would load rounded, at its literal', () => {
      const decimal = '---\nid: 9007199254740993\n---\n'
      const hex = '---\nid: 0x20000000000001\n---\n'
      const tooLarge = 'an integer this large loses precision'

      expect(refused(decimal)).toMatchObject({
        span: at(decimal, '9007199254740993'),
        message: `front matter cannot carry 9007199254740993: ${tooLarge}`,
      })
      expect(refused(hex)).toMatchObject({
        span: at(hex, '0x20000000000001'),
        message: `front matter cannot carry 0x20000000000001: ${tooLarge}`,
      })
    })

    it('should load a large float, a safe integer, and a quoted large integer', () => {
      const text =
        '---\na: 1e20\nb: 9007199254740991\nc: "9007199254740993"\nd: 0o777\n---\n'

      expect(page(text).loaded.value).toMatchObject({
        a: 1e20,
        b: 9007199254740991,
        c: '9007199254740993',
        d: 511,
      })
    })

    it('should report everything it finds, in the order it stands in the file', () => {
      const text = '---\n1: a\nb: !!timestamp x\nc: *d\n---\n'
      const read = readPage('post.md', text)

      expect(read.diagnostics.map((d) => [d.severity, d.span])).toEqual([
        ['error', at(text, '1')],
        ['warning', at(text, '!!timestamp')],
        ['error', at(text, '*d')],
      ])
    })
  })

  describe('a parser position beside an emoji', () => {
    it('should widen an error that stops inside an emoji to the whole emoji', () => {
      const text = '---\ntitle: 🎉 Release: v2 is out\n---\n'

      expect(refused(text)).toMatchObject({
        span: at(text, 'title: «🎉» Release'),
        message:
          'front matter is not valid YAML: ' +
          'Nested mappings are not allowed in compact mappings',
      })
    })

    it('should warn at a tag on an emoji, keeping the value', () => {
      const text = '---\na: !foo 😀\n---\n'
      const read = readPage('post.md', text)

      expect(read.diagnostics).toMatchObject([
        { span: at(text, '!foo'), severity: 'warning', message: 'Unresolved tag: !foo' },
      ])
      expect(read.page?.loaded.value.a).toBe('😀')
    })

    it('should widen an error that stops inside a tag’s emoji', () => {
      const text = '---\na: !😀 x\n---\n'
      const read = readPage('post.md', text)

      expect(read.diagnostics.map((d) => covered(text, d.span))).toEqual(['😀'])
    })
  })

  describe('a tag the core schema does not know', () => {
    it('should warn at the tag and keep the page, reading the value as written', () => {
      const text = '---\ntitle: x\nwhen: !!timestamp 2024-11-24\n---\n'
      const read = readPage('post.md', text)

      expect(read.diagnostics).toEqual([
        {
          path: 'post.md',
          span: at(text, '!!timestamp'),
          severity: 'warning',
          message: 'Unresolved tag: tag:yaml.org,2002:timestamp',
          labels: [],
        },
      ])
      expect(read.page?.loaded.value.when).toBe('2024-11-24')
    })
  })
})

describe('derivedUrl()', () => {
  it('should place an index file at its directory', () => {
    expect(derivedUrl('feed/the-rebound/index.md')).toBe('/feed/the-rebound/')
    expect(derivedUrl('feed/index.lem')).toBe('/feed/')
    expect(derivedUrl('index.md')).toBe('/')
  })

  it('should place any other file at a directory of its own name', () => {
    expect(derivedUrl('tils/git/foo.md')).toBe('/tils/git/foo/')
    expect(derivedUrl('about.lem')).toBe('/about/')
  })

  it('should percent-encode `#`, `?` and `%`, leaving the rest to the core', () => {
    expect(derivedUrl('a#b.md')).toBe('/a%23b/')
    expect(derivedUrl('what?.md')).toBe('/what%3F/')
    expect(derivedUrl('100%/index.md')).toBe('/100%25/')
    expect(derivedUrl('c# notes/café.md')).toBe('/c%23 notes/café/')
  })

  it('should take off only the last extension, and not a dot in a directory name', () => {
    expect(derivedUrl('feed.xml.lem')).toBe('/feed.xml/')
    expect(derivedUrl('v1.2/notes.md')).toBe('/v1.2/notes/')
  })
})
