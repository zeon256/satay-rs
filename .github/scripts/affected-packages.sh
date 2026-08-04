#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ "${1:-}" == "--files" ]]; then
  shift
  changed_json="$(printf '%s\n' "$@" | jq -Rsc 'split("\n") | map(select(length > 0))')"
elif [[ $# -eq 2 ]]; then
  changed_json="$(git diff --name-only -z "$1" "$2" | jq -Rsc 'split("\u0000") | map(select(length > 0))')"
else
  echo "usage: $0 <base-revision> <head-revision>" >&2
  echo "       $0 --files <path>..." >&2
  exit 2
fi

metadata_json="$(cargo metadata --format-version 1 --no-deps)"

jq -cn \
  --argjson metadata "$metadata_json" \
  --argjson changed "$changed_json" '
    def documentation_file:
      endswith(".md")
      or test("(^|/)(LICENSE|NOTICE)(\\.[^/]*)?$");

    def owner($packages; $file):
      [
        $packages[]
        | . as $package
        | select(
            $file == $package.directory
            or ($file | startswith($package.directory + "/"))
          )
      ]
      | sort_by(.directory | length)
      | last;

    def closure($packages; $selected):
      [
        $packages[]
        | . as $package
        | select(
            (($selected | index($package.name)) != null)
            or any(
              $package.dependencies[];
              . as $dependency
              | ($selected | index($dependency)) != null
            )
          )
        | .name
      ]
      | unique as $next
      | if ($next | length) == ($selected | length) then
          $next
        else
          closure($packages; $next)
        end;

    ($metadata.packages
      | map(
          select(
            .id as $id
            | ($metadata.workspace_members | index($id)) != null
          )
          | . + {
              directory_absolute: (.manifest_path | sub("/Cargo.toml$"; ""))
            }
        )) as $workspace_packages
    | ($workspace_packages
      | map({ key: .directory_absolute, value: .name })
      | from_entries) as $names_by_directory
    | ($workspace_packages
      | map({
          name,
          directory: (
            .directory_absolute
            | ltrimstr($metadata.workspace_root + "/")
          ),
          dependencies: [
            .dependencies[]
            | select(.path != null)
            | .path as $path
            | $names_by_directory[$path] // empty
          ]
        })) as $packages
    | ($changed | map(select(documentation_file | not))) as $relevant_files
    | ([
        $relevant_files[] as $file
        | owner($packages; $file)
        | select(. != null)
        | .name
      ] | unique) as $direct_packages
    | ([
        $relevant_files[] as $file
        | select(owner($packages; $file) == null)
        | $file
      ] | unique) as $unowned_files
    | (any(
        $unowned_files[];
        . == "Cargo.toml"
        or . == "Cargo.lock"
        or startswith(".cargo/")
        or startswith("rust-toolchain")
        or . == ".github/workflows/qc.yml"
        or startswith(".github/actions/setup-rust/")
        or startswith(".github/scripts/")
        or (
          (startswith(".github/") | not)
          and (startswith("crates/satay-oas3/") | not)
          and (
            (
              startswith("examples/")
              and (endswith(".yaml") or endswith(".yml"))
            )
            | not
          )
        )
      )) as $affects_all
    | (any(
        $unowned_files[];
        startswith("crates/satay-oas3/")
      )) as $affects_fork
    | (any(
        $unowned_files[];
        startswith("examples/")
        and (endswith(".yaml") or endswith(".yml"))
      )) as $affects_examples
    | ($packages | map(.name)) as $all_packages
    | (
        if $affects_all then
          $all_packages
        else
          $direct_packages
          + if $affects_fork then
              ["satay-oas3", "roast", "satay-oas3-integration-tests"]
            else
              []
            end
          + if $affects_examples then
              [$all_packages[] | select(startswith("satay-example-"))]
            else
              []
            end
        end
        | unique
      ) as $direct
    | closure($packages; $direct) as $affected
    | ["satay-oas3", "roast", "satay-oas3-integration-tests"] as $fork_names
    | {
        changed_files: $changed,
        format_packages: $direct,
        packages: $affected,
        satay_packages: [
          $affected[] as $name
          | select(($fork_names | index($name)) == null)
          | $name
        ],
        fork_support_packages: [
          $affected[]
          | select(. == "roast" or . == "satay-oas3-integration-tests")
        ],
        has_packages: (($affected | length) > 0),
        has_satay_packages: (
          any(
            $affected[];
            . as $name
            | ($fork_names | index($name)) == null
          )
        ),
        has_oas3_parser: (($affected | index("satay-oas3")) != null)
      }
  '
