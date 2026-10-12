export type LocalApiRequestOptions = RequestInit;

export interface LocalApiTransport {
  request: (
    pathOrUrl: string,
    init?: LocalApiRequestOptions
  ) => Promise<Response>;
  openWebSocket: (pathOrUrl: string) => Promise<WebSocket> | WebSocket;
}

function toAbsoluteWsUrl(pathOrUrl: string): string {
  if (/^wss?:\/\//i.test(pathOrUrl)) return pathOrUrl;
  if (/^https?:\/\//i.test(pathOrUrl)) return pathOrUrl.replace(/^http/i, 'ws');

  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const path = pathOrUrl.startsWith('/') ? pathOrUrl : `/${pathOrUrl}`;
  return `${protocol}//${window.location.host}${path}`;
}

const defaultTransport: LocalApiTransport = {
  request: (pathOrUrl, init = {}) => fetch(pathOrUrl, init),
  openWebSocket: (pathOrUrl) => new WebSocket(toAbsoluteWsUrl(pathOrUrl)),
};

let transport: LocalApiTransport = defaultTransport;

export function setLocalApiTransport(nextTransport: LocalApiTransport | null) {
  transport = nextTransport ?? defaultTransport;
}

export async function makeLocalApiRequest(
  pathOrUrl: string,
  init: LocalApiRequestOptions = {}
): Promise<Response> {
  return transport.request(pathOrUrl, init);
}

export async function openLocalApiWebSocket(
  pathOrUrl: string
): Promise<WebSocket> {
  return transport.openWebSocket(pathOrUrl);
}
