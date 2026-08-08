/**
 * Exception hierarchy for the Ghostlink client.
 *
 * Mirrors `sdks/python/ghostlink_client/exceptions.py`:
 *   - `GhostlinkError` — base class for everything this client throws.
 *   - `GhostlinkConnectionError` — the client could not reach the server at
 *     all (DNS failure, connection refused, or a timeout before any response
 *     arrived).
 *   - `GhostlinkAPIError` — the server responded with a non-2xx status code.
 *     Carries `statusCode` and `body` (the parsed JSON error body, or raw
 *     text if the body wasn't JSON).
 *   - `GhostlinkAuthError` — a `GhostlinkAPIError` specifically for 401/403
 *     (missing, invalid, or expired bearer token).
 */

/** Base class for all errors raised by this client. */
export class GhostlinkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "GhostlinkError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

/**
 * The client could not reach the Ghostlink server at all — DNS failure,
 * connection refused, or a timeout before any response arrived.
 */
export class GhostlinkConnectionError extends GhostlinkError {
  constructor(message: string) {
    super(message);
    this.name = "GhostlinkConnectionError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

/** The server responded with a non-2xx status code. */
export class GhostlinkAPIError extends GhostlinkError {
  /** The HTTP status code returned by the server. */
  readonly statusCode: number;
  /** The parsed JSON error body, or raw response text if it wasn't JSON. */
  readonly body: unknown;

  constructor(statusCode: number, message: string, body?: unknown) {
    super(`HTTP ${statusCode}: ${message}`);
    this.name = "GhostlinkAPIError";
    this.statusCode = statusCode;
    this.body = body;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

/** 401/403 — missing, invalid, or expired bearer token. */
export class GhostlinkAuthError extends GhostlinkAPIError {
  constructor(statusCode: number, message: string, body?: unknown) {
    super(statusCode, message, body);
    this.name = "GhostlinkAuthError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}
