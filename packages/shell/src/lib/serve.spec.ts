import { Buffer } from 'node:buffer'
import { once } from 'node:events'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import {
  Agent,
  request as httpRequest,
  type IncomingHttpHeaders,
  type IncomingMessage,
} from 'node:http'
import { connect, createServer as createTcpServer, type AddressInfo } from 'node:net'
import { join } from 'node:path'
import type { Duplex } from 'node:stream'
import { runInNewContext } from 'node:vm'
import { describe, expect, it } from 'vitest'

import { SERVER_HEADERS } from './config.ts'
import { serve, type DevServer } from './serve.ts'
import { tempDir } from '../mocks/temp-dir.ts'

const TAG = '<script type="module" src="/.lemmon/reload.js"></script>'

/** A response as the client received it. */
interface Received {
  status: number | undefined
  headers: IncomingHttpHeaders
  body: Buffer
}

/** The response's status, headers, and body, read to its end. */
async function received(response: IncomingMessage): Promise<Received> {
  const chunks: Buffer[] = []
  for await (const chunk of response) chunks.push(chunk as Buffer)
  return {
    status: response.statusCode,
    headers: response.headers,
    body: Buffer.concat(chunks),
  }
}

/**
 * `path` asked of the server at `port` as it is written: `fetch` and `new URL()` would
 * resolve a `..` before sending it. The connection closes with the response, unless
 * `headers` or the `agent` keep it alive.
 */
async function get(
  port: number,
  path: string,
  options: { method?: string; headers?: Record<string, string>; agent?: Agent } = {}
): Promise<Received> {
  const request = httpRequest({
    host: '127.0.0.1',
    port,
    path,
    method: options.method ?? 'GET',
    headers: options.headers,
    agent: options.agent ?? false,
  })
  request.end()
  const [response] = (await once(request, 'response')) as [IncomingMessage]
  return await received(response)
}

/**
 * A WebSocket handshake for `path`, made by hand so that a spec can see a refusal's
 * status, or write frames no client would on the socket it was upgraded to.
 */
async function handshake(
  port: number,
  path: string,
  headers: Record<string, string> = {}
): Promise<{ status: number | undefined; socket?: Duplex }> {
  const request = httpRequest({
    host: '127.0.0.1',
    port,
    path,
    agent: false,
    headers: {
      Connection: 'Upgrade',
      Upgrade: 'websocket',
      'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ==',
      'Sec-WebSocket-Version': '13',
      ...headers,
    },
  })
  request.end()
  return await new Promise((resolve, reject) => {
    request.on('upgrade', (response: IncomingMessage, socket: Duplex) => {
      resolve({ status: response.statusCode, socket })
    })
    request.on('response', (response: IncomingMessage) => {
      response.resume()
      resolve({ status: response.statusCode })
    })
    request.on('error', reject)
  })
}

/** A browser's WebSocket to the server's reload socket, once it is open. */
async function reloadSocket(port: number): Promise<WebSocket> {
  const socket = new WebSocket(`ws://127.0.0.1:${port}/.lemmon/reload`)
  await once(socket, 'open')
  return socket
}

/**
 * What a server that should meet no error is given to tell of one: rethrown, it is an
 * unhandled rejection, which fails the run.
 */
const UNEXPECTED = (error: unknown): never => {
  throw error
}

/**
 * Runs `test` against a server over `files` — keyed by paths under the served
 * directory, with `/` separators — closing it when `test` ends. The directory is
 * `dist` inside a temporary one, which also holds `secret.txt` outside what is served.
 * What the server could not answer is collected in `errors` where it is given, and is
 * otherwise expected to be nothing.
 */
async function serving(
  files: Record<string, string>,
  test: (server: DevServer, root: string) => Promise<void>,
  headers: Record<string, string> = {},
  errors?: unknown[]
): Promise<void> {
  const dir = tempDir({
    'secret.txt': 'secret',
    ...Object.fromEntries(
      Object.entries(files).map(([path, text]) => [`dist/${path}`, text])
    ),
  })
  const root = join(dir, 'dist')
  mkdirSync(root, { recursive: true })
  const met: unknown[] = errors ?? []
  const onError = (error: unknown) => {
    met.push(error)
  }
  const server = await serve({ root, port: 0, headers, onError })
  try {
    await test(server, root)
  } finally {
    await server.close()
  }
  if (errors === undefined) expect(met, 'what the server could not answer').toEqual([])
}

const site = {
  'index.html': '<!doctype html><html><body><p>home</p></body></html>',
  'feed/index.html': '<p>feed</p>',
  'feed/the-rebound/index.html': '<p>the rebound</p>',
  'notes/a b/index.html': '<p>a note</p>',
  'feed.xml': '<feed/>',
  'a.css': 'p { color: red }',
  'a.txt': 'a',
}

/** The responses every kind of request gets, from a server over {@link site}. */
async function everyKind(port: number): Promise<Received[]> {
  return [
    await get(port, '/'),
    await get(port, '/', { method: 'HEAD' }),
    await get(port, '/a.css'),
    await get(port, '/feed'),
    await get(port, '/nope'),
    await get(port, '/', { method: 'POST' }),
    await get(port, '/.lemmon/reload.js'),
    await get(port, '/.lemmon/reload'),
    await get(port, '/', { headers: { Host: 'limulus.test' } }),
  ]
}

/**
 * What a page's script sees as it runs the reload client: the `WebSocket` it opens and
 * the timers it sets, recorded rather than run so that a spec decides when a retry
 * happens.
 */
function page(protocol: string, host: string, WebSocketClass: unknown) {
  const timers: { run: () => void; delay: number }[] = []
  let reloads = 0
  const context = {
    WebSocket: WebSocketClass,
    location: {
      protocol,
      host,
      reload: () => {
        reloads++
      },
    },
    setTimeout: (run: () => void, delay: number) => {
      timers.push({ run, delay })
    },
  }
  return { context, timers, reloads: () => reloads }
}

describe('serve()', { timeout: 10_000 }, () => {
  describe('files', () => {
    it('should answer a url with the file the build wrote for it', async () => {
      await serving(site, async ({ port }) => {
        expect((await get(port, '/')).body.toString()).toContain('<p>home</p>')
        expect((await get(port, '/feed/')).body.toString()).toContain('<p>feed</p>')
        expect((await get(port, '/feed/the-rebound/')).body.toString()).toContain(
          '<p>the rebound</p>'
        )
        expect((await get(port, '/notes/a%20b/')).body.toString()).toContain('a note')
        expect((await get(port, '/feed.xml?v=2')).body.toString()).toBe('<feed/>')
      })
    })

    it('should send a file’s bytes as they are', async () => {
      await serving({}, async ({ port }, root) => {
        const bytes = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0xc3, 0xff])
        writeFileSync(join(root, 'x.png'), bytes)

        const { status, headers, body } = await get(port, '/x.png')

        expect(status).toBe(200)
        expect([...body]).toEqual([...bytes])
        expect(headers['content-length']).toBe(String(bytes.length))
      })
    })

    it('should name the type by the extension, with a charset where mime-types gives one', async () => {
      const files = {
        'a.html': '',
        'a.css': '',
        'a.js': '',
        'a.json': '',
        'a.webmanifest': '',
        'a.xml': '',
        'a.svg': '',
        'a.woff2': '',
        LICENSE: '',
        'a.unknown': '',
      }
      await serving(files, async ({ port }) => {
        const types: Record<string, string | undefined> = {}
        for (const file of Object.keys(files)) {
          types[file] = (await get(port, `/${file}`)).headers['content-type']
        }

        expect(types).toEqual({
          'a.html': 'text/html; charset=utf-8',
          'a.css': 'text/css; charset=utf-8',
          'a.js': 'text/javascript; charset=utf-8',
          'a.json': 'application/json; charset=utf-8',
          'a.webmanifest': 'application/manifest+json; charset=utf-8',
          'a.xml': 'application/xml',
          'a.svg': 'image/svg+xml',
          'a.woff2': 'font/woff2',
          LICENSE: 'application/octet-stream',
          'a.unknown': 'application/octet-stream',
        })
      })
    })

    it('should answer a HEAD with the GET’s headers and no body', async () => {
      await serving(site, async ({ port }) => {
        const got = await get(port, '/')
        const head = await get(port, '/', { method: 'HEAD' })

        expect(head.status).toBe(200)
        expect(head.body).toHaveLength(0)
        expect({ ...head.headers, date: undefined }).toEqual({
          ...got.headers,
          date: undefined,
        })
        expect(head.headers['content-length']).toBe(String(got.body.length))
      })
    })

    it('should refuse any method but GET and HEAD', async () => {
      await serving(site, async ({ port }) => {
        for (const method of ['POST', 'PUT', 'DELETE', 'OPTIONS']) {
          const { status, headers, body } = await get(port, '/', { method })

          expect(status, method).toBe(405)
          expect(headers.allow).toBe('GET, HEAD')
          expect(headers['content-type']).toBe('text/plain; charset=utf-8')
          expect(body.toString()).toBe('Method Not Allowed\n')
        }
      })
    })
  })

  describe('directories', () => {
    it('should redirect a directory asked for without its `/`, keeping the query', async () => {
      await serving(site, async ({ port }) => {
        const redirects: Record<string, unknown> = {}
        for (const path of ['/feed', '/feed?page=2', '/notes/a%20b', '/feed/the-rebound']) {
          const { status, headers, body } = await get(port, path)
          expect(body).toHaveLength(0)
          redirects[path] = [status, headers.location]
        }

        expect(redirects).toEqual({
          '/feed': [301, '/feed/'],
          '/feed?page=2': [301, '/feed/?page=2'],
          '/notes/a%20b': [301, '/notes/a%20b/'],
          '/feed/the-rebound': [301, '/feed/the-rebound/'],
        })
      })
    })

    it('should answer 404 for a directory whose index.html is missing or no file', async () => {
      await serving(
        { 'empty/.keep': '', 'odd/index.html/x.html': '' },
        async ({ port }) => {
          expect((await get(port, '/empty/')).status).toBe(404)
          expect((await get(port, '/odd/')).status).toBe(404)
        }
      )
    })
  })

  describe('a url naming no file', () => {
    it('should answer 404 with a page carrying the reload client, where no file is', async () => {
      await serving(site, async ({ port }) => {
        // No file there, or a url naming none: a tab open on either reloads once a
        // build writes the page.
        for (const path of ['/nope.html', '/feed/nope/', '/a.css/', '/a//b/']) {
          const { status, headers, body } = await get(port, path)

          expect(status, path).toBe(404)
          expect(headers['content-type']).toBe('text/html; charset=utf-8')
          expect(body.toString()).toBe(
            `<!doctype html>\n<title>Not Found</title>\n<p>Not Found</p>\n${TAG}\n`
          )
        }
      })
    })

    it('should answer 404 for a url leading outside the served directory, where a file is', async () => {
      await serving({ 'a/b.txt': 'b' }, async ({ port }, root) => {
        expect(readFileSync(join(root, '..', 'secret.txt'), 'utf8')).toBe('secret')
        expect((await get(port, '/a/b.txt')).status).toBe(200)

        for (const path of [
          '/../secret.txt',
          '/a/../../secret.txt',
          '/%2e%2e/secret.txt',
          '/..%2fsecret.txt',
          '/a/..%2f..%2fsecret.txt',
          '/a%2f..%2f..%2fsecret.txt',
          '/..%5csecret.txt',
        ]) {
          const { status, body } = await get(port, path)

          expect(status, path).toBe(404)
          expect(body.toString()).not.toContain('secret')
        }
      })
    })

    it('should answer 404 for a malformed escape or an encoded NUL', async () => {
      // Files named as the urls are written, which a server that did not decode a url
      // would answer with.
      const paths = ['/%E0%A4%A', '/%zz.txt', '/a.txt%00', '/a.txt%00.html']
      const files = Object.fromEntries(paths.map((path) => [path.slice(1), 'literal']))
      await serving(files, async ({ port }) => {
        for (const path of paths) {
          expect((await get(port, path)).status, path).toBe(404)
        }
      })
    })
  })

  describe('the host', () => {
    it('should answer a request naming the server as a page from it does', async () => {
      await serving(site, async ({ port }) => {
        for (const host of [
          `127.0.0.1:${port}`,
          `localhost:${port}`,
          `LocalHost:${port}`,
        ]) {
          expect((await get(port, '/', { headers: { Host: host } })).status, host).toBe(200)
        }
      })
    })

    it('should refuse a request naming another host, or another port, with a 403', async () => {
      const headers = { 'Cross-Origin-Opener-Policy': 'same-origin' }
      await serving(
        site,
        async ({ port }) => {
          for (const host of [
            'limulus.test',
            `limulus.test:${port}`,
            `127.0.0.1:${port + 1}`,
            '127.0.0.1',
            `127.0.0.1.limulus.test:${port}`,
            `rebinding-localhost:${port}`,
            `localhost:${port}.limulus.test`,
          ]) {
            const refused = await get(port, '/', { headers: { Host: host } })

            expect(refused.status, host).toBe(403)
            expect(refused.body.toString()).toBe('Forbidden\n')
            expect(refused.headers).toMatchObject({
              'content-type': 'text/plain; charset=utf-8',
              'cross-origin-opener-policy': 'same-origin',
              'cache-control': 'no-cache',
            })
          }
          // HTTP/1.0 needs no `Host` at all, which names nothing.
          const bare = connect(port, '127.0.0.1')
          bare.write('GET / HTTP/1.0\r\n\r\n')
          const chunks: Buffer[] = []
          for await (const chunk of bare) chunks.push(chunk as Buffer)
          expect(Buffer.concat(chunks).toString()).toMatch(/^HTTP\/1\.1 403 /)
        },
        headers
      )
    })

    it('should upgrade a socket for the server’s host, and refuse one for another', async () => {
      await serving({}, async ({ port }) => {
        const local = await handshake(port, '/.lemmon/reload', {
          Host: `localhost:${port}`,
        })
        local.socket?.destroy()
        const foreign = await handshake(port, '/.lemmon/reload', {
          Host: `limulus.test:${port}`,
        })

        expect(local.status).toBe(101)
        expect(foreign.status).toBe(403)
      })
    })
  })

  describe('headers', () => {
    it('should send the site’s headers and `no-cache` with every response', async () => {
      const headers = {
        'Cross-Origin-Opener-Policy': 'same-origin',
        'Cross-Origin-Embedder-Policy': 'require-corp',
        'X-Empty': '',
      }
      await serving(
        site,
        async ({ port }) => {
          for (const response of await everyKind(port)) {
            expect(response.headers).toMatchObject({
              'cross-origin-opener-policy': 'same-origin',
              'cross-origin-embedder-policy': 'require-corp',
              'x-empty': '',
              'cache-control': 'no-cache',
            })
          }
        },
        headers
      )
    })

    it('should cut off a response it cannot write, tell of it, and go on', async () => {
      const errors: unknown[] = []
      // A header Node refuses only as it writes it, as one that slipped past
      // `loadConfig` would be.
      await serving(
        site,
        async ({ port }) => {
          await expect(get(port, '/')).rejects.toMatchObject({ code: 'ECONNRESET' })
          const { status, socket } = await handshake(port, '/.lemmon/reload')
          socket?.destroy()

          expect(status).toBe(101)
        },
        { Trailer: 'Server-Timing' },
        errors
      )

      expect(errors).toEqual([
        expect.objectContaining({ code: 'ERR_HTTP_TRAILER_INVALID' }),
      ])
    })

    it('should send no header of its own that SERVER_HEADERS does not name', async () => {
      await serving(site, async ({ port }) => {
        // A kept-alive connection is where Node adds `keep-alive`.
        const kept = { headers: { Connection: 'keep-alive' } }
        const responses = [...(await everyKind(port)), await get(port, '/', kept)]
        const names = new Set(responses.flatMap((r) => Object.keys(r.headers)))

        expect(names).toContain('keep-alive')
        const served: readonly string[] = SERVER_HEADERS
        expect([...names].filter((name) => !served.includes(name))).toEqual([])
      })
    })
  })

  describe('the reload client', () => {
    it('should place its tag before an HTML page’s last `</body>`, in any case', async () => {
      const page = '<body><p>café</p><script>"</body>"</script></BODY>\n</html>\n'
      await serving({ 'index.html': page }, async ({ port }, root) => {
        const { headers, body } = await get(port, '/')

        expect(body.toString()).toBe(
          `<body><p>café</p><script>"</body>"</script>${TAG}</BODY>\n</html>\n`
        )
        expect(headers['content-length']).toBe(String(body.length))
        expect(readFileSync(join(root, 'index.html'), 'utf8')).toBe(page)
      })
    })

    it('should place its tag after it all on an HTML page with no `</body>`', async () => {
      await serving({ 'a.html': '<p>café</p>' }, async ({ port }) => {
        expect((await get(port, '/a.html')).body.toString()).toBe(`<p>café</p>${TAG}`)
      })
    })

    it('should leave a file that is not HTML as it is', async () => {
      const text = '<p>a</p></body>'
      await serving({ 'a.txt': text, 'a.xml': text, 'a.svg': text }, async ({ port }) => {
        for (const file of ['a.txt', 'a.xml', 'a.svg']) {
          expect((await get(port, `/${file}`)).body.toString(), file).toBe(text)
        }
      })
    })

    it('should serve the client and refuse a plain request to its socket, before the output', async () => {
      const files = { '.lemmon/reload.js': 'shadow', '.lemmon/reload': 'shadow' }
      await serving(files, async ({ port }) => {
        const client = await get(port, '/.lemmon/reload.js?v=2')
        const socket = await get(port, '/.lemmon/reload')

        expect(client.status).toBe(200)
        expect(client.headers['content-type']).toBe('text/javascript; charset=utf-8')
        expect(client.body.toString()).toContain('/.lemmon/reload`')
        expect(socket.status).toBe(426)
        expect(socket.headers.upgrade).toBe('websocket')
        expect(socket.body.toString()).toBe('Upgrade Required\n')
      })
    })

    it('should reload a page running it when the server reloads', async () => {
      await serving({}, async (server) => {
        const source = (await get(server.port, '/.lemmon/reload.js')).body.toString()
        const opened: Promise<unknown>[] = []
        const made: WebSocket[] = []
        // The browser's WebSocket, as the page's script constructs it.
        class PageSocket extends WebSocket {
          constructor(url: string) {
            super(url)
            made.push(this)
            opened.push(once(this, 'open'))
          }
        }
        const { context, reloads } = page('http:', `127.0.0.1:${server.port}`, PageSocket)
        try {
          runInNewContext(source, context)
          expect(made.map((socket) => socket.url)).toEqual([
            `ws://127.0.0.1:${server.port}/.lemmon/reload`,
          ])
          await Promise.all(opened)
          const [message] = made.map(async (socket) => await once(socket, 'message'))
          server.reload()
          await message

          expect(reloads()).toBe(1)
        } finally {
          for (const socket of made) socket.close()
        }
      })
    })

    it('should open its socket over TLS on a page served over it', async () => {
      await serving({}, async ({ port }) => {
        const source = (await get(port, '/.lemmon/reload.js')).body.toString()
        const urls: string[] = []
        class Recorder {
          constructor(url: string) {
            urls.push(url)
          }
          addEventListener() {}
        }

        runInNewContext(source, page('https:', 'limulus.test', Recorder).context)

        expect(urls).toEqual(['wss://limulus.test/.lemmon/reload'])
      })
    })

    it('should open its socket again after it closes, and reload once one opens', async () => {
      const root = tempDir()
      let server = await serve({ root, port: 0, headers: {}, onError: UNEXPECTED })
      const { port } = server
      const made: WebSocket[] = []
      class PageSocket extends WebSocket {
        constructor(url: string) {
          super(url)
          made.push(this)
        }
      }
      const { context, timers, reloads } = page('http:', `127.0.0.1:${port}`, PageSocket)
      try {
        const source = (await get(port, '/.lemmon/reload.js')).body.toString()
        runInNewContext(source, context)
        await once(made[0], 'open')
        expect(reloads()).toBe(0)

        // The server stops, as `lem dev` does when it is restarted.
        const closed = once(made[0], 'close')
        await server.close()
        await closed
        expect(timers.map((timer) => timer.delay)).toEqual([1000])

        // A retry while it is down fails, and is retried in turn.
        timers[0].run()
        await once(made[1], 'close')
        expect(timers.map((timer) => timer.delay)).toEqual([1000, 1000])
        expect(reloads()).toBe(0)

        server = await serve({ root, port, headers: {}, onError: UNEXPECTED })
        timers[1].run()
        await once(made[2], 'open')

        expect(reloads()).toBe(1)
      } finally {
        for (const socket of made) socket.close()
        await server.close()
      }
    })
  })

  describe('the reload socket', () => {
    it('should send every open socket a message on reload', async () => {
      await serving({}, async (server) => {
        const sockets = [await reloadSocket(server.port), await reloadSocket(server.port)]
        try {
          const messages = sockets.map(async (socket) => {
            const [event] = (await once(socket, 'message')) as [MessageEvent]
            return event.data as unknown
          })
          server.reload()

          expect(await Promise.all(messages)).toEqual(['reload', 'reload'])
        } finally {
          for (const socket of sockets) socket.close()
        }
      })
    })

    it('should refuse an upgrade to any other url', async () => {
      await serving({ 'other/index.html': '' }, async ({ port }) => {
        for (const path of ['/other/', '/.lemmon/reload.js', '/.lemmon/reload/']) {
          expect((await handshake(port, path)).status, path).toBe(400)
        }
        const { status, socket } = await handshake(port, '/.lemmon/reload?tab=1')
        socket?.destroy()

        expect(status).toBe(101)
      })
    })

    it('should answer an upgrade to anything but a WebSocket as a request', async () => {
      await serving(site, async ({ port }) => {
        const { status, socket } = await handshake(port, '/', {
          Connection: 'Upgrade, HTTP2-Settings',
          Upgrade: 'h2c',
          'HTTP2-Settings': '',
        })
        socket?.destroy()

        expect(status).toBe(200)
      })
    })

    it('should close a client that breaks the protocol, and go on serving', async () => {
      await serving(site, async ({ port }) => {
        const { socket } = await handshake(port, '/.lemmon/reload')
        if (socket === undefined) throw new Error('the handshake was refused')
        const closed = once(socket, 'close')
        // A text frame without the mask every client's frame carries.
        socket.write(Buffer.from([0x81, 0x00]))
        socket.resume()

        await closed
        expect((await get(port, '/')).status).toBe(200)
      })
    })
  })

  describe('closing', () => {
    it('should close every connection, the reload sockets among them, and then the server', async () => {
      const agent = new Agent({ keepAlive: true })
      try {
        await serving(site, async (server) => {
          const socket = await reloadSocket(server.port)
          const socketClosed = once(socket, 'close')
          // A connection mid-request, which Node's `close()` alone would wait on for a
          // minute: the head of a first request, never finished. (Once a connection has
          // been answered, Node closes it 6 s after, kept alive or not.)
          const midRequest = connect(server.port, '127.0.0.1')
          // A reset is a close as well; without a listener its `error` would throw.
          midRequest.on('error', () => {})
          const midRequestClosed = once(midRequest, 'close')
          midRequest.write(`GET / HTTP/1.1\r\nHost: 127.0.0.1:${server.port}\r\n`)
          // A connection kept alive, idle in the agent's pool. Its request is answered
          // turns of the event loop after the head above was there to read.
          await get(server.port, '/', { agent })

          await server.close()

          await socketClosed
          await midRequestClosed
          await expect(get(server.port, '/')).rejects.toMatchObject({
            code: 'ECONNREFUSED',
          })
        })
      } finally {
        agent.destroy()
      }
    })

    it('should reject with the listen’s error for a port in use at 127.0.0.1', async () => {
      const taken = createTcpServer()
      taken.listen(0, '127.0.0.1')
      await once(taken, 'listening')
      try {
        const { port } = taken.address() as AddressInfo

        const options = { root: tempDir(), port, headers: {}, onError: UNEXPECTED }

        await expect(serve(options)).rejects.toMatchObject({ code: 'EADDRINUSE' })
      } finally {
        taken.close()
      }
    })
  })
})
