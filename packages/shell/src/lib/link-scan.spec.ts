import { describe, expect, it } from 'vitest'

import { cssLinks, htmlLinks, refreshUrl, xmlLinks } from './link-scan.ts'

const PAGE = 'https://limulus.net/feed/a/'

/** The urls `html` links to, each with where it stands, as a page at {@link PAGE}. */
const linked = (html: string) => htmlLinks(html, PAGE).links.map(({ url, at }) => [url, at])

describe('htmlLinks()', () => {
  describe('the attributes posthtml-urls walks', () => {
    it('should read a url from each, on the element it stands on', () => {
      const html =
        '<link href="/a.css"><a href="/b/">b</a><area href="/c"><img src="/d.jpg">' +
        '<script src="/e.js"></script><iframe src="/f/"></iframe><embed src="/g">' +
        '<audio src="/h.mp3"><track src="/i.vtt"></audio><video poster="/j.jpg" src="/k">' +
        '</video><object data="/l"></object><form action="/m"><button formaction="/n">' +
        '</button><input formaction="/o" src="/p"></form><blockquote cite="/q"></blockquote>' +
        '<q cite="/r"></q><del cite="/s"></del><ins cite="/t"></ins>'

      expect(linked(html)).toEqual([
        ['/a.css', '<link href>'],
        ['/b/', '<a href>'],
        ['/c', '<area href>'],
        ['/d.jpg', '<img src>'],
        ['/e.js', '<script src>'],
        ['/f/', '<iframe src>'],
        ['/g', '<embed src>'],
        ['/h.mp3', '<audio src>'],
        ['/i.vtt', '<track src>'],
        ['/j.jpg', '<video poster>'],
        ['/k', '<video src>'],
        ['/l', '<object data>'],
        ['/m', '<form action>'],
        ['/n', '<button formaction>'],
        ['/o', '<input formaction>'],
        ['/p', '<input src>'],
        ['/q', '<blockquote cite>'],
        ['/r', '<q cite>'],
        ['/s', '<del cite>'],
        ['/t', '<ins cite>'],
      ])
    })

    it('should read each `srcset` candidate and `ping` url on its own', () => {
      const html =
        '<picture><source srcset="/a.webp 1x, /b.webp 2x"><img srcset="/c.jpg 640w"></picture>' +
        '<a href="/d/" ping=" /p1\t/p2 ">d</a>'

      expect(linked(html)).toEqual([
        ['/a.webp', '<source srcset>'],
        ['/b.webp', '<source srcset>'],
        ['/c.jpg', '<img srcset>'],
        ['/d/', '<a href>'],
        ['/p1', '<a ping>'],
        ['/p2', '<a ping>'],
      ])
    })

    it('should split a `ping` at whitespace, as HTML does, and not at commas', () => {
      expect(linked('<a href="/d/" ping="/p1, /p2">d</a>')).toEqual([
        ['/d/', '<a href>'],
        ['/p1,', '<a ping>'],
        ['/p2', '<a ping>'],
      ])
    })

    it('should leave an attribute no row names, and an inline script’s text', () => {
      const html =
        '<div href="/a"></div><x-card src="/b"></x-card><img data-src="/c">' +
        '<script>import("/d.js")</script><svg><rect fill="url(#e)"/></svg>'

      expect(linked(html)).toEqual([])
    })
  })

  describe('the rows it adds', () => {
    it('should read a refreshing `meta`’s url, and no other `meta`’s content', () => {
      const html =
        '<meta http-equiv="Refresh" content="0; url=/new/">' +
        '<meta http-equiv="refresh" content="30">' +
        '<meta http-equiv="content-type" content="text/html">' +
        '<meta http-equiv="refresh">' +
        '<meta name="description" content="/not-a-link">'

      expect(linked(html)).toEqual([['/new/', '<meta http-equiv="refresh" content>']])
    })

    it('should read an Open Graph url’s `content`, and not another property’s', () => {
      const html =
        '<meta property="og:url" content="https://limulus.net/feed/a/">' +
        '<meta property="og:image" content="/a.jpg">' +
        '<meta property="og:image:secure_url" content="/b.jpg">' +
        '<meta property="og:video:url" content="/c.mp4">' +
        '<meta property="og:audio" content="/d.mp3">' +
        '<meta property="og:title" content="TILs: git">' +
        '<meta property="og:image">' +
        '<meta content="/e.jpg">'

      expect(linked(html)).toEqual([
        ['https://limulus.net/feed/a/', '<meta property="og:url" content>'],
        ['/a.jpg', '<meta property="og:image" content>'],
        ['/b.jpg', '<meta property="og:image:secure_url" content>'],
        ['/c.mp4', '<meta property="og:video:url" content>'],
        ['/d.mp3', '<meta property="og:audio" content>'],
      ])
    })

    it('should read any SVG element’s `href` and `xlink:href`, and no other element’s', () => {
      const html =
        '<svg><use href="#icon"></use><use xlink:href="/sprite.svg#a"></use>' +
        '<a href="/a/"><text>a</text></a><a xlink:href="/b/"></a><image href="/c.png"/>' +
        '<filter><feImage xlink:href="/d.png"/></filter><use xml:lang="en"></use></svg>' +
        '<use href="/html-use"></use><x-icon href="/custom"></x-icon>'

      expect(linked(html)).toEqual([
        ['#icon', '<use href>'],
        ['/sprite.svg#a', '<use xlink:href>'],
        ['/a/', '<a href>'],
        ['/b/', '<a xlink:href>'],
        ['/c.png', '<image href>'],
        ['/d.png', '<feImage xlink:href>'],
      ])
    })

    it('should read the urls of a `<style>`’s text and of a `style` attribute', () => {
      const html =
        '<style>@import "/a.css"; p { background: url(/b.png) }</style>' +
        '<p style="background-image: url(\'/c.png\')">x</p>' +
        '<svg><style>/* x */<!-- a comment node -->rect { fill: url(/d.svg#g) }</style></svg>'

      expect(linked(html)).toEqual([
        ['/a.css', '<style>'],
        ['/b.png', '<style>'],
        ['/c.png', '<p style>'],
        ['/d.svg#g', '<style>'],
      ])
    })
  })

  describe('the base', () => {
    it('should be the page’s url where no `<base href>` sets one', () => {
      const { links } = htmlLinks('<base target="_top"><a href="x">x</a>', PAGE)

      expect(links).toEqual([{ url: 'x', at: '<a href>', base: PAGE }])
    })

    it('should be the first `<base href>`’s, for links before it too, and not the next’s', () => {
      const { links } = htmlLinks(
        '<a href="x">x</a><base href="/other/"><base href="/third/"><a href="y">y</a>',
        PAGE
      )

      expect(links.map(({ base }) => base)).toEqual([
        'https://limulus.net/other/',
        'https://limulus.net/other/',
      ])
    })

    it('should be the page’s url where the first `<base href>` does not parse', () => {
      const { links } = htmlLinks(
        '<base href="http://exa mple/"><base href="/third/"><a href="y">y</a>',
        PAGE
      )

      expect(links.map(({ base }) => base)).toEqual([PAGE])
    })

    it('should not be set by a `<base>` inside a `<template>`', () => {
      const { links } = htmlLinks(
        '<template><base href="/t/"></template><a href="y">y</a>',
        PAGE
      )

      expect(links.map(({ base }) => base)).toEqual([PAGE])
    })
  })

  describe('what it walks', () => {
    it('should read a page nested deeper than the call stack goes', () => {
      // Spans, since parse5 looks down every open element for a `<p>` to close at each
      // `<div>`: nesting 20,000 of those takes a second, and five under coverage.
      const html = `${'<span>'.repeat(20_000)}<a href="/deep/">x</a><p id="bottom">`

      const { links, targets } = htmlLinks(html, PAGE)

      expect(links).toEqual([{ url: '/deep/', at: '<a href>', base: PAGE }])
      expect(targets).toEqual(new Set(['bottom']))
    })

    it('should read a `<template>`’s content and a `<noscript>`’s as markup', () => {
      const html =
        '<template><a href="/t/">t</a></template><noscript><img src="/n.gif"></noscript>'

      expect(linked(html)).toEqual([
        ['/t/', '<a href>'],
        ['/n.gif', '<img src>'],
      ])
    })
  })

  describe('targets', () => {
    it('should hold every `id` in the document, and every HTML `a`’s `name`', () => {
      const html =
        '<h2 id="intro">x</h2><a name="old">x</a><svg><symbol id="icon"></symbol>' +
        '<a name="svg-a"></a></svg><p id="">x</p><a>x</a>'

      expect(htmlLinks(html, PAGE).targets).toEqual(new Set(['intro', 'old', 'icon']))
    })

    it('should not hold an id inside a `<template>`, which is no part of the document', () => {
      const html = '<template><p id="t">x</p></template><p id="d">x</p>'

      expect(htmlLinks(html, PAGE).targets).toEqual(new Set(['d']))
    })
  })
})

describe('refreshUrl()', () => {
  it('should read the url after the time, `url=` and any quotes', () => {
    expect(refreshUrl('5; url=/a')).toBe('/a')
    expect(refreshUrl('  0;URL = "/a b" c')).toBe('/a b')
    expect(refreshUrl("0, '/b'")).toBe('/b')
    expect(refreshUrl('0 /c')).toBe('/c')
    expect(refreshUrl('.5;/d')).toBe('/d')
    expect(refreshUrl("1.5; url='/e")).toBe('/e')
  })

  it('should read a url that does not spell `url=` from where it starts', () => {
    expect(refreshUrl('0; urn:x')).toBe('urn:x')
    expect(refreshUrl('0; url /f')).toBe('url /f')
  })

  it('should read no url where the content names none, or is no refresh', () => {
    expect(refreshUrl('5')).toBeUndefined()
    expect(refreshUrl('5; ')).toBeUndefined()
    expect(refreshUrl('soon; url=/a')).toBeUndefined()
    expect(refreshUrl('5x; url=/a')).toBeUndefined()
  })
})

describe('cssLinks()', () => {
  it('should read a `url()` bare or with a string, and `src()` with a string', () => {
    expect(cssLinks('url(/a.png)')).toEqual([{ url: '/a.png', at: 'url()' }])
    expect(
      cssLinks("a { b: url( '/b.png' ); c: URL(\"/c.png\"); d: src('/d.png') }")
    ).toEqual([
      { url: '/b.png', at: 'url()' },
      { url: '/c.png', at: 'url()' },
      { url: '/d.png', at: 'url()' },
    ])
  })

  it('should read `@import`’s string or url', () => {
    const css = '@import "/a.css"; @IMPORT url(/b.css) screen; @import url("/c.css");'

    expect(cssLinks(css)).toEqual([
      { url: '/a.css', at: '@import' },
      { url: '/b.css', at: '@import' },
      { url: '/c.css', at: '@import' },
    ])
  })

  it('should read a url’s escapes as CSS does', () => {
    expect(cssLinks('a { b: url(/a\\20 b.png) }')).toEqual([
      { url: '/a b.png', at: 'url()' },
    ])
  })

  it('should read one url for a `data:` URI holding `url()` in a string, beside a real one', () => {
    const svg = "data:image/svg+xml,%3Csvg%3E%3Crect clip-path='url(%23clip)'/%3E%3C/svg%3E"
    const css = `a { b: url("${svg}"), url(/c.png) }`

    expect(cssLinks(css)).toEqual([
      { url: svg, at: 'url()' },
      { url: '/c.png', at: 'url()' },
    ])
  })

  it('should read no url in a comment, another function’s string, or a bad url', () => {
    const css =
      "/* url(/a.png) */ @font-face { src: url(/f.woff2) format('woff2') } " +
      'a { b: url(/c d.png); content: "url(/e.png)" }'

    expect(cssLinks(css)).toEqual([{ url: '/f.woff2', at: 'url()' }])
  })
})

describe('xmlLinks()', () => {
  const FEED = 'https://limulus.net/feed.xml'

  /** The urls `xml` links to, each with where it stands and its base, as {@link FEED}. */
  const read = (xml: string | Uint8Array) =>
    xmlLinks(typeof xml === 'string' ? new TextEncoder().encode(xml) : xml, FEED)

  it('should read an RSS feed’s stylesheet, links and permalink guids', () => {
    const xml = `<?xml version="1.0"?>
<!-- <?xml-stylesheet href="/commented.xsl" type="text/xsl"?> -->
<?xml-stylesheet type="text/xsl" href="/feed.xsl"?>
<?other href="/other.xsl"?>
<rss xmlns:atom="http://www.w3.org/2005/Atom"><channel>
  <atom:link href="https://limulus.net/feed.xml" rel="self"/>
  <link>https://limulus.net/</link>
  <item>
    <link> https://limulus.net/feed/a/ </link>
    <guid>https://limulus.net/feed/a/</guid>
    <guid isPermaLink="false">tag:limulus.net,2024:a</guid>
    <guid isPermaLink=" False ">urn:b</guid>
    <description>&lt;a href="/escaped/"&gt;x&lt;/a&gt;</description>
    <?xml-stylesheet href="/inside.xsl"?>
  </item>
</channel></rss>`

    expect(read(xml)).toEqual({
      links: [
        { url: '/feed.xsl', at: '<?xml-stylesheet href?>', base: FEED },
        { url: 'https://limulus.net/feed.xml', at: '<atom:link href>', base: FEED },
        { url: 'https://limulus.net/', at: '<link>', base: FEED },
        { url: 'https://limulus.net/feed/a/', at: '<link>', base: FEED },
        { url: 'https://limulus.net/feed/a/', at: '<guid>', base: FEED },
      ],
    })
  })

  it('should read an Atom feed’s links, and not its ids or escaped content', () => {
    const xml = `<feed xmlns="http://www.w3.org/2005/Atom">
  <id>https://limulus.net/tils</id>
  <link href="https://limulus.net/tils/" rel="alternate"/>
  <entry>
    <id>https://limulus.net/tils/a/</id>
    <link href="/tils/a/">ignored text</link>
    <content type="html">&lt;img src="/escaped.png"&gt;</content>
  </entry>
</feed>`

    expect(read(xml)).toEqual({
      links: [
        { url: 'https://limulus.net/tils/', at: '<link href>', base: FEED },
        { url: '/tils/a/', at: '<link href>', base: FEED },
      ],
    })
  })

  it('should read a sitemap’s locs, its extensions’ among them, and no empty one', () => {
    const xml = `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
  xmlns:image="http://www.google.com/schemas/sitemap-image/1.1"
  xmlns:video="http://www.google.com/schemas/sitemap-video/1.1">
  <url>
    <loc>https://limulus.net/a/</loc>
    <image:image><image:loc>https://limulus.net/a.jpg</image:loc></image:image>
    <video:video><video:thumbnail_loc><![CDATA[/t.jpg]]></video:thumbnail_loc></video:video>
    <lastmod>2024-01-01</lastmod>
  </url>
  <url><loc>  </loc></url>
</urlset>`

    expect(read(xml)).toEqual({
      links: [
        { url: 'https://limulus.net/a/', at: '<loc>', base: FEED },
        { url: 'https://limulus.net/a.jpg', at: '<image:loc>', base: FEED },
        { url: '/t.jpg', at: '<video:thumbnail_loc>', base: FEED },
      ],
    })
  })

  it('should read any element’s `xlink:href`', () => {
    const xml = '<doc xmlns:l="http://www.w3.org/1999/xlink"><ref l:href="/r/"/></doc>'

    expect(read(xml)).toEqual({ links: [{ url: '/r/', at: '<ref l:href>', base: FEED }] })
  })

  it('should resolve each link against the nearest `xml:base` that parses', () => {
    const xml =
      '<rss xml:base="https://limulus.net/tils/"><channel xml:base="git/">' +
      '<link>a/</link><item xml:base="http://exa mple/"><link>b/</link></item>' +
      '</channel><link>c/</link></rss>'

    expect(read(xml)).toEqual({
      links: [
        { url: 'a/', at: '<link>', base: 'https://limulus.net/tils/git/' },
        { url: 'b/', at: '<link>', base: 'https://limulus.net/tils/git/' },
        { url: 'c/', at: '<link>', base: 'https://limulus.net/tils/' },
      ],
    })
  })

  it('should decode a stylesheet href’s entities and references, and keep any other', () => {
    const href = (value: string) =>
      read(`<?xml-stylesheet href='${value}'?><a/>`) as { links: { url: string }[] }

    expect(href('/a.xsl?b=1&amp;c=&#x41;&#66;').links[0].url).toBe('/a.xsl?b=1&c=AB')
    expect(href('/&nbsp;&#x110000;').links[0].url).toBe('/&nbsp;&#x110000;')
    expect(read('<?xml-stylesheet xhref="/x.xsl"?><a/>')).toEqual({ links: [] })
  })

  it('should refuse a document the XML parser refuses, with its first error', () => {
    expect(read('<feed><entry></feed>')).toEqual({
      refused:
        'is refused by the XML parser (Opening and ending tag mismatch: "entry" != "feed")',
    })
    expect(read('<feed>&nbsp;</feed>')).toEqual({
      refused: 'is refused by the XML parser (entity not found:&nbsp;)',
    })
    expect(read('')).toEqual({
      refused: 'is refused by the XML parser (missing root element)',
    })
  })

  it('should refuse what the parser only warns at, but a U+FFFD, which UTF-8 may hold', () => {
    expect(read('<a href=x></a>')).toEqual({
      refused: 'is refused by the XML parser (attribute "x" missed quot(")!)',
    })
    expect(read('<loc>/a\uFFFD</loc>')).toEqual({
      links: [{ url: '/a\uFFFD', at: '<loc>', base: FEED }],
    })
  })

  it('should refuse the entities a document declares, which the parser does not expand', () => {
    const laughs =
      '<!DOCTYPE r [<!ENTITY a "lol"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">]>' +
      '<urlset><url><loc>&b;</loc></url></urlset>'

    expect(read(laughs)).toEqual({
      refused: 'is refused by the XML parser (entity not found:&b;)',
    })
  })

  it('should refuse a document that declares an encoding other than UTF-8, or is not UTF-8', () => {
    const declaring = (encoding: string) =>
      `<?xml version="1.0" encoding="${encoding}"?><urlset><loc>/a</loc></urlset>`
    const encoder = new TextEncoder()
    const latin1 = [...encoder.encode('<loc>/caf'), 0xe9, ...encoder.encode('</loc>')]

    expect(read(declaring('ISO-8859-1'))).toEqual({
      refused: 'declares the encoding `ISO-8859-1`, where the check reads UTF-8',
    })
    expect(read(declaring('utf-8'))).toEqual({
      links: [{ url: '/a', at: '<loc>', base: FEED }],
    })
    expect(read(`\uFEFF${declaring('UTF-8')}`)).toEqual({
      links: [{ url: '/a', at: '<loc>', base: FEED }],
    })
    expect(read(new Uint8Array(latin1))).toEqual({ refused: 'is not UTF-8' })
  })

  it('should read an RSS channel image’s url, and no other `url`', () => {
    const xml =
      '<rss><channel><image><url>/logo.png</url><link>/</link></image>' +
      '<item><url>/not-a-link</url></item></channel></rss>'

    expect(read(xml)).toEqual({
      links: [
        { url: '/logo.png', at: '<url>', base: FEED },
        { url: '/', at: '<link>', base: FEED },
      ],
    })
  })

  it('should read a document nested deeper than the call stack goes', () => {
    const depth = 20_000
    const xml = `<r>${'<a>'.repeat(depth)}<loc>/deep</loc>${'</a>'.repeat(depth)}</r>`

    expect(read(xml)).toEqual({ links: [{ url: '/deep', at: '<loc>', base: FEED }] })
  })
})
