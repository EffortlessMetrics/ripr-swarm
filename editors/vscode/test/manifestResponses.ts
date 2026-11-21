import * as assert from 'assert';
import { EventEmitter } from 'events';
import { ClientRequest, IncomingMessage } from 'http';
import https = require('https');
import { PassThrough } from 'stream';

export async function withManifestResponses(
  responses: Array<{ status: number; body: Buffer }>,
  run: (requested: string[]) => Promise<void>
): Promise<void> {
  const original = https.get;
  const requested: string[] = [];
  https.get = ((url: string, callback: (response: IncomingMessage) => void) => {
    requested.push(url);
    const selected = responses[requested.length - 1];
    assert.ok(selected, `unexpected request ${url}`);
    const request = new EventEmitter() as ClientRequest;
    request.setTimeout = () => request;
    request.destroy = (error?: Error) => {
      if (error) { request.emit('error', error); }
      return request;
    };
    process.nextTick(() => {
      const stream = new PassThrough();
      const response = stream as unknown as IncomingMessage;
      response.statusCode = selected.status;
      response.headers = {};
      callback(response);
      stream.end(selected.body);
    });
    return request;
  }) as typeof https.get;
  try {
    await run(requested);
  } finally {
    https.get = original;
  }
}
