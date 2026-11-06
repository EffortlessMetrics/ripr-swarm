//! Independently retained complete control-step source from reviewed 6261cc3.
//! This is a test oracle, not executed shell or generated from CI at test time.

pub(super) const SURFACE_STEP: &str = r####"        name: Detect Rust surface change
        id: surface
        env:
          EVENT_NAME: ${{ github.event_name }}
          CALLED: ${{ contains(toJSON(inputs), '"subject_sha"') }}
          BASE_SHA: ${{ github.event.pull_request.base.sha }}
          HEAD_SHA: ${{ github.event.pull_request.head.sha }}
        run: |
          rust=true reason=not_pull_request
          classify() {
            [ "${CALLED:-false}" != true ] || return 0
            [ "$EVENT_NAME" = "pull_request" ] || return 0
            reason=missing_identity
            [ -n "$BASE_SHA" ] && [ -n "$HEAD_SHA" ] || return 0
            reason=invalid_identity
            [[ "$BASE_SHA" =~ ^[0-9a-f]{40}$ ]] && [[ "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]] || return 0
            # Classify only the event's PR contribution; keep the integration checkout for tests.
            reason=unavailable_history
            if ! git cat-file -e "$BASE_SHA^{commit}" 2>/dev/null || ! git cat-file -e "$HEAD_SHA^{commit}" 2>/dev/null; then
              git fetch --no-tags origin "$BASE_SHA" "$HEAD_SHA" 2>/dev/null || return 0
            fi
            [ "$(git rev-parse --is-shallow-repository)" = "false" ] || return 0
            [ "$(git cat-file -t "$BASE_SHA" 2>/dev/null)" = "commit" ] && [ "$(git cat-file -t "$HEAD_SHA" 2>/dev/null)" = "commit" ] || return 0
            reason=unavailable_merge_base
            merge_bases="$(git merge-base --all "$BASE_SHA" "$HEAD_SHA" 2>/dev/null)" || return 0
            [ -n "$merge_bases" ] || return 0
            reason=ambiguous_merge_base
            [[ "$merge_bases" != *$'\n'* ]] || return 0
            reason=temporary_file_failed
            changed="$(mktemp)" || return 0
            trap 'rm -f "$changed"' EXIT
            reason=diff_failed
            # NUL records preserve filenames; no rename collapsing hides a removed Rust path.
            git diff --no-renames --name-only -z "$BASE_SHA"..."$HEAD_SHA" > "$changed" 2>/dev/null || return 0
            reason=empty_diff
            [ -s "$changed" ] || return 0
            reason=rust_surface
            while IFS= read -r -d '' path; do
              case "$path" in
                crates/*|fixtures/*|Cargo.toml|Cargo.lock|rust-toolchain*|.cargo/*) return 0 ;;
              esac
            done < "$changed"
            rust=false reason=no_rust_surface
          }
          classify
          echo "rust=$rust" >> "$GITHUB_OUTPUT"
          { echo "### Rust-Only Surface Detection"; echo ""; echo "- rust: \`$rust\`"; echo "- reason: \`$reason\`"; } >> "$GITHUB_STEP_SUMMARY""####;

pub(super) const AGGREGATE_STEP: &str = r####"        name: Require every selected child
        env:
          IDENTITY_RESULT: ${{ needs.source-identity.result }}
          SELECT_RUST: ${{ inputs.run_rust_only }}
          SELECT_RELEASE: ${{ inputs.run_release_proof }}
          SELECT_MSRV: ${{ inputs.run_msrv }}
          SELECT_VSCODE: ${{ inputs.run_vscode }}
          RUST_RESULT: ${{ needs.rust-only-features.result }}
          RELEASE_RESULT: ${{ needs.release-proof.result }}
          MSRV_RESULT: ${{ needs.msrv.result }}
          VSCODE_RESULT: ${{ needs.vscode.result }}
        run: |
          set -euo pipefail
          [[ "$IDENTITY_RESULT" == success ]] || exit 1
          require() {
            case "$1:$2" in true:success|false:skipped) ;; *) echo 'Missing/failed or unexpected CI child' >&2; exit 1 ;; esac
          }
          require "$SELECT_RUST" "$RUST_RESULT"
          require "$SELECT_RELEASE" "$RELEASE_RESULT"
          require "$SELECT_MSRV" "$MSRV_RESULT"
          require "$SELECT_VSCODE" "$VSCODE_RESULT""####;
