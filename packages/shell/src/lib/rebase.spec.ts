import { describe, expect, it } from 'vitest'

import { rebaseLinks } from './rebase.ts'

const ORIGIN = 'https://limulus.net'
const PAGE = '/feed/the-rebound/'

/** `body` rebased as the body of {@link PAGE} on {@link ORIGIN}. */
const absolute = (body: string) => rebaseLinks(body, PAGE, ORIGIN).body

/** `body` rebased as the body of {@link PAGE}, where the site names no origin. */
const rooted = (body: string) => rebaseLinks(body, PAGE).body

/** The value `body`'s one `href` holds once rebased with an origin. */
const href = (value: string) => absolute(`<a href="${value}">x</a>`)

/** `path` on {@link ORIGIN}, from {@link PAGE} where it does not start with `/`. */
const on = (path: string) =>
  path.startsWith('/') ? `${ORIGIN}${path}` : `${ORIGIN}${PAGE}${path}`

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

    it('should keep a relative url’s query', () => {
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

    it('should strip a url of ASCII whitespace, and no other, before it rebases or keeps it', () => {
      expect(href(' /x ')).toBe('<a href="https://limulus.net/x">x</a>')
      expect(href('\n https://example.com/ ')).toBe('<a href="https://example.com/">x</a>')
      expect(href('\u00a0/x')).toBe(
        '<a href="https://limulus.net/feed/the-rebound/%C2%A0/x">x</a>'
      )
    })

    it('should rebase every attribute of the table posthtml-urls walks', () => {
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

    it('should rebase a refreshing `meta`’s url inside its content, and leave the rest', () => {
      expect(absolute(`<meta http-equiv="Refresh" content="0; url='./w' ">`)).toBe(
        `<meta http-equiv="Refresh" content="0; url='https://limulus.net/feed/the-rebound/w' ">`
      )
      const waiting = '<meta http-equiv="refresh" content="30">'

      expect(absolute(waiting)).toBe(waiting)
    })

    it('should percent-encode the quotes of a refresh’s url, which would end it', () => {
      const body = `<meta http-equiv="refresh" content="0; url='./#top'">`

      expect(rebaseLinks(body, "/posts/it's/", ORIGIN).body).toBe(
        `<meta http-equiv="refresh" content="0; url='https://limulus.net/posts/it%27s/#top'">`
      )
      expect(rebaseLinks(body, "/posts/it's/").body).toBe(
        `<meta http-equiv="refresh" content="0; url='/posts/it%27s/#top'">`
      )
      // Kept, as a url that parses on its own, but stripped of the space before it.
      expect(
        absolute(`<meta http-equiv="refresh" content="0; url=' https://x/&quot;'">`)
      ).toBe(`<meta http-equiv="refresh" content="0; url='https://x/%22'">`)
    })

    it('should rebase an Open Graph url’s content', () => {
      expect(absolute('<meta property="og:image" content="./a.jpg">')).toBe(
        '<meta property="og:image" content="https://limulus.net/feed/the-rebound/a.jpg">'
      )
    })

    it('should rebase a `meta`’s content an Open Graph property reads whole as one url, though it refreshes too', () => {
      const both = (content: string) =>
        `<meta http-equiv="refresh" property="og:url" content="${content}">`

      // Its refresh's url, which starts after the time, is in it and goes with it.
      expect(absolute(both('0;./w'))).toBe(both(`${ORIGIN}${PAGE}0;./w`))
    })

    it('should leave what holds no url: another attribute, a custom element, a `meta` that does not refresh, `og:title`', () => {
      const body =
        '<a title="./x" data-href="./y">a</a><x-link href="./z"></x-link>' +
        '<meta http-equiv="content-type" content="./w"><meta name="description" ' +
        'content="./d"><meta property="og:title" content="./t">'

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
      // Spans, since parse5 looks down every open element for a `<p>` to close at each
      // `<div>`: nesting 20,000 of those takes a second, and five under coverage.
      const body = `${'<span>'.repeat(depth)}<a href="x">x</a><a href="http://exa mple/">y</a>`
      const { body: rebased, unrebased } = rebaseLinks(
        `${body}<a href="http://exa mple/z">z</a>`,
        PAGE,
        ORIGIN
      )

      expect(rebased).toContain(
        '<a href="https://limulus.net/feed/the-rebound/x">x</a><a href="http://exa mple/">'
      )
      expect(unrebased).toEqual([
        { at: '<a href>', url: 'http://exa mple/' },
        { at: '<a href>', url: 'http://exa mple/z' },
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

    it('should leave what the check reads but the rebase does not: a `<style>`’s text', () => {
      const body = '<style>p { background: url(./b.png) } @import "./a.css";</style>'

      expect(absolute(body)).toBe(body)
    })

    it('should leave what neither reads: an SVG paint, script text, a CSS comment', () => {
      const body =
        '<svg><rect fill="url(#e)" stroke="url(./s.svg#g)"/></svg>' +
        '<script>fetch("./x.json")</script><p style="/* url(./a.png) */ color: red">p</p>'

      expect(absolute(body)).toBe(body)
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

    it('should say once that it keeps a url the parser copies with its element', () => {
      const body = '<b><a href="http://exa mple/"><i>q</b>r</i></a>'

      expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({
        body,
        unrebased: [{ at: '<a href>', url: 'http://exa mple/' }],
      })
    })

    describe('a `srcset`', () => {
      it('should leave one whose every candidate parses on its own as written', () => {
        const body =
          '<img srcset="https://pho.limulus.net/a.jpg 1x,https://pho.limulus.net/b.jpg  2x">'

        expect(absolute(body)).toBe(body)
      })

      it('should rebase each candidate in place, its descriptors, commas and spacing kept', () => {
        expect(absolute('<img srcset="a.jpg 1x,https://pho.limulus.net/b.jpg 2x">')).toBe(
          `<img srcset="${on('a.jpg')} 1x,https://pho.limulus.net/b.jpg 2x">`
        )
        expect(
          absolute('<source srcset="/s.webp 640w 480h,  /l.webp 1280w,\n /x.webp">')
        ).toBe(
          `<source srcset="${on('/s.webp')} 640w 480h,  ${on('/l.webp')} 1280w,\n ` +
            `${on('/x.webp')}">`
        )
        expect(absolute('<img srcset="/a.jpg">')).toBe(`<img srcset="${on('/a.jpg')}">`)
      })

      it('should read one with its character references decoded, and write it escaped', () => {
        expect(absolute('<img srcset="a.jpg?w=1&amp;h=2 1x,&#10;b.jpg 2x">')).toBe(
          `<img srcset="${on('a.jpg')}?w=1&amp;h=2 1x,\n${on('b.jpg')} 2x">`
        )
      })

      it('should keep one whose candidates the parser refuses byte for byte, saying nothing, which the check reports', () => {
        const body =
          '<img srcset=\'a.jpg 1x, b.jpg 2q\' src=./c.png><img srcset="./d.jpg 0w">'

        expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({
          body: `<img srcset='a.jpg 1x, b.jpg 2q' src="${on('c.png')}"><img srcset="./d.jpg 0w">`,
          unrebased: [],
        })
      })

      it('should leave one of only whitespace and commas, which holds no url', () => {
        const body = '<img srcset=" , " src="./a.png">'

        expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({
          body: `<img srcset=" , " src="${on('a.png')}">`,
          unrebased: [],
        })
      })
    })

    describe('a `ping`', () => {
      it('should rebase each url at whitespace, in place, its spacing kept', () => {
        expect(absolute('<a href="https://x/" ping="/p\t./q  ">x</a>')).toBe(
          `<a href="https://x/" ping="${on('/p')}\t${on('q')}  ">x</a>`
        )
      })

      it('should keep a comma inside the url it stands in', () => {
        expect(absolute('<a href="https://x/" ping="/p,/q">x</a>')).toBe(
          `<a href="https://x/" ping="${on('/p,/q')}">x</a>`
        )
      })

      it('should leave one whose every url parses on its own as written', () => {
        const body = '<a href="https://x/" ping="https://p/  https://q/">x</a>'

        expect(absolute(body)).toBe(body)
      })
    })

    describe('an SVG `href`', () => {
      it('should rebase any SVG element’s `href` or `xlink:href`, written under its name', () => {
        const body =
          '<svg><use href="sprite.svg#i"/><image xlink:href="./i.png"/>' +
          '<filter><feImage href="/f.png"/></filter>' +
          '<a href="./y" xlink:href="./x"><text>t</text></a></svg>'

        expect(absolute(body)).toBe(
          `<svg><use href="${on('sprite.svg#i')}"/><image xlink:href="${on('i.png')}"/>` +
            `<filter><feImage href="${on('/f.png')}"/></filter>` +
            `<a href="${on('y')}" xlink:href="${on('x')}"><text>t</text></a></svg>`
        )
      })

      it('should keep a fragment alone where the element embeds it, and rebase one an `a` goes to', () => {
        const kept = '<svg><use href="#i"/><use xlink:href="#j"/><symbol id="i"/></svg>'

        expect(absolute(kept)).toBe(kept)
        // A fragment alone once stripped, and kept so.
        expect(absolute('<svg><use href=" #i "/></svg>')).toBe(
          '<svg><use href="#i"/></svg>'
        )
        expect(
          absolute('<svg><a href="#x"><text>t</text></a><a xlink:href="#y"/></svg>')
        ).toBe(
          `<svg><a href="${on('#x')}"><text>t</text></a><a xlink:href="${on('#y')}"/></svg>`
        )
      })
    })

    describe('a `style` attribute', () => {
      it('should rebase a bare `url()` and a string, writing each single-quoted in place', () => {
        const body =
          '<p style="background: url(./c.png) no-repeat; mask: url( &quot;/d.svg&quot; )">p</p>'

        expect(absolute(body)).toBe(
          `<p style="background: url('${on('c.png')}') no-repeat; ` +
            `mask: url( '${on('/d.svg')}' )">p</p>`
        )
      })

      it('should write the rest of the value as it reads, its double quotes escaped', () => {
        expect(
          absolute(`<p style='font-family: "Fira Code"; background: url(./bg.png)'>p</p>`)
        ).toBe(
          `<p style="font-family: &quot;Fira Code&quot;; background: url('${on('bg.png')}')">` +
            'p</p>'
        )
      })

      it('should escape what the CSS string it writes cannot hold as it is', () => {
        expect(absolute(`<p style='background: url("./it&apos;s.png")'>p</p>`)).toBe(
          `<p style="background: url('${on("it\\'s.png")}')">p</p>`
        )
        // Kept, as a url that parses on its own, but stripped of the space before it; each
        // newline CSS reads, a line feed, form feed or carriage return, by its code point.
        expect(
          absolute(
            `<p style="background: url(' https://x.example/a\\A b\\C c\\D d.png')">p</p>`
          )
        ).toBe(`<p style="background: url('https://x.example/a\\a b\\c c\\d d.png')">p</p>`)
      })

      it('should keep a fragment alone, which names part of the body, and a `data:` url', () => {
        const kept =
          '<p style="clip-path: url(#c); background: url(data:image/png;base64,AAAA)">p</p>'

        expect(absolute(kept)).toBe(kept)
        expect(absolute('<p style="clip-path: url(c.svg#c)">p</p>')).toBe(
          `<p style="clip-path: url('${on('c.svg#c')}')">p</p>`
        )
      })
    })

    it('should keep a url the URL parser refuses as written, saying so, and rebase the rest', () => {
      const body = '<p><img src=" http://exa mple.com/a.png"><a href="./b">b</a></p>'

      expect(rebaseLinks(body, PAGE, ORIGIN)).toEqual({
        body: body.replace('./b', 'https://limulus.net/feed/the-rebound/b'),
        unrebased: [{ at: '<img src>', url: 'http://exa mple.com/a.png' }],
      })
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
      expect(rooted('<img srcset="a.jpg 2x, /b.jpg 3x">')).toBe(
        '<img srcset="/feed/the-rebound/a.jpg 2x, /b.jpg 3x">'
      )
    })

    it('should leave a url from `/` or `//`, and one that parses on its own', () => {
      const body =
        '<a href="/photos/">p</a><a href="//cdn.example/z">z</a>' +
        '<a href="https://example.com/">e</a>'

      expect(rooted(body)).toBe(body)
    })

    it('should strip a url it leaves', () => {
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
        unrebased: [{ at: '<a href>', url: 'http://exa mple.com/' }],
      })
    })
  })
})
