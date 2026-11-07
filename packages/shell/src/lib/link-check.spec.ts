import type { Diagnostic, Span } from '@limulus/lemmon-core'
import { describe, expect, it } from 'vitest'

import { checkLinks, type LinkCheck } from './link-check.ts'
import type { Generated } from './plugins.ts'
import { FILE_START } from './program.ts'
import { OutputFiles, outputFile, type Output, type Writer } from './write.ts'
import { at } from '../mocks/spans.ts'

const ORIGIN = 'https://limulus.net'
const CONFIG = '/site/lemmon.config.ts'

/** Where a page at `url` stands: its front matter's `url` key, `url: /a/`. */
const urlKey = (url: string): Span => at(`---\nurl: ${url}\n---\n`, `url: ${url}`)

/** A producer's mark, where the file it wrote stands. */
const MARK = at('<lem.mark("styles")>', 'mark("styles")')

/**
 * A build's output in memory, as the check is given it: each file claimed by its writer,
 * in the order added, and the bytes of each but a passthrough file.
 */
class Site {
  readonly files = new OutputFiles()
  readonly outputs: Output[] = []

  /** A page `path` writes at `url`, as `text`: one of a generator's where `generated`. */
  page(path: string, url: string, text: string, generated?: Generated): this {
    return this.#write(url, text, { path, span: urlKey(url), generated })
  }

  /** A file the `plugin` plugin's asset hook built at `url`. */
  asset(url: string, text: string, plugin = 'scripts'): this {
    return this.#write(url, text, { path: CONFIG, assets: { plugin } })
  }

  /** A file a producer of `plugin` wrote at `url` for `page`, or for its body. */
  produced(url: string, text: string, page: string, body?: boolean): this {
    const producer = { plugin: 'bundler', page, body }
    return this.#write(url, text, { path: 'layouts.lem', span: MARK, producer })
  }

  /** A passthrough file, which nothing scans. */
  passthrough(path: string): this {
    expect(this.files.claim(path, { path })).toBeUndefined()
    return this
  }

  /** What the check reports over this site, with the config's say. */
  checked(config: Partial<LinkCheck> = {}): Diagnostic[] {
    const { files, outputs } = this
    return checkLinks({
      outputs,
      files,
      manifest: {},
      mappedBy: new Map(),
      origin: ORIGIN,
      served: [],
      config: CONFIG,
      ...config,
    })
  }

  /** The messages the check reports over this site. */
  messages(config: Partial<LinkCheck> = {}): string[] {
    return this.checked(config).map((diagnostic) => diagnostic.message)
  }

  #write(url: string, text: string, writer: Writer): this {
    const file = outputFile(url)!
    expect(this.files.claim(file, writer)).toBeUndefined()
    this.outputs.push({ file, url, bytes: new TextEncoder().encode(text) })
    return this
  }
}

/** A site of one page, `a.md` at `/a/`, holding `html`, beside `b.md` at `/b/`. */
const linking = (html: string) =>
  new Site().page('a.md', '/a/', html).page('b.md', '/b/', '<h2 id="intro">B</h2>')

describe('checkLinks()', () => {
  describe('a site whose every link names a file it writes', () => {
    it('should find nothing to report', () => {
      const site = new Site()
        .page(
          'a.md',
          '/a/',
          '<link rel="stylesheet" href="/s.css"><script src="/app.js"></script>' +
            '<a href="../b/">b</a> <a href="/b/#intro">intro</a> <a href="#top">top</a> ' +
            '<a href="#notes">notes</a> <a href="https://limulus.net/b/">b</a> ' +
            '<a href="./">self</a> <img src="/photo.jpg"> <h2 id="notes">Notes</h2>'
        )
        .page('b.md', '/b/', '<h2 id="intro">B</h2>')
        .asset('/app.js', 'run()')
        .produced('/s.css', 'a { b: url(/photo.jpg) }', 'a.md')
        .passthrough('photo.jpg')

      expect(site.checked()).toEqual([])
    })
  })

  describe('a link that names no file', () => {
    it('should be an error at the page’s url, naming the file its url names', () => {
      const site = linking('<p>A <a href="link">wonderful essay</a></p>')

      expect(site.checked()).toEqual([
        {
          path: 'a.md',
          span: urlKey('/a/'),
          severity: 'error',
          message:
            '`a.md` links to "link" in `<a href>`, which names `a/link`, where the build ' +
            'writes no file',
          labels: [],
        },
      ])
    })

    it('should name the path where it names no file the build could write', () => {
      const site = linking('<a href="/b//">x</a> <a href="/c%2Fd/">y</a>')

      expect(site.messages()).toEqual([
        '`a.md` links to "/b//" in `<a href>`, which names `/b//`, where the build writes ' +
          'no file',
        '`a.md` links to "/c%2Fd/" in `<a href>`, which names `/c%2Fd/`, where the build ' +
          'writes no file',
      ])
    })

    it('should say where the build writes the `index.html` inside it, labelled at its writer', () => {
      const site = linking('<a href="https://limulus.net/b">b</a>')

      expect(site.checked()).toEqual([
        expect.objectContaining({
          message:
            '`a.md` links to "https://limulus.net/b" in `<a href>`, which names `b`, where ' +
            'the build writes no file; it writes `b/index.html`, at `/b/`',
          labels: [
            { path: 'b.md', span: urlKey('/b/'), message: 'the writer of `b/index.html`' },
          ],
        }),
      ])
    })

    it('should not say so of an `index.html` inside it that differs but for case', () => {
      const site = linking('<a href="/B">b</a>')

      expect(site.checked()).toEqual([
        expect.objectContaining({
          message:
            '`a.md` links to "/B" in `<a href>`, which names `B`, where the build writes no file',
          labels: [],
        }),
      ])
    })

    it('should name a file that differs only in case or Unicode form, labelled at its writer', () => {
      const site = linking('<a href="/B/">b</a> <a href="/cafe\u0301/">c</a>')
        .page('café.md', '/caf%C3%A9/', 'Café')
        .page('c.md', '/c/', '<img src="/Photo.JPG">')
        .passthrough('photo.jpg')

      expect(site.checked()).toEqual([
        expect.objectContaining({
          message:
            '`a.md` links to "/B/" in `<a href>`, which names `B/index.html`: the build ' +
            'writes `b/index.html`, which differs from it only in case or Unicode form, and ' +
            'a host that matches names exactly serves nothing there',
          labels: [
            { path: 'b.md', span: urlKey('/b/'), message: 'the writer of `b/index.html`' },
          ],
        }),
        expect.objectContaining({
          message: expect.stringMatching(
            /^`a.md` links to "\/cafe\u0301\/" in `<a href>`, which names `cafe\u0301\/index.html`: the build writes `café\/index.html`, which differs/
          ) as string,
          labels: [
            {
              path: 'café.md',
              span: urlKey('/caf%C3%A9/'),
              message: 'the writer of `café/index.html`',
            },
          ],
        }),
        // A passthrough file is labelled at its start.
        expect.objectContaining({
          path: 'c.md',
          message: expect.stringMatching(
            /^`c.md` links to "\/Photo.JPG" in `<img src>`, which names `Photo.JPG`: the build writes `photo.jpg`, which differs/
          ) as string,
          labels: [
            { path: 'photo.jpg', span: FILE_START, message: 'the writer of `photo.jpg`' },
          ],
        }),
      ])
    })
  })

  describe('a fragment', () => {
    it('should name an element of the HTML page it reaches, by id or `a` name, decoded', () => {
      const site = linking(
        '<a href="/b/#intro">x</a> <a href="#caf%C3%A9">y</a> <a href="#old">z</a> ' +
          '<p id="café">c</p> <a name="old">o</a>'
      )

      expect(site.checked()).toEqual([])
    })

    it('should be an error where the page has none, labelled at that page', () => {
      const site = linking('<a href="/b/#fn2">x</a>')

      expect(site.checked()).toEqual([
        {
          path: 'a.md',
          span: urlKey('/a/'),
          severity: 'error',
          message:
            '`a.md` links to "/b/#fn2" in `<a href>`, and `b/index.html` has no element ' +
            'whose `id`, or an `a` whose `name`, is `fn2`',
          labels: [
            { path: 'b.md', span: urlKey('/b/'), message: 'the writer of `b/index.html`' },
          ],
        },
      ])
    })

    it('should name the fragment as it reads decoded', () => {
      const site = linking('<a href="#d%C3%A9j%C3%A0-vu">x</a>')

      expect(site.messages()).toEqual([
        '`a.md` links to "#d%C3%A9j%C3%A0-vu" in `<a href>`, and `a/index.html` has no ' +
          'element whose `id`, or an `a` whose `name`, is `déjà-vu`',
      ])
    })

    it('should not be labelled where the page is the link’s own', () => {
      const site = linking('<a href="#fn">x</a>')

      expect(site.checked()).toEqual([
        expect.objectContaining({
          message:
            '`a.md` links to "#fn" in `<a href>`, and `a/index.html` has no element whose ' +
            '`id`, or an `a` whose `name`, is `fn`',
          labels: [],
        }),
      ])
    })

    it('should reach the top where it is empty or `top` in any case, and drop a text directive', () => {
      const site = linking(
        '<a href="#">a</a> <a href="#TOP">b</a> <a href="#To%70">c</a> ' +
          '<a href="/b/#:~:text=B">d</a> <a href="/b/#intro:~:text=B">e</a> ' +
          '<a href="/b/#missing:~:text=B">f</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "/b/#missing:~:text=B" in `<a href>`, and `b/index.html` has no ' +
          'element whose `id`, or an `a` whose `name`, is `missing`',
      ])
    })

    it('should not reach an id inside a `<template>`, which no document holds', () => {
      const site = linking('<a href="#t">x</a> <template><p id="t">t</p></template>')

      expect(site.messages()).toEqual([
        '`a.md` links to "#t" in `<a href>`, and `a/index.html` has no element whose `id`, ' +
          'or an `a` whose `name`, is `t`',
      ])
    })

    it('should not be checked into any file but an HTML page the build writes', () => {
      const site = linking(
        '<a href="/feed.xml#a">a</a> <a href="/old.html#b">b</a> <a href="/app.js#c">c</a> ' +
          '<a href="/@eric#d">d</a> <a href="/b/#intro">e</a>'
      )
        .page('feed.lem', '/feed.xml', '<feed/>')
        .asset('/app.js', 'run()')
        .passthrough('old.html')

      expect(site.checked({ served: ['/@eric'] })).toEqual([])
    })
  })

  describe('what it skips', () => {
    it('should check the site’s own absolute links, on either scheme, and skip every other host’s', () => {
      const site = linking(
        '<a href="https://limulus.net/x/">a</a> <a href="//limulus.net/y/">b</a> ' +
          '<a href="http://limulus.net/z/">c</a> <a href="https://www.limulus.net/z/">d</a> ' +
          '<a href="https://limulus.net:8443/z/">e</a> <a href="https://example.com/z/">f</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "https://limulus.net/x/" in `<a href>`, which names `x/index.html`, ' +
          'where the build writes no file',
        '`a.md` links to "//limulus.net/y/" in `<a href>`, which names `y/index.html`, where ' +
          'the build writes no file',
        '`a.md` links to "http://limulus.net/z/" in `<a href>`, which names `z/index.html`, ' +
          'where the build writes no file',
      ])
    })

    it('should take every absolute link as another origin’s where the site names none', () => {
      const site = linking('<a href="https://limulus.net/x/">a</a> <a href="/y/">b</a>')

      expect(site.messages({ origin: undefined })).toEqual([
        '`a.md` links to "/y/" in `<a href>`, which names `y/index.html`, where the build ' +
          'writes no file',
      ])
    })

    it('should skip another scheme, and a url the URL parser refuses, and check the rest', () => {
      const site = linking(
        '<a href="mailto:eric@limulus.net">a</a> <a href="tel:+15555550123">b</a> ' +
          '<img src="data:image/gif;base64,R0lGODlhAQABAAAAACw="> ' +
          '<a href="javascript:void(0)">c</a> <a href="blob:https://limulus.net/x">d</a> ' +
          '<a href="http://exa mple.com/">e</a> <a href="/missing">f</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "/missing" in `<a href>`, which names `missing`, where the build ' +
          'writes no file',
      ])
    })

    it('should take a path `served` lists, matched exactly, and no other', () => {
      const site = linking(
        '<a href="/@eric">a</a> <a href="https://limulus.net/@eric?x=1#y">b</a> ' +
          '<a href="/@eric/">c</a> <a href="/@eric/posts">d</a> <a href="/@Eric">e</a>'
      )

      expect(site.messages({ served: ['/@eric'] })).toEqual([
        '`a.md` links to "/@eric/" in `<a href>`, which names `@eric/index.html`, where the ' +
          'build writes no file',
        '`a.md` links to "/@eric/posts" in `<a href>`, which names `@eric/posts`, where the ' +
          'build writes no file',
        '`a.md` links to "/@Eric" in `<a href>`, which names `@Eric`, where the build writes ' +
          'no file',
      ])
    })

    it('should read a `ping`’s urls as HTML separates them, at whitespace', () => {
      const site = linking(
        '<a href="/b/" ping="/b/ /a/">x</a> <a href="/b/" ping="/b/,/a/">y</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "/b/,/a/" in `<a ping>`, which names `b/,/a/index.html`, where the ' +
          'build writes no file',
      ])
    })

    it('should match `served` against the path as the URL parser writes it', () => {
      const site = linking('<a href="/café">a</a> <a href="/caf%C3%A9">b</a>')

      expect(site.messages({ served: ['/caf%C3%A9'] })).toEqual([])
    })
  })

  describe('`served`', () => {
    it('should warn at a path the build writes, whose links and fragments it then skips', () => {
      const site = linking('<a href="/b/#missing">x</a> <a href="/@eric">y</a>')

      expect(site.checked({ served: ['/b/', '/B/', '/@eric', '/a//b'] })).toEqual([
        {
          path: CONFIG,
          span: FILE_START,
          severity: 'warning',
          message:
            '`served` lists `/b/`, where the build writes `b/index.html`, so a link there is ' +
            'not checked, nor a fragment into it: `served` is for the paths the host ' +
            'answers without a file',
          labels: [
            { path: 'b.md', span: urlKey('/b/'), message: 'the writer of `b/index.html`' },
          ],
        },
      ])
    })
  })

  describe('its diagnostics', () => {
    it('should report a url once per page, however it is written, naming each place it stands', () => {
      const site = linking(
        '<a href="/x/">1</a> <a href="https://limulus.net/x/">2</a> <a href="../x/">3</a> ' +
          '<a href="/x/?v=1">4</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "/x/", "https://limulus.net/x/", and "../x/" in `<a href>` 3 times, ' +
          'which names `x/index.html`, where the build writes no file',
        '`a.md` links to "/x/?v=1" in `<a href>`, which names `x/index.html`, where the build ' +
          'writes no file',
      ])
    })

    it('should report one url written alike apart where its bases differ', () => {
      const feed =
        '<rss><channel xml:base="https://limulus.net/a/"><link>x/</link></channel>' +
        '<channel xml:base="https://limulus.net/b/"><link>x/</link></channel></rss>'
      const site = new Site().page('feed.lem', '/feed.xml', feed)

      expect(site.messages()).toEqual([
        '`feed.lem` links to "x/" in `<link>`, which names `a/x/index.html`, where the build ' +
          'writes no file',
        '`feed.lem` links to "x/" in `<link>`, which names `b/x/index.html`, where the build ' +
          'writes no file',
      ])
    })

    it('should report a url once per page, naming each place it stands and how often', () => {
      const site = linking(
        '<a href="/x/">1</a> <link href="/x/"> <a href=" /x/ ">2</a> <a href="/x/">3</a> ' +
          '<img src="/y.jpg" srcset="/y.jpg 1x, /y.jpg 2x"> <a href="/b/">ok</a> ' +
          '<a href="/b/">ok</a>'
      )

      expect(site.messages()).toEqual([
        '`a.md` links to "/x/" in `<a href>` 3 times and `<link href>`, which names ' +
          '`x/index.html`, where the build writes no file',
        '`a.md` links to "/y.jpg" in `<img src>` and `<img srcset>` twice, which names ' +
          '`y.jpg`, where the build writes no file',
      ])
    })

    it('should report a url again on each page that writes it, page by page', () => {
      const site = linking('<a href="/x/">x</a>').page('c.md', '/c/', '<a href="/x/">x</a>')

      expect(site.checked().map(({ path, span }) => [path, span])).toEqual([
        ['a.md', urlKey('/a/')],
        ['c.md', urlKey('/c/')],
      ])
    })

    it('should name a generated page by its item, where its writer stands', () => {
      const site = new Site().page('topics.lem', '/tils/git/', '<a href="/x/">x</a>', {
        list: 'site.topics',
        index: 0,
      })

      expect(site.checked()).toEqual([
        expect.objectContaining({
          path: 'topics.lem',
          span: urlKey('/tils/git/'),
          message:
            '`topics.lem` for `site.topics[0]` links to "/x/" in `<a href>`, which names ' +
            '`x/index.html`, where the build writes no file',
        }),
      ])
    })

    it('should name a producer’s file by the page, or the body, it was written for, at its mark', () => {
      const site = new Site()
        .produced('/a.css', 'a { b: url(x.png) }', 'a.md')
        .produced('/b.css', '@import "c.css";', 'b.md', true)

      expect(site.checked()).toEqual([
        {
          path: 'layouts.lem',
          span: MARK,
          severity: 'error',
          message:
            '`a.css`, which the `bundler` plugin writes for `a.md`, links to "x.png" in ' +
            '`url()`, which names `x.png`, where the build writes no file',
          labels: [],
        },
        expect.objectContaining({
          message:
            '`b.css`, which the `bundler` plugin writes for the body of `b.md`, links to ' +
            '"c.css" in `@import`, which names `c.css`, where the build writes no file',
        }),
      ])
    })

    it('should name an asset hook’s file by its plugin, at the config’s start', () => {
      const site = new Site().asset('/lib/x.css', 'a { b: url(./y.png) }', 'esbuild')

      expect(site.checked()).toEqual([
        {
          path: CONFIG,
          span: FILE_START,
          severity: 'error',
          message:
            "`lib/x.css`, which the `esbuild` plugin's `assets` hook writes, links to " +
            '"./y.png" in `url()`, which names `lib/y.png`, where the build writes no file',
          labels: [],
        },
      ])
    })
  })

  describe('what it reads', () => {
    it('should resolve a stylesheet’s urls against its own url', () => {
      const site = new Site()
        .produced('/assets/bundle/a.css', 'a { b: url(../fonts/f.woff2) }', 'a.md')
        .passthrough('assets/fonts/f.woff2')
        .produced('/assets/bundle/b.css', 'a { b: url(f.woff2) }', 'a.md')

      expect(site.messages()).toEqual([
        '`assets/bundle/b.css`, which the `bundler` plugin writes for `a.md`, links to ' +
          '"f.woff2" in `url()`, which names `assets/bundle/f.woff2`, where the build writes ' +
          'no file',
      ])
    })

    it('should read an XML page’s links against its own url, or its `xml:base`', () => {
      const feed =
        '<?xml-stylesheet href="/feed.xsl" type="text/xsl"?>' +
        '<feed xmlns="http://www.w3.org/2005/Atom"><id>https://limulus.net/tils</id>' +
        '<link href="https://limulus.net/tils"/><entry xml:base="/b/"><link href="./"/>' +
        '</entry></feed>'
      const site = linking('')
        .page('feed.lem', '/tils/feed.xml', feed)
        .passthrough('feed.xsl')

      expect(site.messages()).toEqual([
        '`feed.lem` links to "https://limulus.net/tils" in `<link href>`, which names ' +
          '`tils`, where the build writes no file',
      ])
    })

    it('should refuse an XML page the XML parser refuses, at its writer', () => {
      const site = new Site().page('feed.lem', '/feed.xml', '<feed><entry></feed>')

      expect(site.checked()).toEqual([
        {
          path: 'feed.lem',
          span: urlKey('/feed.xml'),
          severity: 'error',
          message:
            '`feed.xml`, which `feed.lem` writes, is refused by the XML parser (Opening and ' +
            'ending tag mismatch: "entry" != "feed"), so no link in it can be read',
          labels: [],
        },
      ])
    })

    it('should refuse an XML page that declares an encoding the check does not read', () => {
      const site = new Site().page(
        'feed.lem',
        '/feed.xml',
        '<?xml version="1.0" encoding="ISO-8859-1"?><rss/>'
      )

      expect(site.messages()).toEqual([
        '`feed.xml`, which `feed.lem` writes, declares the encoding `ISO-8859-1`, where the ' +
          'check reads UTF-8, so no link in it can be read',
      ])
    })

    it('should name a plugin’s refused XML file by its plugin', () => {
      const site = new Site()
        .asset('/a.xml', '<a>', 'icons')
        .produced('/b.xml', '<b>', 'b.md')

      expect(site.messages()).toEqual([
        "`a.xml`, which the `icons` plugin's `assets` hook writes, is refused by the XML " +
          'parser (unclosed xml tag(s): a), so no link in it can be read',
        '`b.xml`, which the `bundler` plugin writes for `b.md`, is refused by the XML parser ' +
          '(unclosed xml tag(s): b), so no link in it can be read',
      ])
    })

    it('should refuse a `srcset` the parser refuses once per page per place and value, at its writer, and check the rest', () => {
      const refused = '<img srcset="/missing.jpg 1x, /b.jpg 2q">'
      const site = linking(
        `${refused}<p>${refused}</p><source srcset="/missing.jpg 1x, /b.jpg 2q">` +
          '<img srcset="/c.jpg 0w" src="/d.jpg">'
      ).page('c.md', '/c/', refused)
      const error = (path: string, url: string, at: string, srcset: string) => ({
        path,
        span: urlKey(url),
        severity: 'error',
        message:
          `\`${path}\` holds \`${at}\` as ${JSON.stringify(srcset)}, where a candidate's ` +
          'descriptors do not parse, so none of its links can be checked',
        labels: [],
      })

      expect(site.checked()).toEqual([
        error('a.md', '/a/', '<img srcset>', '/missing.jpg 1x, /b.jpg 2q'),
        error('a.md', '/a/', '<source srcset>', '/missing.jpg 1x, /b.jpg 2q'),
        error('a.md', '/a/', '<img srcset>', '/c.jpg 0w'),
        expect.objectContaining({
          path: 'a.md',
          message: expect.stringMatching(/"\/d.jpg"/),
        }),
        error('c.md', '/c/', '<img srcset>', '/missing.jpg 1x, /b.jpg 2q'),
      ])
    })

    it('should read a file by its extension, in any case, and no other kind', () => {
      const site = new Site()
        .page('a.md', '/A.HTML', '<a href="/x/">x</a>')
        .asset('/app.js', 'fetch("/missing")')
        .page('robots.lem', '/robots.txt', 'Sitemap: https://limulus.net/missing.xml')
        .produced('/B.Css', 'a { b: url(/y.png) }', 'a.md')

      expect(site.messages()).toEqual([
        '`a.md` links to "/x/" in `<a href>`, which names `x/index.html`, where the build ' +
          'writes no file',
        '`B.Css`, which the `bundler` plugin writes for `a.md`, links to "/y.png" in ' +
          '`url()`, which names `y.png`, where the build writes no file',
      ])
    })
  })

  describe('the manifest', () => {
    const site = () => new Site().asset('/lib/app.1a2b.js', 'run()').page('b.md', '/b/', '')

    it('should check each url an asset hook mapped, at the config’s start, naming the plugin', () => {
      const checked = site().checked({
        manifest: { 'lib/app.js': '/lib/app.1a2b.js', 'lib/x.js': '/lib/x.9z9z.js' },
        mappedBy: new Map([
          ['lib/app.js', 'scripts'],
          ['lib/x.js', 'esbuild'],
        ]),
      })

      expect(checked).toEqual([
        {
          path: CONFIG,
          span: FILE_START,
          severity: 'error',
          message:
            'the `esbuild` plugin\'s `assets` hook maps `lib/x.js` to "/lib/x.9z9z.js", ' +
            'which names `lib/x.9z9z.js`, where the build writes no file',
          labels: [],
        },
      ])
    })

    it('should say a url it maps to no file once, and not again at each page that prints it', () => {
      const printing = '<script type="module" src="/lib/x.9z9z.js"></script>'
      const checked = new Site()
        .page('a.md', '/a/', `${printing}<a href="/missing/">m</a>`)
        .page('b.md', '/b/', `${printing}<script src="https://limulus.net/lib/x.9z9z.js">`)
        .checked({
          manifest: { 'lib/x.js': '/lib/x.9z9z.js' },
          mappedBy: new Map([['lib/x.js', 'esbuild']]),
        })

      expect(
        checked.map(({ path, message }) => [path, message.split(', which')[0]])
      ).toEqual([
        [
          CONFIG,
          'the `esbuild` plugin\'s `assets` hook maps `lib/x.js` to "/lib/x.9z9z.js"',
        ],
        ['a.md', '`a.md` links to "/missing/" in `<a href>`'],
      ])
    })

    it('should skip a url on another origin, and label a near miss', () => {
      const messages = site().checked({
        manifest: { 'lib/cdn.js': 'https://cdn.example/x.js', 'b.html': '/B/' },
        mappedBy: new Map([
          ['lib/cdn.js', 'cdn'],
          ['b.html', 'pages'],
        ]),
      })

      expect(messages).toEqual([
        expect.objectContaining({
          message:
            'the `pages` plugin\'s `assets` hook maps `b.html` to "/B/", which names ' +
            '`B/index.html`: the build writes `b/index.html`, which differs from it only in ' +
            'case or Unicode form, and a host that matches names exactly serves nothing there',
          labels: [
            { path: 'b.md', span: urlKey('/b/'), message: 'the writer of `b/index.html`' },
          ],
        }),
      ])
    })
  })
})
