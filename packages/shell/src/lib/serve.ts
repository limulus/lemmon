import { contentType, lookup } from 'mime-types'
import { strict as assert } from 'node:assert'
import { Buffer } from 'node:buffer'
import { once } from 'node:events'
import { readFile, stat } from 'node:fs/promises'
import {
  createServer,
  STATUS_CODES,
  type IncomingMessage,
  type OutgoingHttpHeaders,
  type ServerResponse,
} from 'node:http'
import { extname, join } from 'node:path'
import { WebSocketServer } from 'ws'

import { contains, type ServerHeader } from './config.ts'
import { outputFile } from './write.ts'

/** What {@link serve} serves, and where. */
export interface ServeOptions {
  /** The directory served: the build's output. */
  root: string
  /** The port listened on, at `127.0.0.1`; 0 for one the system picks. */
  port: number
  /**
   * Sent with every response: the config's `dev.headers`, as `loadConfig` validated
   * them, so none is a header the server sends itself.
   */
  headers: Readonly<Record<string, string>>
}

/** A running dev server. */
export interface DevServer {
  /** The port it listens on: the one asked for, or the one the system picked. */
  readonly port: number
  /** Tells every page open on the server to reload, through its reload socket. */
  reload(): void
  /** Closes every connection, the reload sockets among them, and then the server. */
  close(): Promise<void>
}

/** Where a page's reload client is served from. */
const CLIENT = '/.lemmon/reload.js'
/** The WebSocket each reload client listens on. */
const SOCKET = '/.lemmon/reload'

/**
 * The reload client: it reloads the page on any message from the server. A socket that
 * closes is opened again every second, and the page reloads once one opens, since a
 * server that came back — `lem dev` restarted — has built again. A file rather than an
 * inline script, so that a page whose policy allows only `script-src 'self'` still
 * runs it.
 */
const CLIENT_SOURCE = `const url = \`\${location.protocol.replace('http', 'ws')}//\${location.host}${SOCKET}\`
let lost = false
function connect() {
  const socket = new WebSocket(url)
  socket.addEventListener('open', () => {
    if (lost) location.reload()
  })
  socket.addEventListener('message', () => location.reload())
  socket.addEventListener('close', () => {
    lost = true
    setTimeout(connect, 1000)
  })
}
connect()
`

/** What every HTML page served gets, so that it reloads with the server. */
const CLIENT_TAG = Buffer.from(`<script type="module" src="${CLIENT}"></script>`)

/**
 * The headers a response is sent with besides the site's own: only names that
 * `SERVER_HEADERS` lists, which a site's `dev.headers` cannot, so neither overrides the
 * other.
 */
type Own = Partial<Pick<OutgoingHttpHeaders, ServerHeader>>

/** What a request is answered from. */
interface Served {
  root: string
  headers: Readonly<Record<string, string>>
  port: number
}

/**
 * Serves `root` over HTTP at `127.0.0.1` — the literal address, since a `localhost`
 * listen binds `::1` alone on macOS, where another server on `127.0.0.1` at the same
 * port would go unreported — rejecting with the listen's error, `EADDRINUSE` for a port
 * in use.
 *
 * A request whose `Host` names anything but `127.0.0.1` or `localhost` at the port is
 * refused with a 403, so that a page on another site whose name has been pointed here
 * (DNS rebinding) cannot read what is served; a WebSocket upgrade, too. Otherwise a
 * request is answered from `root` as the build wrote it: the file a page's url names
 * (`outputFile`, so `/feed/` is `feed/index.html`), with a `Content-Type` from its
 * extension; a directory requested without its `/` redirected to the url with one,
 * keeping the query; and anything else — a url naming no file inside `root`, a
 * malformed escape among them, or a file not there — 404. A rebuild rewrites each file
 * in place, so a page read while it writes can be served partial; the reload comes
 * after the build, so only a page refreshed by hand in those milliseconds sees one.
 * Only GET and HEAD are answered; any other method is 405. An HTML page gets the
 * reload client's `<script>` before its last `</body>`, or after it all where it has
 * none, in the bytes sent: the file on disk is left as the build wrote it. Two urls
 * answer before `root` is consulted: the client, at `/.lemmon/reload.js`, and the
 * WebSocket it opens, at `/.lemmon/reload`, which {@link DevServer.reload} sends a
 * message to. A WebSocket upgrade to any other url is refused; any other upgrade, such
 * as HTTP/2's `h2c`, is answered as a request — from Node 24.9, which reads
 * `shouldUpgradeCallback`; before it, the upgrade reaches ws, which refuses it with a
 * 400. A response that cannot be written is cut off, and the server goes on.
 *
 * Every response carries `headers` and `Cache-Control: no-cache`, but those the `ws`
 * package writes — the WebSocket handshake, answered or refused — and those Node's own
 * parser writes for a request it cannot read.
 */
export async function serve({ root, port, headers }: ServeOptions): Promise<DevServer> {
  const served: Served = { root, headers, port }
  const sockets = new WebSocketServer({
    noServer: true,
    // ws refuses an upgrade to any other url with a 400, and one from a foreign host
    // with the 403 a request gets.
    path: SOCKET,
    verifyClient: ({ req }, done) => {
      done(isLocal(req, served.port), 403)
    },
  })
  const server = createServer(
    {
      shouldUpgradeCallback: (request) =>
        request.headers.upgrade?.toLowerCase() === 'websocket',
    },
    (request, response) => {
      // Node refuses some headers only as it writes them — a `Trailer` on a response
      // with a length — and an unhandled rejection would take the server down.
      respond(served, request, response).catch(() => {
        response.destroy()
      })
    }
  )
  server.on('upgrade', (request, socket, head) => {
    sockets.handleUpgrade(request, socket, head, (client) => {
      // ws closes a client that breaks the protocol itself, and then emits `error`,
      // which with no listener would throw and take the server down.
      client.on('error', () => {})
    })
  })
  server.listen(port, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  assert(typeof address === 'object' && address !== null, 'a TCP server has a port')
  served.port = address.port
  return {
    port: address.port,
    reload() {
      for (const client of sockets.clients) client.send('reload')
    },
    async close() {
      const closed = once(server, 'close')
      server.close()
      server.closeAllConnections()
      // An upgraded socket is no longer the server's connection, but it holds the
      // server open until it ends.
      for (const client of sockets.clients) client.terminate()
      await closed
    },
  }
}

/**
 * Whether `request`'s `Host` names this server as a page served from it would:
 * `127.0.0.1` or `localhost`, in any case, at `port` — which it leaves out at 80.
 */
function isLocal(request: IncomingMessage, port: number): boolean {
  const match = /^(?:127\.0\.0\.1|localhost)(?::(\d+))?$/i.exec(request.headers.host ?? '')
  return match !== null && Number(match[1] ?? '80') === port
}

/** What a request is answered with, besides the site's headers. */
interface Reply {
  status: number
  headers: Own
  body: string | Buffer
}

async function respond(
  served: Served,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  const reply = await answer(served, request)
  const body = typeof reply.body === 'string' ? Buffer.from(reply.body) : reply.body
  response.writeHead(reply.status, {
    ...served.headers,
    'cache-control': 'no-cache',
    'content-length': body.length,
    ...reply.headers,
  })
  // Node sends no body for a HEAD, leaving the length the GET's.
  response.end(body)
}

async function answer({ root, port }: Served, request: IncomingMessage): Promise<Reply> {
  assert(request.url !== undefined, 'a request from a server has a url')
  if (!isLocal(request, port)) return plain(403)
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    return plain(405, { allow: 'GET, HEAD' })
  }
  const query = request.url.indexOf('?')
  const url = query === -1 ? request.url : request.url.slice(0, query)
  if (url === CLIENT) return file(CLIENT, CLIENT_SOURCE)
  if (url === SOCKET) return plain(426, { upgrade: 'websocket' })
  const name = outputFile(url)
  if (name === undefined) return plain(404)
  const path = join(root, name)
  assert(contains(root, path), 'a url names a file inside the output, or none')
  const stats = await stat(path).catch(() => undefined)
  if (stats?.isDirectory() && !url.endsWith('/')) {
    const search = query === -1 ? '' : request.url.slice(query)
    return { status: 301, headers: { location: `${url}/${search}` }, body: '' }
  }
  // Missing, or a directory: no page is there.
  const bytes = await readFile(path).catch(() => undefined)
  if (bytes === undefined) return plain(404)
  return file(path, lookup(path) === 'text/html' ? withClient(bytes) : bytes)
}

/** A file's content, as the type its extension names. */
const file = (path: string, body: string | Buffer): Reply => ({
  status: 200,
  headers: { 'content-type': typeOf(path) },
  body,
})

/** A response that is its status, in plain text. */
const plain = (status: number, headers: Own = {}): Reply => ({
  status,
  headers: { 'content-type': 'text/plain; charset=utf-8', ...headers },
  body: `${STATUS_CODES[status]}\n`,
})

/**
 * The type a file is served as, by its extension: `mime-types`', with its charset rule —
 * `utf-8` for text such as HTML, CSS, and JavaScript, none for XML or SVG, which declare
 * their own.
 */
function typeOf(path: string): string {
  return contentType(extname(path)) || 'application/octet-stream'
}

/**
 * `html` with the reload client's tag before its last `</body>`, or after it all where
 * it has none. The bytes are searched, not parsed, so the tag lands wrong on a page
 * whose last `</body>` is not its body's end — one in a comment or a script's string
 * after it, where the tag sits inside that comment or string — and a UTF-16 page gets
 * ASCII appended. The build writes UTF-8, and the tag is only ever in what this server
 * sends.
 */
function withClient(html: Buffer): Buffer {
  // Latin-1 decodes each byte as one character, so an index in the text is one in the
  // bytes, whatever the page's encoding.
  const at = html.toString('latin1').toLowerCase().lastIndexOf('</body>')
  const cut = at === -1 ? html.length : at
  return Buffer.concat([html.subarray(0, cut), CLIENT_TAG, html.subarray(cut)])
}
