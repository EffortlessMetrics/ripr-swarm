#!/usr/bin/env bash
# Refuse narrative or superseded source-review attestation (#8157).
set -euo pipefail
trusted_root=${1:?trusted admission root}
repository=${2:?repository}
pr_number=${3:?pr number}
expected_head=${4:?expected head}
author=${5:-}
pr=$(gh api "repos/$repository/pulls/$pr_number")
if [ -z "$author" ]; then
  author=$(jq -r '.user.login' <<<"$pr")
fi
reviews=$(gh api --paginate "repos/$repository/pulls/$pr_number/reviews?per_page=100")
jq -cs '.[0]' <<<"$reviews" | python3 "$trusted_root/.github/actions/pr-candidate-admission/review_disposition.py" \
  --check-reviews /dev/stdin --head "$expected_head" --author "$author"
