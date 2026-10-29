#!/bin/sh
# Sign Karakulak's extensions on addons.mozilla.org (unlisted) and put the
# signed .xpi files into the image. Only extensions whose manifest version
# differs from the signed .xpi are touched. Versions held for manual review
# are skipped and picked up on a later run once Mozilla approves them.
#
# First time only, save your AMO API key
# (https://addons.mozilla.org/developers/addon/api/key/):
#   extensions/sign.sh --setup
#
# Then: extensions/sign.sh            (all extensions)
#       extensions/sign.sh panel      (one extension)
# Bump "version" in extensions/<name>/ext/manifest.json to release a change.
#
# CI passes AMO_JWT_ISSUER and AMO_JWT_SECRET as environment variables.
set -e

config="${XDG_CONFIG_HOME:-$HOME/.config}/karakulak/amo.env"
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/config/includes.chroot/usr/share/karakulak/extensions"
api="https://addons.mozilla.org/api/v5"

if [ "$1" = "--setup" ]; then
	printf "JWT issuer: "
	read -r issuer
	printf "JWT secret: "
	stty -echo
	read -r secret
	stty echo
	echo
	mkdir -p "$(dirname "$config")"
	(umask 077 && printf "AMO_JWT_ISSUER='%s'\nAMO_JWT_SECRET='%s'\n" "$issuer" "$secret" > "$config")
	echo "Saved: $config (only you can read it)"
	exit 0
fi

if [ -f "$config" ]; then
	. "$config"
fi
if [ -z "$AMO_JWT_ISSUER" ] || [ -z "$AMO_JWT_SECRET" ]; then
	echo "No AMO key found. Run once: extensions/sign.sh --setup" >&2
	exit 1
fi

base64url() {
	openssl base64 -A | tr '+/' '-_' | tr -d '='
}

# Short-lived JWT for the AMO API, signed with the secret.
token() {
	now=$(date +%s)
	header=$(printf '{"alg":"HS256","typ":"JWT"}' | base64url)
	payload=$(printf '{"iss":"%s","jti":"%s","iat":%s,"exp":%s}' \
		"$AMO_JWT_ISSUER" "$(openssl rand -hex 16)" "$now" "$((now + 60))" | base64url)
	signature=$(printf '%s.%s' "$header" "$payload" |
		openssl dgst -sha256 -hmac "$AMO_JWT_SECRET" -binary | base64url)
	printf '%s.%s.%s' "$header" "$payload" "$signature"
}

amo() {
	curl -sS -H "Authorization: JWT $(token)" "$@"
}

submit() {
	name="$1"
	"$root/extensions/build.sh" "$name"
	tmp=$(mktemp -d)
	# Credentials go through the environment, not argv, so they stay out of ps.
	if WEB_EXT_API_KEY="$AMO_JWT_ISSUER" WEB_EXT_API_SECRET="$AMO_JWT_SECRET" \
		npx --yes web-ext@8 sign \
		--source-dir "$root/extensions/$name/ext" \
		--artifacts-dir "$tmp" \
		--channel unlisted \
		--approval-timeout "${SIGN_APPROVAL_TIMEOUT:-600000}"; then
		mv "$tmp"/*.xpi "$out/$name.xpi"
		echo "Signed: $name"
	else
		echo "Not signed yet: $name (submitted; run again after Mozilla approves it)" >&2
	fi
	rm -r "$tmp"
}

sign_one() {
	name="$1"
	manifest="$root/extensions/$name/ext/manifest.json"
	id=$(jq -r '.browser_specific_settings.gecko.id' "$manifest")
	want=$(jq -r '.version' "$manifest")
	have=$(unzip -p "$out/$name.xpi" manifest.json 2>/dev/null | jq -r '.version' 2>/dev/null || true)

	if [ "$want" = "$have" ]; then
		echo "Up to date: $name $want"
		return
	fi

	status=$(amo -o /dev/null -w '%{http_code}' "$api/addons/addon/$id/versions/v$want/")
	if [ "$status" = "404" ]; then
		submit "$name"
		return
	fi

	version=$(amo "$api/addons/addon/$id/versions/v$want/")
	if [ "$(echo "$version" | jq -r '.file.status')" = "public" ]; then
		amo -L -o "$out/$name.xpi" "$(echo "$version" | jq -r '.file.url')"
		echo "Downloaded approved: $name $want"
	else
		echo "Still waiting for Mozilla review: $name $want" >&2
	fi
}

mkdir -p "$out"
if [ -n "$1" ]; then
	sign_one "$1"
else
	for manifest in "$root"/extensions/*/ext/manifest.json; do
		sign_one "$(basename "$(dirname "$(dirname "$manifest")")")"
	done
fi
