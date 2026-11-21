import * as assert from 'assert';
import * as crypto from 'crypto';
import { withManifestResponses } from '../manifestResponses';
import {
  DistributionDescriptor,
  resolveDistributionRequest
} from '../../src/distributionDescriptor';
import {
  FetchAdmittedManifest,
  fetchAdmittedManifestForDistribution,
  fetchManifestForDistribution,
  isDirectManifestNotFound,
  ManifestFetchError
} from '../../src/downloader';

const stable: DistributionDescriptor = {
  schema: 1,
  productVersion: '0.11.0',
  channel: 'stable',
  releaseTag: 'v0.11.0',
  releaseRef: 'refs/tags/v0.11.0',
  manifestFile: 'ripr-server-manifest-v0.11.0.json',
  sourceRepository: 'https://github.com/EffortlessMetrics/ripr'
};

const catalog: DistributionDescriptor = {
  ...stable,
  fallbackPlacements: [
    {
      channel: 'rc',
      releaseTag: 'v0.11.0-rc.1',
      releaseRef: 'refs/tags/v0.11.0-rc.1'
    }
  ]
};

function notFound(url: string, redirected: boolean): ManifestFetchError {
  return new ManifestFetchError(`GET ${url} failed with HTTP 404.`, { statusCode: 404, redirected });
}

function producerManifest(overrides: Record<string, unknown> = {}): Buffer {
  const manifest = {
    schema_version: '2',
    product_version: '0.11.0',
    distribution_generation: 'a'.repeat(64),
    source_repository: 'EffortlessMetrics/ripr',
    target_set: {
      targets: ['x86_64-unknown-linux-gnu'],
      digest: 'b'.repeat(64)
    },
    producer: { tool: 'xtask release-server-manifest', schema: 'server-manifest/2' },
    build_identity: {
      repository: 'EffortlessMetrics/ripr',
      candidate_sha: 'c'.repeat(40),
      candidate_tree: 'd'.repeat(40),
      toolchain: '1.95.0',
      toolchain_file_sha256: 'e'.repeat(64),
      cargo_lock_sha256: 'f'.repeat(64),
      profile: 'release',
      features: '',
      locked: true
    },
    assets: {
      'x86_64-unknown-linux-gnu': {
        subject: 'ripr-server-v0.11.0-x86_64-unknown-linux-gnu.tar.gz',
        archive_format: 'tar.gz',
        archive_size: 1234,
        sha256: '1'.repeat(64),
        executable: { path: 'ripr', size: 56, sha256: '2'.repeat(64) },
        receipt: {
          path: 'ripr-server-v0.11.0-x86_64-unknown-linux-gnu.receipt.json',
          sha256: '3'.repeat(64),
          schema_version: '0.2',
          target: 'x86_64-unknown-linux-gnu'
        }
      }
    },
    ...overrides
  };
  return Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
}

suite('Distribution manifest fallback', () => {
  test('real admitted RC bytes pass and changed bytes fail against the same catalog digest', async () => {
    const bytes = producerManifest();
    const digest = crypto.createHash('sha256').update(bytes).digest('hex');
    const distribution = resolveDistributionRequest('0.11.0', {
      ...catalog,
      schema: 2,
      producer: { tool: 'xtask release-distribution-catalog', schema: 'distribution-catalog/1' },
      distributionGeneration: 'a'.repeat(64),
      manifestSha256: digest,
      targetSetDigest: 'b'.repeat(64)
    });
    await withManifestResponses([
      { status: 404, body: Buffer.alloc(0) },
      { status: 200, body: bytes }
    ], async (requested) => {
      // No admitted-fetcher injection: run selection, HTTP status handling,
      // raw-byte digest admission and typed parsing through the default path.
      const admitted = await fetchAdmittedManifestForDistribution('', distribution, '0.11.0', digest);
      assert.strictEqual(admitted.manifest.productVersion, '0.11.0');
      assert.strictEqual(admitted.manifest.assets['x86_64-unknown-linux-gnu'].sha256, '1'.repeat(64));
      assert.strictEqual(admitted.manifestSelection, 'fallback_exact_after_preferred_absent');
      assert.deepStrictEqual(requested, [
        'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0/ripr-server-manifest-v0.11.0.json',
        'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0-rc.1/ripr-server-manifest-v0.11.0.json'
      ]);
    });
    // Valid JSON with different bytes isolates the digest oracle from parsing.
    const changed = Buffer.concat([bytes, Buffer.from(' ')]);
    await withManifestResponses([
      { status: 404, body: Buffer.alloc(0) },
      { status: 200, body: changed }
    ], async (requested) => {
      await assert.rejects(
        fetchAdmittedManifestForDistribution('', distribution, '0.11.0', digest),
        /digest/i
      );
      assert.strictEqual(requested.length, 2);
    });
  });

  test('falls back to the RC placement on a direct 404 of the preferred manifest', async () => {
    const distribution = resolveDistributionRequest('0.11.0', catalog);
    const bytes = Buffer.from(JSON.stringify({ version: '0.11.0', assets: {} }));
    const digest = crypto.createHash('sha256').update(bytes).digest('hex');
    const requested: string[] = [];
    const fetchImpl = async (url: string) => {
      requested.push(url);
      if (requested.length === 1) {
        throw notFound(url, false);
      }
      return bytes;
    };
    const result = await fetchManifestForDistribution('', distribution, '0.11.0', digest, fetchImpl);
    assert.strictEqual(result.manifest.parsed.version, '0.11.0');
    assert.deepStrictEqual(requested, [
      'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0/ripr-server-manifest-v0.11.0.json',
      'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0-rc.1/ripr-server-manifest-v0.11.0.json'
    ]);
  });

  test('admits the exact RC placement with the same embedded digest after direct stable 404', async () => {
    const distribution = resolveDistributionRequest('0.11.0', {
      ...catalog,
      schema: 2,
      producer: { tool: 'xtask release-distribution-catalog', schema: 'distribution-catalog/1' },
      distributionGeneration: 'a'.repeat(64),
      manifestSha256: 'b'.repeat(64),
      targetSetDigest: 'c'.repeat(64)
    });
    const expectedDigest = distribution.manifestSha256 as string;
    const requested: Array<[string, string]> = [];
    const fetchImpl: FetchAdmittedManifest = async (url, digest) => {
      requested.push([url, digest]);
      if (requested.length === 1) {
        throw notFound(url, false);
      }
      return { productVersion: '0.11.0', assets: {} } as never;
    };

    const result = await fetchAdmittedManifestForDistribution(
      '',
      distribution,
      '0.11.0',
      expectedDigest,
      fetchImpl
    );

    assert.strictEqual(
      result.manifestUrl,
      'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0-rc.1/ripr-server-manifest-v0.11.0.json'
    );
    assert.strictEqual(result.manifestSelection, 'fallback_exact_after_preferred_absent');
    assert.strictEqual(result.preferredManifestObservation, 'direct_not_found');
    assert.strictEqual(result.fallbackManifestObservation, 'accepted');
    assert.deepStrictEqual(requested, [
      [
        'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0/ripr-server-manifest-v0.11.0.json',
        expectedDigest
      ],
      [
        'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0-rc.1/ripr-server-manifest-v0.11.0.json',
        expectedDigest
      ]
    ]);
  });

  test('trusted manifest fallback propagates every non-authoritative absence unchanged', async () => {
    const distribution = resolveDistributionRequest('0.11.0', {
      ...catalog,
      schema: 2,
      producer: { tool: 'xtask release-distribution-catalog', schema: 'distribution-catalog/1' },
      distributionGeneration: 'a'.repeat(64),
      manifestSha256: 'b'.repeat(64),
      targetSetDigest: 'c'.repeat(64)
    });
    const failures: Array<[string, unknown]> = [
      ['http 500', new ManifestFetchError('GET https://example.invalid/stable failed with HTTP 500.', { statusCode: 500, redirected: false })],
      ['redirected 404', notFound('https://example.invalid/stable', true)],
      ['transport', new Error('socket hang up')],
      ['digest conflict', new Error('Server manifest SHA-256 does not match the admitted digest.')]
    ];

    for (const [name, failure] of failures) {
      let calls = 0;
      const fetchImpl: FetchAdmittedManifest = async () => {
        calls += 1;
        throw failure;
      };
      await assert.rejects(
        fetchAdmittedManifestForDistribution(
          '',
          distribution,
          '0.11.0',
          distribution.manifestSha256 as string,
          fetchImpl
        ),
        (error: unknown) => error === failure,
        name
      );
      assert.strictEqual(calls, 1, `${name} must not reach the trusted fallback`);
    }
  });

  test('trusted manifest fallback is disabled for an explicit mirror transport', async () => {
    const distribution = resolveDistributionRequest('0.11.0', {
      ...catalog,
      schema: 2,
      producer: { tool: 'xtask release-distribution-catalog', schema: 'distribution-catalog/1' },
      distributionGeneration: 'a'.repeat(64),
      manifestSha256: 'b'.repeat(64),
      targetSetDigest: 'c'.repeat(64)
    });
    const failure = notFound('https://mirror.invalid/ripr/ripr-server-manifest-v0.11.0.json', false);
    let calls = 0;
    const fetchImpl: FetchAdmittedManifest = async () => {
      calls += 1;
      throw failure;
    };

    await assert.rejects(
      fetchAdmittedManifestForDistribution(
        'https://mirror.invalid/ripr',
        distribution,
        '0.11.0',
        distribution.manifestSha256 as string,
        fetchImpl
      ),
      (error: unknown) => error === failure
    );
    assert.strictEqual(calls, 1);
  });

  test('propagates transport, server, redirect, and malformed failures without falling back', async () => {
    const distribution = resolveDistributionRequest('0.11.0', catalog);
    const failures: Array<[string, unknown]> = [
      ['http 500', new ManifestFetchError('GET https://example.invalid/stable failed with HTTP 500.', { statusCode: 500, redirected: false })],
      ['redirected 404', notFound('https://example.invalid/stable', true)],
      ['transport', new Error('socket hang up')],
      ['malformed', new Error('Server manifest is not an object.')]
    ];
    for (const [name, failure] of failures) {
      let calls = 0;
      const fetchImpl = async () => {
        calls += 1;
        throw failure;
      };
      await assert.rejects(
        fetchManifestForDistribution('', distribution, '0.11.0', 'a'.repeat(64), fetchImpl),
        (error: unknown) => error === failure,
        name
      );
      assert.strictEqual(calls, 1, `${name} must not reach the fallback`);
    }
  });

  test('propagates a failing fallback candidate instead of wrapping it', async () => {
    const distribution = resolveDistributionRequest('0.11.0', catalog);
    const serverError = new ManifestFetchError('GET https://example.invalid/rc failed with HTTP 500.', {
      statusCode: 500,
      redirected: false
    });
    const fetchImpl = async (url: string) => {
      if (url.includes('v0.11.0-rc.1')) {
        throw serverError;
      }
      throw notFound(url, false);
    };
    await assert.rejects(
      fetchManifestForDistribution('', distribution, '0.11.0', 'a'.repeat(64), fetchImpl),
      (error: unknown) => error === serverError
    );
  });

  test('an unpinned schema-1 catalogue never follows its named fallback', async () => {
    const failure = notFound('https://example.invalid/legacy', false);
    let calls = 0;
    const fetchImpl = async () => {
      calls += 1;
      throw failure;
    };
    await assert.rejects(
      fetchManifestForDistribution('', resolveDistributionRequest('0.11.0', catalog), '0.11.0', undefined, fetchImpl),
      (error: unknown) => error === failure
    );
    assert.strictEqual(calls, 1);
  });

  test('classifies only direct 404 responses as unpublished placements', () => {
    assert.strictEqual(isDirectManifestNotFound(notFound('https://example.invalid/x', false)), true);
    assert.strictEqual(isDirectManifestNotFound(notFound('https://example.invalid/x', true)), false);
    assert.strictEqual(
      isDirectManifestNotFound(
        new ManifestFetchError('GET https://example.invalid/x failed with HTTP 500.', {
          statusCode: 500,
          redirected: false
        })
      ),
      false
    );
    assert.strictEqual(isDirectManifestNotFound(new Error('socket hang up')), false);
  });
});
