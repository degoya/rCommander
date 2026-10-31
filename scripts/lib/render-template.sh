# shellcheck shell=bash
#
# Fills a packaging template's `@NAME@` placeholders (RC-100-52): what scripts/package-managers.sh,
# scripts/package-deb-rpm.sh and scripts/package-repo.sh share. Copied from rDownloader's
# scripts/lib/render-template.sh (RD-1280-15) at commit 8c30caac8.
#
#   source "$ROOT/scripts/lib/render-template.sh"
#   rc_render_template <template> <output> NAME=value...
#
# A placeholder left over after every pair is an error: the template names a value the caller
# does not know, and a package manifest with `@SHA256@` in it would be published as it is. The
# caller turns off Bash 5.2's patsub_replacement, which would put the matched text in place of a
# `&` in a value.

rc_render_template() {
    local template="$1" output="$2" content pair
    shift 2
    content="$(< "$template")"
    for pair in "$@"; do
        content="${content//@${pair%%=*}@/${pair#*=}}"
    done
    if [[ "$content" =~ @[A-Z0-9_]+@ ]]; then
        echo "error: $template keeps ${BASH_REMATCH[0]} after rendering" >&2
        return 1
    fi
    printf '%s\n' "$content" > "$output"
}
