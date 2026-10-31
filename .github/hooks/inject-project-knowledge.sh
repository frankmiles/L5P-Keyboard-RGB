#!/bin/sh
set -eu

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
knowledge_file="$script_dir/../../PROJECT_KNOWLEDGE.md"

if [ ! -f "$knowledge_file" ]; then
    printf 'Project knowledge document not found: %s\n' "$knowledge_file" >&2
    exit 2
fi

escaped_content=$(
    sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' "$knowledge_file" |
        awk '{ if (NR > 1) printf "\\n"; printf "%s", $0 }'
)

printf '{"systemMessage":"Project reference loaded from PROJECT_KNOWLEDGE.md. Use its search index to locate relevant facts and verify them against the current source when needed.\\n\\n%s"}\n' "$escaped_content"
