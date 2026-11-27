#!/bin/sh
# Whether a commit's checks are green on all three systems, before anything is built from it to
# share. The release workflow (release.yml) runs it on the tagged commit before it builds
# anything. Development builds have the same rule, in the script that publishes them.
#
#   .github/scripts/checks-green.sh <owner/repo> <commit> [minutes]
#
# Green means:
# - the commit is on master;
# - the newest run for a push of each of the Checks (ci.yml), Windows (windows.yml) and Linux
#   (linux.yml) workflows on that commit concluded "success";
# - every check another app reported on it (none do today) passed.
#
# While a check is still running it waits, looking again every minute, for up to that many
# minutes (none without), then fails, naming what was still running. A check that failed, or
# was cancelled, fails it at once, naming the workflow and the address of its run.
#
# A commit with no run of one of the three fails after five minutes. That happens when it was
# pushed together with later commits (GitHub runs a push's checks on its last commit only), when
# a newer push took its run's place in the queue, or when it was never pushed to master.
#
# Needs gh, signed in or given GH_TOKEN; it only reads.
set -eu

if [ $# -lt 2 ] || [ $# -gt 3 ]; then
    echo "usage: $0 <owner/repo> <commit> [minutes]" >&2
    exit 2
fi
repo=$1
sha=$2
minutes=${3:-0}
case "$sha" in
*[!0-9a-f]* | "") echo "error: \"$sha\" isn't a full commit id (40 hex digits)" >&2; exit 2 ;;
esac
if [ ${#sha} -ne 40 ]; then
    echo "error: \"$sha\" isn't a full commit id (40 hex digits)" >&2
    exit 2
fi
case "$minutes" in
"" | *[!0-9]*) echo "error: the minutes to wait are a whole number, not \"$minutes\"" >&2; exit 2 ;;
esac

# The workflows that must have passed: their files, and the names they go by.
required="ci.yml=Checks windows.yml=Windows linux.yml=Linux"
tab=$(printf '\t')

# A problem: an annotation on a GitHub run, or a line on standard error elsewhere.
problem() {
    if [ -n "${GITHUB_ACTIONS:-}" ]; then
        echo "::error title=$1::$2"
    else
        echo "checks: $1: $2" >&2
    fi
}

# gh api, tried three times: a long wait shouldn't end on one lost answer.
ask() {
    for try in 1 2 3; do
        if gh api "$@"; then return 0; fi
        if [ "$try" != 3 ]; then sleep 10; fi
    done
    return 1
}

if ! where=$(ask "repos/$repo/compare/master...$sha?per_page=1" --jq .status); then
    problem "GitHub didn't answer" "couldn't find $sha in $repo, or GitHub didn't answer (above)"
    exit 1
fi
case "$where" in
identical | behind) ;;
*)
    problem "Not on master" "$sha isn't on $repo's master (GitHub says it is $where of master). Only a commit pushed to master, whose checks ran there, is built from."
    exit 1
    ;;
esac

start=$(date +%s)
while :; do
    waited=$((($(date +%s) - start) / 60))
    # The newest run for a push of each workflow on the commit (a run started again is the same
    # run, with its newest attempt's result): file, name, status, conclusion, address.
    if ! runs=$(ask "repos/$repo/actions/runs?head_sha=$sha&event=push&per_page=100" --jq '
        .workflow_runs | group_by(.path) | map(max_by(.created_at)) | .[]
        | [(.path | ltrimstr(".github/workflows/")), .name, .status, (.conclusion // "none"), .html_url]
        | @tsv'); then
        problem "GitHub didn't answer" "couldn't list the workflow runs on $sha (above)"
        exit 1
    fi
    # What apps other than GitHub Actions reported: app, name, status, conclusion, address.
    if ! others=$(ask "repos/$repo/commits/$sha/check-runs?per_page=100" --jq '
        .check_runs[] | select(.app.slug != "github-actions")
        | [.app.slug, .name, .status, (.conclusion // "none"), .html_url]
        | @tsv'); then
        problem "GitHub didn't answer" "couldn't list the checks on $sha (above)"
        exit 1
    fi

    passed=""
    failed=""
    running=""
    missing=""
    for want in $required; do
        file=${want%%=*}
        name=${want#*=}
        line=$(printf '%s\n' "$runs" | awk -F "$tab" -v file="$file" '$1 == file { print; exit }')
        if [ -z "$line" ]; then
            missing="$missing $name"
            continue
        fi
        status=$(printf '%s\n' "$line" | cut -f3)
        conclusion=$(printf '%s\n' "$line" | cut -f4)
        url=$(printf '%s\n' "$line" | cut -f5)
        if [ "$status" != completed ]; then
            running="$running $name ($status, $url);"
        elif [ "$conclusion" = success ]; then
            passed="$passed$name: success, $url
"
        else
            failed="$failed$name$tab$conclusion$tab$url
"
        fi
    done
    while IFS="$tab" read -r app name status conclusion url; do
        if [ -z "$app" ]; then continue; fi
        if [ "$status" != completed ]; then
            running="$running $name from $app ($status, $url);"
        elif [ "$conclusion" = success ] || [ "$conclusion" = skipped ]; then
            passed="$passed$name from $app: $conclusion, $url
"
        else
            failed="$failed$name from $app$tab$conclusion$tab$url
"
        fi
    done <<EOF
$others
EOF

    if [ -n "$failed" ]; then
        printf '%s' "$failed" | while IFS="$tab" read -r name conclusion url; do
            problem "$name: $conclusion" "the $name check on $sha concluded \"$conclusion\": $url"
        done
        echo "Not green: nothing is built from $sha."
        exit 1
    fi
    if [ -z "$running" ] && [ -z "$missing" ]; then
        printf '%s' "$passed"
        echo "Every check on $sha passed."
        exit 0
    fi
    # No run at all is waited for five minutes at most: a push's runs appear within seconds.
    if [ -n "$missing" ] && { [ "$waited" -ge 5 ] || [ "$waited" -ge "$minutes" ]; }; then
        for name in $missing; do
            problem "No $name run" "the $name workflow has no run for a push of $sha. It runs on a push's last commit only, and a newer push can take a queued run's place: build from a commit whose checks ran (master's newest, once its checks are done)."
        done
        exit 1
    fi
    if [ "$waited" -ge "$minutes" ]; then
        problem "Checks still running" "after $waited minutes, still running on $sha:$running"
        exit 1
    fi
    echo "Waiting for the checks on $sha ($waited of $minutes minutes):$running${missing:+ no run yet:$missing}"
    sleep 60
done
