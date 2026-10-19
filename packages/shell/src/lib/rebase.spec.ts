import { describe, expect, it, vi } from 'vitest'

import { rebaseLinks } from './rebase.ts'

const ORIGIN = 'https://limulus.net'
const PAGE = '/feed/the-rebound/'

/** `body` rebased as the body of {@link PAGE} on {@link ORIGIN}. */
const absolute = (body: string) => rebaseLinks(body, PAGE, ORIGIN).body

/** `body` rebased as the body of {@link PAGE}, where the site names no origin. */
const rooted = (body: string) => rebaseLinks(body, PAGE).body

/** The value `body`'s one `href` holds once rebased with an origin. */
const href = (value: string) => absolute(`<a href="${value}">x</a>`)

describe('rebaseLinks()', () => {
  describe('with an origin', () => {
    it('should resolve a relative url against the page’s url', () => {
      expect(href('../tuples/#a')).toBe(
        '<a href="https://limulus.net/feed/tuples/#a">x</a>'
      )
      expect(href('./x.pdf')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/x.pdf">x</a>'
      )
      expect(href('x/')).toBe('<a href="https://limulus.net/feed/the-rebound/x/">x</a>')
    })

    it('should resolve a fragment, and an empty url, to the page itself', () => {
      expect(href('#fn1')).toBe('<a href="https://limulus.net/feed/the-rebound/#fn1">x</a>')
      expect(href('')).toBe('<a href="https://limulus.net/feed/the-rebound/">x</a>')
      expect(absolute('<a href>x</a>')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/">x</a>'
      )
    })

    it('should keep a relative url’s query, which Eleventy drops', () => {
      expect(href('?page=2')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/?page=2">x</a>'
      )
      expect(href('../?q=1#top')).toBe('<a href="https://limulus.net/feed/?q=1#top">x</a>')
    })

    it('should put a path from `/`, or from `//`, on the origin', () => {
      expect(href('/photos/')).toBe('<a href="https://limulus.net/photos/">x</a>')
      expect(href('//pho.limulus.net/a.jpg')).toBe(
        '<a href="https://pho.limulus.net/a.jpg">x</a>'
      )
    })

    it('should leave a url that parses on its own byte for byte', () => {
      for (const url of [
        'https://example.com/a?b#c',
        'mailto:eric@limulus.net',
        'data:image/png;base64,AAAA',
        'tel:+15555550123',
        'HTTPS://Example.COM',
      ]) {
        expect(href(url)).toBe(`<a href="${url}">x</a>`)
      }
    })

    it('should trim a url, as Eleventy does, before it rebases or keeps it', () => {
      expect(href(' /x ')).toBe('<a href="https://limulus.net/x">x</a>')
      expect(href('\n https://example.com/ ')).toBe('<a href="https://example.com/">x</a>')
    })

    it('should rebase every attribute posthtml-urls rebases for Eleventy', () => {
      const table: [string, string][] = [
        ['a', 'href'],
        ['area', 'href'],
        ['audio', 'src'],
        ['base', 'href'],
        ['blockquote', 'cite'],
        ['button', 'formaction'],
        ['del', 'cite'],
        ['embed', 'src'],
        ['form', 'action'],
        ['iframe', 'src'],
        ['img', 'src'],
        ['input', 'formaction'],
        ['input', 'src'],
        ['ins', 'cite'],
        ['link', 'href'],
        ['object', 'data'],
        ['q', 'cite'],
        ['script', 'src'],
        ['source', 'src'],
        ['track', 'src'],
        ['video', 'poster'],
        ['video', 'src'],
      ]
      for (const [tag, name] of table) {
        // A video's `source` is in a `video`, a track's too; each is reached all the same.
        const body = `<div><${tag} ${name}="x.bin" title="./t"></${tag}></div>`
        expect(absolute(body), `${tag} ${name}`).toBe(
          `<div><${tag} ${name}="https://limulus.net/feed/the-rebound/x.bin" ` +
            `title="./t"></${tag}></div>`
        )
      }
    })

    it('should leave what is no link in its table: another attribute, a custom element, a meta refresh', () => {
      const body =
        '<a title="./x" data-href="./y">a</a><x-link href="./z"></x-link>' +
        '<meta http-equiv="refresh" content="0; url=./w">'

      expect(absolute(body)).toBe(body)
    })

    it('should rebase inside a template, an SVG, and a noscript', () => {
      expect(absolute('<template><a href="./t">t</a></template>')).toBe(
        '<template><a href="https://limulus.net/feed/the-rebound/t">t</a></template>'
      )
      expect(absolute('<svg><a href="#s"><text>s</text></a></svg>')).toBe(
        '<svg><a href="https://limulus.net/feed/the-rebound/#s"><text>s</text></a></svg>'
      )
      expect(absolute('<noscript><img src="./p.png"></noscript>')).toBe(
        '<noscript><img src="https://limulus.net/feed/the-rebound/p.png"></noscript>'
      )
    })

    it('should rebase a body nested deeper than the call stack goes, in document order', () => {
      const depth = 20_000
      const body = `${'<div>'.repeat(depth)}<a href="x">x</a><a href="http://exa mple/">y</a>`
      const { body: rebased, unrebased } = rebaseLinks(
        `${body}<a href="http://exa mple/z">z</a>`,
        PAGE,
        ORIGIN
      )

      expect(rebased).toContain(
        '<a href="https://limulus.net/feed/the-rebound/x">x</a><a href="http://exa mple/">'
      )
      expect(unrebased).toEqual([
        { kind: 'refused', at: '<a href>', url: 'http://exa mple/' },
        { kind: 'refused', at: '<a href>', url: 'http://exa mple/z' },
      ])
    })

    it('should write an attribute it rebases double-quoted, escaped, by its name in lower case', () => {
      expect(absolute("<a HREF='?a=1&amp;b=2'>x</a>")).toBe(
        '<a href="https://limulus.net/feed/the-rebound/?a=1&amp;b=2">x</a>'
      )
      expect(absolute('<a href=./y>y</a>')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/y">y</a>'
      )
      expect(absolute('<a href=\'#"q"\'>q</a>')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/#%22q%22">q</a>'
      )
      expect(absolute('<a href="mailto:a@b?subject=&quot;hi&quot;">m</a>')).toBe(
        '<a href="mailto:a@b?subject=&quot;hi&quot;">m</a>'
      )
    })

    it('should leave every other byte of the body as it was', () => {
      const before =
        '<h2 id="x">Café &amp; ’</h2>\r\n<!-- <a href="./c"> -->\n' +
        '<pre><code>&lt;a href="./code"&gt;</code></pre>\n<p>Read <a class="k"\n' +
        '   href="../tuples/">this</a>.</p>\n<script>let a = "<a href=./s>"</script>\n' +
        '<p><a href="#fn1" id="r1">1</a></p>\n'
      const after = before
        .replace('href="../tuples/"', 'href="https://limulus.net/feed/tuples/"')
        .replace('href="#fn1"', 'href="https://limulus.net/feed/the-rebound/#fn1"')

      expect(absolute(before)).toBe(after)
    })

    it('should rebase an attribute once where the parser copies its element', () => {
      // The `a` is reopened inside the `i` the `b` closes around, carrying its location;
      // and made again inside the `p` it is closed across, with none.
      expect(absolute('<b><a href="./u"><i>q</b>r</i></a>')).toBe(
        '<b><a href="https://limulus.net/feed/the-rebound/u"><i>q</b>r</i></a>'
      )
      expect(absolute('<a href="./v"><p>y</a>z</p>')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/v"><p>y</a>z</p>'
      )
    })

    describe('a `srcset`', () => {
      it('should leave one whose every candidate parses on its own as written', () => {
        const body =
          '<img srcset="https://pho.limulus.net/a.jpg 1x,https://pho.limulus.net/b.jpg  2x">'

        expect(absolute(body)).toBe(body)
      })

      it('should drop a candidate whose descriptors do not parse from one it rewrites, as Eleventy does, and say so', () => {
        const log = vi.spyOn(console, 'log')
        const srcset = 'a.jpg 1x, b.jpg 2q'

        expect(rebaseLinks(`<img srcset="${srcset}">`, PAGE, ORIGIN)).toEqual({
          body: '<img srcset="https://limulus.net/feed/the-rebound/a.jpg 1x">',
          unrebased: [{ kind: 'dropped', at: '<img srcset>', srcset, descriptor: '2q' }],
        })
        // What `parse-srcset` would have printed is said here instead.
        expect(log).not.toHaveBeenCalled()
        log.mockRestore()
      })

      it('should leave one it does not rewrite as written, a candidate that does not parse and all', () => {
        const body = '<img srcset="https://x/a.jpg 1x, https://x/b.jpg 2q">'
        const log = console.log

        expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({ body, unrebased: [] })
        // Taken over only while `parse-srcset` runs.
        expect(console.log).toBe(log)
      })

      it('should rebase each candidate where one is relative, joined as Eleventy joins them', () => {
        expect(absolute('<img srcset="a.jpg 1x,https://pho.limulus.net/b.jpg 2x">')).toBe(
          '<img srcset="https://limulus.net/feed/the-rebound/a.jpg 1x, ' +
            'https://pho.limulus.net/b.jpg 2x">'
        )
        expect(absolute('<source srcset="/s.webp 640w 480h, /l.webp 1280w">')).toBe(
          '<source srcset="https://limulus.net/s.webp 640w 480h, ' +
            'https://limulus.net/l.webp 1280w">'
        )
        expect(absolute('<img srcset="/a.jpg">')).toBe(
          '<img srcset="https://limulus.net/a.jpg">'
        )
      })
    })

    describe('a `ping`', () => {
      it('should rebase each url, joined by commas', () => {
        expect(absolute('<a href="https://x/" ping="/p,  ./q,">x</a>')).toBe(
          '<a href="https://x/" ping="https://limulus.net/p, ' +
            'https://limulus.net/feed/the-rebound/q">x</a>'
        )
      })

      it('should leave one whose every url parses on its own as written', () => {
        const body = '<a href="https://x/" ping="https://p/,https://q/">x</a>'

        expect(absolute(body)).toBe(body)
      })
    })

    it('should keep a url the URL parser refuses as written, saying so, and rebase the rest', () => {
      const body = '<p><img src=" http://exa mple.com/a.png"><a href="./b">b</a></p>'

      expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({
        body: body.replace('./b', 'https://limulus.net/feed/the-rebound/b'),
        unrebased: [{ kind: 'refused', at: '<img src>', url: 'http://exa mple.com/a.png' }],
      })
    })

    it('should leave a namespaced attribute, SVG’s `xlink:href`, as Eleventy does', () => {
      const alone = '<svg><a xlink:href="#a"><text>t</text></a></svg>'
      const both = '<svg><a href="./y" xlink:href="./x"><text>t</text></a></svg>'

      expect(absolute(alone)).toBe(alone)
      expect(absolute(both)).toBe(
        both.replace('href="./y"', 'href="https://limulus.net/feed/the-rebound/y"')
      )
    })

    it('should take an element named as an object’s own property for no link', () => {
      const body = '<constructor href="./x"></constructor>'

      expect(absolute(body)).toBe(body)
    })

    it('should resolve a url naming a host with backslashes as the URL parser reads it', () => {
      expect(href('\\\\cdn.example/x')).toBe('<a href="https://cdn.example/x">x</a>')
    })
  })

  describe('with no origin', () => {
    it('should make a relative url a path from `/`, its query and fragment kept', () => {
      expect(rooted('<a href="../tuples/#a">x</a>')).toBe('<a href="/feed/tuples/#a">x</a>')
      expect(rooted('<a href="?page=2">x</a>')).toBe(
        '<a href="/feed/the-rebound/?page=2">x</a>'
      )
      expect(rooted('<a href="#fn1">x</a>')).toBe('<a href="/feed/the-rebound/#fn1">x</a>')
      expect(rooted('<img srcset="a.jpg 2x">')).toBe(
        '<img srcset="/feed/the-rebound/a.jpg 2x">'
      )
    })

    it('should leave a url from `/` or `//`, and one that parses on its own', () => {
      const body =
        '<a href="/photos/">p</a><a href="//cdn.example/z">z</a>' +
        '<a href="https://example.com/">e</a>'

      expect(rooted(body)).toBe(body)
    })

    it('should trim a url it leaves', () => {
      expect(rooted('<a href=" /photos/ ">p</a>')).toBe('<a href="/photos/">p</a>')
    })

    it('should keep a url naming a host of its own, with backslashes too, as written', () => {
      const body = '<a href="\\\\cdn.example/x">x</a><a href="\\\\page.invalid/y">y</a>'

      expect(rooted(body)).toBe(body)
    })

    it('should write a path that resolves to start `//` so that it reads as no host', () => {
      expect(rebaseLinks('<a href=".//evil.example/">x</a>', '/').body).toBe(
        '<a href="/.//evil.example/">x</a>'
      )
      expect(rebaseLinks('<a href="../..//x/">x</a>', '/posts/a/').body).toBe(
        '<a href="/.//x/">x</a>'
      )
    })

    it('should keep a url the URL parser refuses as written, saying so', () => {
      const body = '<a href="http://exa mple.com/">x</a>'

      expect(rebaseLinks(body, PAGE)).toEqual({
        body,
        unrebased: [{ kind: 'refused', at: '<a href>', url: 'http://exa mple.com/' }],
      })
    })
  })
})
