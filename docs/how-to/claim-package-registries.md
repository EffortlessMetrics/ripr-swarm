# Claim the package identities and prepare publishing

This is the maintainer setup guide for the
[PyPI/npm distribution plan](../PYPI_NPM_DISTRIBUTION.md).
Track non-secret progress in
[ripr #1780](https://github.com/EffortlessMetrics/ripr/issues/1780).
No account, package or environment is created by this document.

## What to claim

| Service | Intended identity | When it is secured |
| --- | --- | --- |
| npm | Organization `effortlessmetrics` | When you control the organization and its scope |
| PyPI | Project `ripr-rs` | When the registry accepts the first genuine publication under your ownership |
| npm | `@effortlessmetrics/ripr` and native packages below | Each exists after its first genuine publication inside your scope |
| TestPyPI | Optional separate project/publisher | Rehearsal only; never reserves production PyPI |

The product and executable remain **ripr**. No repository/editor rename is
needed. The unrelated PyPI project already uses `ripr`; never advertise
`pip install ripr` or `uvx ripr` as installation of this product.

**Availability is not verified.** Registry lookups failed during this planning
pass. An absent page is not assurance that a name is accepted. Confirm ownership
and conflicts in the actual authenticated service UI; do not automatically pick
a different scope or spelling when a check fails.

## 1. Secure npm organization ownership now

Sign in to your individual npm account. Confirm you already own the organization,
or use profile -> **Add an Organization** and choose `effortlessmetrics` with the
public-packages plan. npm organization names define their package scopes;
GitHub organization ownership does not create the npm organization. See
[organization creation](https://docs.npmjs.com/creating-an-organization/) and
[scope ownership](https://docs.npmjs.com/about-scopes/).

Enable strong 2FA, keep recovery material outside repositories and appoint an
appropriate backup owner. Record the public organization URL and ownership
confirmation in #1780, not passwords, tokens, recovery codes or authenticated
page captures.

The intended package inventory is:

```text
@effortlessmetrics/ripr
@effortlessmetrics/ripr-linux-x64-gnu
@effortlessmetrics/ripr-linux-arm64-gnu
@effortlessmetrics/ripr-darwin-x64
@effortlessmetrics/ripr-darwin-arm64
@effortlessmetrics/ripr-win32-x64-msvc
```

You do not need to publish six empty packages to protect these scoped names.
Control the organization, then publish genuine tested packages when ready.
Neither unscoped npm `ripr` nor an npm package called `ripr-rs` is required by
this design. Future target packages stay within the same controlled scope.

## 2. Prepare PyPI; distinguish setup from reservation

Sign in/create your PyPI account, verify email, configure 2FA and retain private
recovery material. Under account **Publishing**, prepare a pending GitHub
publisher for the proposed source workflow:

| Field | Proposed value |
| --- | --- |
| PyPI project | `ripr-rs` |
| GitHub owner | `EffortlessMetrics` |
| Repository | `ripr` |
| Workflow filename | `publish-pypi.yml` |
| GitHub environment | `pypi` |

These workflow/environment names are planned, not implemented by this guide.
Reconcile them with the reviewed source workflow before use. A
[pending publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/)
creates the project only on its first accepted publication. **It does not
reserve the name**, and another registrant can take the name in the meantime.

The [first functional prerelease task](https://github.com/EffortlessMetrics/ripr/issues/1782)
is deliberately separate from full npm/matrix completion. Qualify an honest
platform subset and a useful native CLI, obtain exact source/version/artifact
publication approval, then publish and install it back from production PyPI.
Do not upload a dummy package just to hold the name;
[PEP 541](https://peps.python.org/pep-0541/) treats name-squatting projects as
invalid. A limited real prerelease is different from a placeholder.

Python [name normalization](https://packaging.python.org/en/latest/specifications/name-normalization/)
treats `ripr-rs`, `ripr_rs` and `ripr.rs` as the same normalized project name.
They are not three independent reservations. Optional
[TestPyPI](https://packaging.python.org/en/latest/guides/using-testpypi/)
setup is separate and does not secure production ownership.

## 3. Bind publishers to the source repository

Before first publication, review the proposed source-only environments and
workflow identities under explicit settings authorization:

| Registry | GitHub owner/repository | Workflow filename | Environment |
| --- | --- | --- | --- |
| PyPI | `EffortlessMetrics/ripr` | `publish-pypi.yml` | `pypi` |
| npm | `EffortlessMetrics/ripr` | `publish-npm.yml` | `npm` |

Use filename only in registry forms, not `.github/workflows/...`. Restrict the
source environments to reviewed release refs and approval. Give OIDC permission
only to the publishing job. Build/qualification jobs should not receive registry
write credentials. Never bind either publisher to `ripr-swarm`.

[PyPI publisher use](https://docs.pypi.org/trusted-publishers/using-a-publisher/)
and [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) describe
the provider-specific setup. Saving a form is not proof that publishing works.
After verification, consider disabling unnecessary token-based publication;
revoke only credentials created for this bootstrap, not unrelated existing ones.

## 4. Bootstrap npm, then use staged publishing

The proposed normal release path is:

```text
qualified tarball -> source OIDC staging -> human inspection and 2FA approval
                  -> public hash readback -> clean installed-use check
```

[npm staged publishing](https://docs.npmjs.com/staged-publishing/) cannot create
a brand-new package. First publish each inspected real native tarball, then the
launcher, through a separately authorized maintainer bootstrap with explicit
public access and a prerelease tag such as `next`. Keep that tag off stable
`latest`. Do not claim OIDC provenance for a manual bootstrap without evidence.

After each package exists, configure its own trusted publisher with the source
tuple above. Prefer **stage-only** permission rather than silently allowing
direct publication. Review/approve and verify native packages before approving
the launcher that references them. A staged package is not yet public.

As checked on 2026-09-28, staging requires npm >=11.15.0 and Node >=22.14.0;
GitHub OIDC publishing uses GitHub-hosted runners. Pin reviewed compatible tools
in the implementation. Those publisher minimums do not establish consumer
runtime minimums. OIDC does not authorize general npm administration or human
stage approval; `npm whoami` is not an OIDC readiness check.

The [publishing issue](https://github.com/EffortlessMetrics/ripr/issues/1781)
owns exact-artifact authorization, staged/public state, dist-tag decisions,
partial publication and fix-forward. Do not retry a conflicting immutable
package with different bytes or publish a launcher before its payloads exist.

## Record progress without secrets

Keep account readiness, scope ownership, pending/configured publishers, project
creation, publication and independent verification separate. Record public URLs,
owner confirmation, exact non-secret publisher tuples, selected artifact hashes
and verification receipts in the source issues. Never attach tokens, recovery
codes, credentials, full environment dumps or authenticated HTML captures.

The immediate parallel actions are npm scope ownership and PyPI account/pending
publisher preparation. The name-acquisition milestone is the first **real** PyPI
prerelease. No broader release, unrelated setting change or publication is
authorized merely by completing this checklist.
